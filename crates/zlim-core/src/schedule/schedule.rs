#![expect(clippy::module_inception, reason = "For better structure.")]

use core::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap};

use zlim_log as log;
use zlim_utils::debug::DebugLocation;
use zlim_utils::hash::{HashMap, HashSet, NoopState, SparseState};
use zlim_utils::mem::Bump;

use super::{ConflictTable, ExecutorKind, JobSchedule};
use super::{Dag, Node, ToposortError};
use super::{InternedScheduleLabel, JobExecutor, ScheduleLabel, ScheduleStage};
use super::{MultiThreadedExecutor, SingleThreadedExecutor};
use crate::job::{Job, JobDB, JobGroup, JobGroupLabel, JobId, JobLabel};
use crate::system::{AccessTable, SystemError, SystemFlags};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// Types

// -------------------------------------------------------------

struct JobEntry {
    node: Node,
    stage: Option<&'static str>,
    object: Option<Box<dyn Job>>,
    access: Option<AccessTable>,
}

// -------------------------------------------------------------

#[derive(Default)]
struct Jobs {
    jobs: Vec<JobEntry>,
    nodes: HashMap<JobId, Node, NoopState>,
    idents: HashMap<Node, JobId, SparseState>,
    uninit: Vec<Node>,
    reusable: BinaryHeap<Reverse<u16>>,
}

// -------------------------------------------------------------

struct GroupEntry {
    group: JobGroup,
    stage: Option<&'static str>,
}

type Groups = HashMap<&'static str, GroupEntry>;

// -------------------------------------------------------------

struct StageEntry {
    begin: Node,
    end: Node,
    jobs: BTreeSet<Node>,
    groups: BTreeSet<&'static str>,
}

type Stages = HashMap<&'static str, StageEntry>;

// -------------------------------------------------------------

#[derive(Default)]
struct Hierarchy {
    parents: BTreeSet<Node>,
    children: BTreeSet<Node>,
}

type Hierarchies = HashMap<Node, Hierarchy>;

// -------------------------------------------------------------

#[derive(Default)]
struct OrderingGraph {
    order: Dag,
    weak_order: Dag,
}

// -------------------------------------------------------------

#[derive(Default)]
struct ConflictGraph {
    exclusive: HashSet<Node>,
    conflicts: HashMap<Node, HashSet<Node>>,
}

// -----------------------------------------------------------------------------
// Jobs Methods

impl Jobs {
    /// Returns the job id registered at `node`, if the tag is still valid.
    fn get_id(&self, node: Node) -> Option<JobId> {
        self.idents.get(&node).copied()
    }

    /// Returns the node of the job, if it is registered.
    fn get_node(&self, id: JobId) -> Option<Node> {
        self.nodes.get(&id).copied()
    }

    /// Returns whether the job is registered.
    fn contains(&self, id: JobId) -> bool {
        self.nodes.contains_key(&id)
    }
}

impl Jobs {
    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    fn insert(&mut self, id: JobId, job: Box<dyn Job>) -> Node {
        if let Some(&node) = self.nodes.get(&id) {
            ::core::hint::cold_path(); // unreachable ?
            let entry = &mut self.jobs[node.index()];
            debug_assert_eq!(entry.node, node);
            return entry.node;
        }

        if let Some(Reverse(idx)) = self.reusable.pop() {
            let entry = &mut self.jobs[idx as usize];
            entry.node.tag = entry.node.tag.wrapping_add(1);
            entry.object = Some(job);
            entry.access = None;
            entry.stage = None;
            let node = entry.node;
            self.uninit.push(node);
            self.nodes.insert(id, node);
            self.idents.insert(node, id);
            return node;
        }

        assert! {
            self.jobs.len() < u16::MAX as usize,
            "too many Job in a Schedule, cannot exceed u16::MAX",
        }

        let idx = self.jobs.len() as u16;
        let node = Node { idx, tag: 1 };
        self.jobs.push(JobEntry {
            node,
            stage: None,
            object: Some(job),
            access: None,
        });

        self.uninit.push(node);
        self.nodes.insert(id, node);
        self.idents.insert(node, id);
        node
    }

    fn remove(&mut self, id: JobId) -> Option<Node> {
        let node = self.nodes.remove(&id)?;
        self.idents.remove(&node);

        let entry = &mut self.jobs[node.index()];
        entry.node.tag = entry.node.tag.wrapping_add(1);
        entry.stage = None;
        entry.object = None;
        entry.access = None;

        // The slot no longer needs initialization; drop any pending entry.
        if let Some(index) = self.uninit.iter().position(|&n| n == node) {
            self.uninit.swap_remove(index);
        }

        self.reusable.push(Reverse(node.idx));
        Some(node)
    }

    fn stage(&self, node: Node) -> Option<&'static str> {
        let entry = self.jobs.get(node.index())?;
        (entry.node.tag == node.tag).then_some(entry.stage)?
    }

    fn get_job(&self, node: Node) -> Option<&dyn Job> {
        let entry = self.jobs.get(node.index())?;
        (entry.node.tag == node.tag).then_some(entry.object.as_deref())?
    }

    fn get_job_mut(&mut self, node: Node) -> Option<&mut dyn Job> {
        let entry = self.jobs.get_mut(node.index())?;
        (entry.node.tag == node.tag).then_some(entry.object.as_deref_mut())?
    }

    fn get_entry_mut(&mut self, node: Node) -> Option<&mut JobEntry> {
        let entry = self.jobs.get_mut(node.index())?;
        (entry.node.tag == node.tag).then_some(entry)
    }

    fn access_table(&self, node: Node) -> Option<&AccessTable> {
        let entry = self.jobs.get(node.index())?;
        (entry.node.tag == node.tag).then_some(entry.access.as_ref())?
    }

    fn set_access_table(&mut self, node: Node, access: AccessTable) {
        let entry = &mut self.jobs[node.index()];
        debug_assert_eq!(entry.node.tag, node.tag);
        entry.access = Some(access);
    }

    fn take(&mut self, node: Node) -> (Box<dyn Job>, AccessTable) {
        let entry = &mut self.jobs[node.index()];
        debug_assert_eq!(entry.node.tag, node.tag);
        (
            entry.object.take().expect("job should be present"),
            entry.access.take().expect("job should be initialized"),
        )
    }

    fn recycle(&mut self, node: Node, job: Box<dyn Job>, access: AccessTable) {
        let entry = &mut self.jobs[node.index()];
        if entry.node.tag != node.tag {
            return; // already be removed
        }
        entry.object = Some(job);
        entry.access = Some(access);
    }

    fn present_nodes(&self) -> Vec<Node> {
        use core::hint::assert_unchecked;
        let mut buf = Vec::with_capacity(self.jobs.len());
        for entry in &self.jobs {
            if entry.object.is_some() {
                unsafe {
                    assert_unchecked(buf.len() < buf.capacity());
                    buf.push(entry.node);
                }
            }
        }
        buf
    }
}

// -----------------------------------------------------------------------------
// OrderingGraph Implementation

impl OrderingGraph {
    fn insert_weak_order(&mut self, before: Node, after: Node) {
        self.weak_order.insert_edge(before, after);
    }

    fn insert_strong_order(&mut self, before: Node, after: Node) {
        self.order.insert_edge(before, after);
    }

    fn insert_node(&mut self, node: Node) {
        self.weak_order.insert_node(node);
        self.order.insert_node(node); // optional
    }

    fn remove_node(&mut self, node: Node) {
        self.weak_order.remove_node(node);
        self.order.remove_node(node);
    }
}

// -----------------------------------------------------------------------------
// ConflictTable Implementation

impl ConflictGraph {
    fn set_exclusive(&mut self, node: Node) {
        self.exclusive.insert(node);
    }

    fn set_conflict(&mut self, a: Node, b: Node) {
        self.conflicts.entry(a).or_default().insert(b);
        self.conflicts.entry(b).or_default().insert(a);
    }

    fn is_exclusive(&self, node: Node) -> bool {
        self.exclusive.contains(&node)
    }

    fn is_conflict(&self, a: Node, b: Node) -> bool {
        self.conflicts.get(&a).is_some_and(|set| set.contains(&b))
    }

    fn remove(&mut self, node: Node) {
        self.exclusive.remove(&node);
        if let Some(a_set) = self.conflicts.remove(&node) {
            for b in a_set.iter() {
                if let Some(b_set) = self.conflicts.get_mut(b) {
                    b_set.remove(&node);
                }
            }
        }
    }
}

// -----------------------------------------------------------------------------
// SyncPoint

struct SyncPoint {
    id: JobId,
    last_run: Tick,
}

impl Job for SyncPoint {
    #[inline]
    fn id(&self) -> JobId {
        self.id
    }

    #[inline]
    fn flags(&self) -> SystemFlags {
        SystemFlags::NO_OP.union(SystemFlags::EXCLUSIVE)
    }

    #[inline]
    fn last_run(&self) -> Tick {
        self.last_run
    }

    #[inline]
    fn clamp_ticks(&mut self, now: Tick) {
        self.last_run.clamp_with(now);
    }

    #[inline]
    fn set_last_run(&mut self, last_run: Tick) {
        self.last_run = last_run;
    }

    #[inline]
    fn initialize(&mut self, world: &World) {
        self.last_run = world.last_run();
    }

    #[inline]
    fn register_access(&self, _: &mut AccessTable) {}

    #[inline]
    unsafe fn run_raw(&mut self, _: WorldCell<'_>) -> Result<(), SystemError> {
        Ok(())
    }

    #[inline]
    fn apply_deferred(&mut self, _: &mut World) {}
}

impl SyncPoint {
    #[inline]
    fn new(name: JobId, after: JobId) -> Self {
        let buf = format!("#Sync<{name}, {after}>");
        let this_name = zlim_utils::str::intern_str(&buf);
        let id = JobId::isolated(this_name);
        Self {
            id,
            last_run: Tick::new(0),
        }
    }
}

// -----------------------------------------------------------------------------

/// A collection of jobs and job groups that can be executed together.
///
/// A `Schedule` resolves job/group insertions into a dependency graph (strong
/// and weak ordering edges plus access-conflict edges) and rebuilds an
/// executor-ready [`JobSchedule`] whenever its contents change.
///
/// Jobs are inserted either individually (by name or by [`JobLabel`]) or as
/// whole groups (by name or by [`JobGroupLabel`]); groups additionally carry
/// ordering constraints between their jobs.  Execution happens through a
/// [`JobExecutor`] — by default the platform's [`ExecutorKind`], but any
/// executor can be plugged in via [`Schedule::with_executor`].
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// enum MainSchedule {
///     Update,
/// }
///
/// #[job_fn(type = PhysicsStep, name = "physics_step")]
/// fn physics_step() {}
///
/// #[job_fn(type = RenderStep, name = "render_step")]
/// fn render_step() {}
///
/// let mut schedule = Schedule::new(MainSchedule::Update);
///
/// schedule.insert::<PhysicsStep>(());
/// schedule.insert::<RenderStep>(());
///
/// assert_eq!(schedule.jobs().len(), 2);
///
/// let mut world = World::alloc();
/// schedule.run(&mut world);
/// ```
///
/// [`JobSchedule`]: crate::schedule::JobSchedule
/// [`JobLabel`]: crate::job::JobLabel
/// [`JobGroupLabel`]: crate::job::JobGroupLabel
/// [`JobExecutor`]: crate::schedule::JobExecutor
/// [`ExecutorKind`]: crate::schedule::ExecutorKind
/// [`Schedule::with_executor`]: Schedule::with_executor
pub struct Schedule {
    label: InternedScheduleLabel,
    jobs: Jobs,
    groups: Groups,
    stages: Stages,
    hierarchies: Hierarchies,
    ordering: OrderingGraph,
    confict: ConflictGraph,
    schedule: JobSchedule,
    executor: Box<dyn JobExecutor>,
    executor_initialized: bool,
    is_changed: bool,
}

// -----------------------------------------------------------------------------
// Construction & Accessors

impl Schedule {
    /// Creates a new schedule with the given label.
    ///
    /// The concrete executor is selected from [`ExecutorKind::default`]:
    /// multi-threaded when the task pool supports it, single-threaded
    /// otherwise.  Use [`Schedule::with_executor`] to pick an executor
    /// explicitly.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// #[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
    /// struct Startup;
    ///
    /// let schedule = Schedule::new(Startup);
    ///
    /// assert_eq!(schedule.label(), Startup.intern());
    /// assert_eq!(schedule.jobs().len(), 0);
    /// ```
    ///
    /// [`ExecutorKind::default`]: crate::schedule::ExecutorKind::default
    /// [`Schedule::with_executor`]: Schedule::with_executor
    pub fn new(label: impl ScheduleLabel) -> Self {
        let executor: Box<dyn JobExecutor> = match ExecutorKind::default() {
            ExecutorKind::SingleThreaded => Box::new(SingleThreadedExecutor::new()),
            ExecutorKind::MultiThreaded => Box::new(MultiThreadedExecutor::new()),
        };
        Self::with_executor(label, executor)
    }

    /// Creates a new schedule with the given label and executor.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::schedule::{MultiThreadedExecutor, SingleThreadedExecutor};
    /// use zlim_core::schedule::{AnonymousSchedule, ExecutorKind};
    ///
    /// // Force serial execution regardless of the platform default.
    /// let serial = Schedule::with_executor(AnonymousSchedule, Box::new(SingleThreadedExecutor::new()));
    /// assert_eq!(serial.executor_kind(), ExecutorKind::SingleThreaded);
    ///
    /// // Or explicitly opt into parallel execution.
    /// let parallel = Schedule::with_executor(AnonymousSchedule, Box::new(MultiThreadedExecutor::new()));
    /// assert_eq!(parallel.executor_kind(), ExecutorKind::MultiThreaded);
    /// ```
    pub fn with_executor(label: impl ScheduleLabel, executor: Box<dyn JobExecutor>) -> Self {
        Self {
            label: label.intern(),
            jobs: Default::default(),
            groups: Default::default(),
            stages: Default::default(),
            hierarchies: Default::default(),
            ordering: Default::default(),
            confict: Default::default(),
            schedule: Default::default(),
            executor,
            executor_initialized: false,
            is_changed: false,
        }
    }

    /// Returns this schedule's interned label.
    pub fn label(&self) -> InternedScheduleLabel {
        self.label
    }

    /// Iterates all registered job ids in this schedule.
    pub fn jobs(&self) -> impl ExactSizeIterator<Item = JobId> + '_ {
        self.jobs.nodes.keys().copied()
    }

    /// Iterates the names of all inserted groups in this schedule.
    pub fn groups(&self) -> impl ExactSizeIterator<Item = &'static str> + '_ {
        self.groups.keys().copied()
    }

    /// Iterates the names of all inserted stages in this schedule.
    pub fn stages(&self) -> impl ExactSizeIterator<Item = &'static str> + '_ {
        self.stages.keys().copied()
    }

    /// Returns `true` if the job is registered in this schedule.
    pub fn contains_job(&self, id: JobId) -> bool {
        self.jobs.contains(id)
    }

    /// Returns `true` if the group is registered in this schedule.
    ///
    /// Excluding anonymous group.
    pub fn contains_group(&self, name: &str) -> bool {
        self.groups.contains_key(name)
    }

    /// Returns `true` if the group is registered in this schedule.
    ///
    /// Excluding anonymous phase.
    pub fn contains_stage(&self, name: &str) -> bool {
        self.stages.contains_key(name)
    }

    /// Returns the executor kind of this schedule.
    pub fn executor_kind(&self) -> ExecutorKind {
        self.executor.kind()
    }
}

// -----------------------------------------------------------------------------
// Insert Job

struct StageInfo {
    key: &'static str,
    begin: JobId,
    end: JobId,
}

impl StageInfo {
    #[inline]
    fn resolve(stage: &impl ScheduleStage) -> Self {
        Self {
            key: stage.group_name(),
            begin: stage.stage_begin(),
            end: stage.stage_end(),
        }
    }

    #[inline]
    fn is_anonymous(&self) -> bool {
        self.key == JobGroup::ANONYMOUS
    }
}

impl Schedule {
    #[inline]
    fn try_init_stage(&mut self, stage: &StageInfo) -> bool {
        use super::stage::{StageBegin, StageEnd};

        let key = stage.key;

        if self.stages.contains_key(key) {
            return false;
        }

        ::core::hint::cold_path();

        self.is_changed = true;

        let x = {
            let id = stage.begin;
            let job = StageBegin::new(id);
            let node = self.jobs.insert(id, Box::new(job));
            self.ordering.insert_node(node);
            node
        };

        let y = {
            let id = stage.end;
            let job = StageEnd::new(id);
            let node = self.jobs.insert(id, Box::new(job));
            self.ordering.insert_node(node);
            node
        };

        self.ordering.insert_strong_order(x, y);

        let entry = StageEntry {
            begin: x,
            end: y,
            jobs: BTreeSet::new(),
            groups: BTreeSet::new(),
        };

        self.stages.insert(key, entry);

        true
    }

    /// Inserts an apply-deferred sync point between `before` and `after`
    /// when `before` is a deferred job.
    ///
    /// Used by the multi-threaded executor; a no-op otherwise.  Both nodes
    /// must exist and be live (the caller has already resolved them).
    #[inline(never)]
    fn insert_sync_point(&mut self, before: Node, after: Node) {
        // Skip the sync point if the before-job is not deferred, or if its
        // object is currently compiled into the JobSchedule.
        let Some(job) = self.jobs.get_job(before) else {
            ::core::hint::cold_path();
            return;
        };

        if !job.flags().intersects(SystemFlags::DEFERRED) {
            return;
        }

        let before_id = self
            .jobs
            .get_id(before)
            .expect("sync point endpoint should exist");
        let after_id = self
            .jobs
            .get_id(after)
            .expect("sync point endpoint should exist");

        let point = SyncPoint::new(before_id, after_id);
        let job_id = point.id;

        if !self.jobs.contains(job_id) {
            let job_node = self.jobs.insert(job_id, Box::new(point));

            self.ordering.insert_node(job_node); // optional

            self.ordering.insert_strong_order(before, job_node);
            self.ordering.insert_weak_order(job_node, after);

            self.confict.set_exclusive(job_node);

            // Register the cascade relationship: the sync point is removed
            // together with either of its endpoints.
            self.hierarchies
                .entry(before)
                .or_default()
                .children
                .insert(job_node);
            self.hierarchies
                .entry(after)
                .or_default()
                .children
                .insert(job_node);
            let parents = self.hierarchies.entry(job_node).or_default();
            parents.parents.insert(before);
            parents.parents.insert(after);
        }
    }

    #[inline(never)]
    fn insert_db(&mut self, db: JobDB, stage: &StageInfo, caller: DebugLocation) -> bool {
        let id = JobId::isolated(db.name);

        if self.jobs.get_node(id).is_some() {
            core::hint::cold_path();
            log::warn!(
                "The Job `{}` already exists in schedule `{:?}`, skipped. \n\t{}",
                id,
                self.label,
                caller,
            );
            return false;
        }

        self.is_changed = true;

        let key = stage.key;
        let anonymous: bool = stage.is_anonymous();

        if !anonymous {
            self.try_init_stage(stage);
        }

        let multi_thread = self.executor.kind() == ExecutorKind::MultiThreaded;

        let job = (db.ctor)(JobGroup::ANONYMOUS);
        let node = self.jobs.insert(id, job);
        self.ordering.insert_node(node);

        if !anonymous {
            self.jobs.jobs[node.index()].stage = Some(key);

            let stage = self.stages.get_mut(key).expect("inserted above");
            stage.jobs.insert(node);

            let (begin, end) = (stage.begin, stage.end);

            self.ordering.insert_strong_order(begin, node);
            self.ordering.insert_weak_order(node, end);
            // No need to add sync point, StageEnd is noop and deferred.
        }

        for run_if_ctor in db.run_if {
            let run_if = run_if_ctor(JobGroup::ANONYMOUS);
            let run_if_id = run_if.id();
            let run_if_flags = run_if.flags();
            let run_if_node = self.jobs.insert(run_if_id, run_if);
            self.ordering.insert_node(run_if_node);
            if !anonymous {
                // The condition belongs to the stage: it runs strictly after
                // the stage's begin marker.
                let begin = self.stages.get(key).expect("inserted above").begin;
                self.ordering.insert_strong_order(begin, run_if_node);
            }
            self.ordering.insert_strong_order(run_if_node, node);
            if multi_thread && run_if_flags.intersects(SystemFlags::DEFERRED) {
                self.insert_sync_point(run_if_node, node);
            }
            self.hierarchies
                .entry(node)
                .or_default()
                .children
                .insert(run_if_node);
            self.hierarchies
                .entry(run_if_node)
                .or_default()
                .parents
                .insert(node);
        }

        true
    }

    /// Inserts a standalone job looked up from the global [`JobDB`] registry
    /// by name, into the given [`ScheduleStage`].
    ///
    /// Returns `false` if the job is not registered or already exists.
    ///
    /// [`JobDB`]: crate::job::JobDB
    /// [`ScheduleStage`]: crate::schedule::ScheduleStage
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn insert_by_name(&mut self, name: &str, stage: impl ScheduleStage) -> bool {
        let Some(db) = JobDB::get(name) else {
            core::hint::cold_path();
            log::error!("Missing Job named `{name}`, perhaps JobDB is not registered.");
            return false;
        };
        let stage = StageInfo::resolve(&stage);
        self.insert_db(db, &stage, DebugLocation::caller())
    }

    /// Inserts a standalone job from a [`JobLabel`] into the given
    /// [`ScheduleStage`].
    ///
    /// The label's `name()` is tried against the global registry first;
    /// only if that fails is the job constructed from `database()`.
    ///
    /// Returns `false` if the job already exists.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::schedule::AnonymousSchedule;
    ///
    /// #[job_fn(type = CollisionCheck, name = "collision_check")]
    /// fn collision_check() {}
    ///
    /// let mut schedule = Schedule::new(AnonymousSchedule);
    ///
    /// // Insert into the anonymous stage.
    /// assert!(schedule.insert::<CollisionCheck>(()));
    ///
    /// // Inserting the same job twice returns `false`.
    /// assert!(!schedule.insert::<CollisionCheck>(()));
    /// ```
    ///
    /// [`JobLabel`]: crate::job::JobLabel
    /// [`ScheduleStage`]: crate::schedule::ScheduleStage
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn insert<L: JobLabel>(&mut self, stage: impl ScheduleStage) -> bool {
        let name = L::name();
        let stage = StageInfo::resolve(&stage);
        if let Some(db) = JobDB::get(name) {
            self.insert_db(db, &stage, DebugLocation::caller())
        } else {
            ::core::hint::cold_path();
            let db = L::database();
            JobDB::register(db);
            self.insert_db(db, &stage, DebugLocation::caller())
        }
    }
}

// -----------------------------------------------------------------------------
// Remove Job

impl Schedule {
    /// Removes the job with the given id.
    ///
    /// Returns `false` if the job is not registered in this schedule.
    #[inline(never)]
    fn remove_job(&mut self, id: JobId) -> bool {
        let Some(node) = self.jobs.get_node(id) else {
            core::hint::cold_path();
            return false;
        };
        let stage = self.jobs.stage(node);
        let node = self.jobs.remove(id).expect("checked above");

        self.is_changed = true;
        self.ordering.remove_node(node);
        self.confict.remove(node);

        // Scrub the job from its stage's membership set.
        if let Some(key) = stage
            && let Some(entry) = self.stages.get_mut(key)
        {
            entry.jobs.remove(&node);
        }

        let Some(h) = self.hierarchies.remove(&node) else {
            return true;
        };

        for child in h.children {
            if let Some(child_id) = self.jobs.get_id(child) {
                let _ = self.remove_job(child_id);
            }
        }

        for parent in h.parents {
            if let Some(p) = self.hierarchies.get_mut(&parent) {
                p.children.remove(&node);
            }
        }

        true
    }

    /// Removes the standalone job with the given name.
    ///
    /// Returns `false` if the job is not registered in this schedule.
    pub fn remove_by_name(&mut self, name: &str) -> bool {
        // SAFETY: Temporary Value
        let name = unsafe { core::mem::transmute::<&str, &'static str>(name) };
        self.remove_job(JobId::isolated(name))
    }

    /// Removes the standalone job identified by the [`JobLabel`].
    ///
    /// [`JobLabel`]: crate::job::JobLabel
    pub fn remove<L: JobLabel>(&mut self) -> bool {
        let name = L::name();
        self.remove_job(JobId::isolated(name))
    }
}

// -----------------------------------------------------------------------------
// Order

impl Schedule {
    /// Inserts one ordering edge from a group's job-index pair.
    ///
    /// Edges referencing jobs that were skipped during insertion are
    /// silently dropped.
    #[inline]
    fn insert_edge<const STRONG: bool>(
        &mut self,
        before: JobId,
        after: JobId,
        caller: DebugLocation,
    ) {
        let Some(b) = self.jobs.get_node(before) else {
            ::core::hint::cold_path();
            let label = self.label;
            log::warn!("Missing Job `{before}` in Schedule `{label:?}`, skiped.\n\t{caller}");
            return;
        };
        let Some(a) = self.jobs.get_node(after) else {
            ::core::hint::cold_path();
            let label = self.label;
            log::warn!("Missing Job `{after}` in Schedule `{label:?}`, skiped.\n\t{caller}");
            return;
        };

        if STRONG {
            self.ordering.insert_strong_order(b, a);
        } else {
            self.ordering.insert_weak_order(b, a);
        }
    }

    /// Insert a strong order by given sequence.
    ///
    /// # order / strong_order
    ///
    /// The subsequent jobs will only be executed after the previous jobs
    /// have been completed **successfully**, and the results of deferred
    /// commands queued by previous jobs is **definitely visible**.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::schedule::AnonymousSchedule;
    ///
    /// #[derive(TypePath, ScheduleStage)]
    /// enum FixedMain {
    ///     PreUpdate,
    ///     Update,
    ///     PostUpdate,
    /// }
    ///
    /// let mut schedule = Schedule::new(AnonymousSchedule);
    ///
    /// schedule.insert_stage(FixedMain::PreUpdate);
    /// schedule.insert_stage(FixedMain::Update);
    /// schedule.insert_stage(FixedMain::PostUpdate);
    /// schedule.insert_order(&[FixedMain::PreUpdate.stage_end(), FixedMain::Update.stage_begin()]);
    /// schedule.insert_order(&[FixedMain::Update.stage_end(), FixedMain::PostUpdate.stage_begin()]);
    /// ```
    #[inline(never)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn insert_order(&mut self, order: &[JobId]) {
        if order.len() <= 1 {
            return;
        }
        self.is_changed = true;
        let caller = DebugLocation::caller();

        for index in 1..order.len() {
            let before: JobId = order[index - 1];
            let after: JobId = order[index];
            self.insert_edge::<true>(before, after, caller);
        }

        if self.executor_kind() == ExecutorKind::SingleThreaded {
            // In single-threaded mode, no need to insert sync points.
            return;
        }

        for index in 1..order.len() {
            let before: JobId = order[index - 1];
            let after: JobId = order[index];
            let Some(before_node) = self.jobs.get_node(before) else {
                return;
            };
            let Some(after_node) = self.jobs.get_node(after) else {
                return;
            };
            self.insert_sync_point(before_node, after_node);
        }
    }

    /// Insert a weak order by given sequence.
    ///
    /// # weak_order
    ///
    /// The subsequent jobs will only be executed after the previous jobs
    /// have been completed, **whether successful or not (even skipped)**.
    ///
    /// The results of deferred commands queued by previous jobs is **definitely visible**.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::schedule::AnonymousSchedule;
    ///
    /// #[derive(TypePath, ScheduleStage)]
    /// enum FixedMain {
    ///     PreUpdate,
    ///     Update,
    ///     PostUpdate,
    /// }
    ///
    /// let mut schedule = Schedule::new(AnonymousSchedule);
    ///
    /// schedule.insert_stage(FixedMain::PreUpdate);
    /// schedule.insert_stage(FixedMain::Update);
    /// schedule.insert_stage(FixedMain::PostUpdate);
    /// schedule.insert_weak_order(&[FixedMain::PreUpdate.stage_end(), FixedMain::Update.stage_begin()]);
    /// schedule.insert_weak_order(&[FixedMain::Update.stage_end(), FixedMain::PostUpdate.stage_begin()]);
    /// ```
    #[inline(never)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn insert_weak_order(&mut self, order: &[JobId]) {
        if order.len() <= 1 {
            return;
        }

        self.is_changed = true;
        let caller = DebugLocation::caller();

        for index in 1..order.len() {
            let before: JobId = order[index - 1];
            let after: JobId = order[index];
            self.insert_edge::<false>(before, after, caller);
        }

        if self.executor_kind() == ExecutorKind::SingleThreaded {
            // In single-threaded mode, no need to insert sync points.
            return;
        }

        for index in 1..order.len() {
            let before: JobId = order[index - 1];
            let after: JobId = order[index];
            let Some(before_node) = self.jobs.get_node(before) else {
                return;
            };
            let Some(after_node) = self.jobs.get_node(after) else {
                return;
            };
            self.insert_sync_point(before_node, after_node);
        }
    }

    /// Insert a relaxed order by given sequence.
    ///
    /// # relaxed_order
    ///
    /// The subsequent jobs will only be executed after the previous jobs
    /// have been completed, **whether successful or not (even skipped)**.
    ///
    /// The results of deferred commands queued by previous jobs **may not be visible**.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::schedule::AnonymousSchedule;
    ///
    /// #[derive(TypePath, ScheduleStage)]
    /// enum FixedMain {
    ///     PreUpdate,
    ///     Update,
    ///     PostUpdate,
    /// }
    ///
    /// let mut schedule = Schedule::new(AnonymousSchedule);
    ///
    /// schedule.insert_stage(FixedMain::PreUpdate);
    /// schedule.insert_stage(FixedMain::Update);
    /// schedule.insert_stage(FixedMain::PostUpdate);
    /// schedule.insert_relaxed_order(&[FixedMain::PreUpdate.stage_end(), FixedMain::Update.stage_begin()]);
    /// schedule.insert_relaxed_order(&[FixedMain::Update.stage_end(), FixedMain::PostUpdate.stage_begin()]);
    /// ```
    #[inline(never)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn insert_relaxed_order(&mut self, order: &[JobId]) {
        if order.len() <= 1 {
            return;
        }

        self.is_changed = true;
        let caller = DebugLocation::caller();

        for index in 1..order.len() {
            let before: JobId = order[index - 1];
            let after: JobId = order[index];
            self.insert_edge::<false>(before, after, caller);
        }
    }
}

// -----------------------------------------------------------------------------
// Insert Group

impl Schedule {
    /// Inserts every job of `group` together with its ordering constraints,
    /// into the given [`ScheduleStage`].
    ///
    /// Returns `false` if the group already exists.  Jobs missing from the
    /// global [`JobDB`] registry are skipped with a warning.
    ///
    /// [`JobDB`]: crate::job::JobDB
    /// [`ScheduleStage`]: crate::schedule::ScheduleStage
    #[inline(never)]
    fn insert_group_object(
        &mut self,
        group: JobGroup,
        stage: &StageInfo,
        caller: DebugLocation,
    ) -> bool {
        let name = group.name;
        let multi_thread = self.executor.kind() == ExecutorKind::MultiThreaded;

        if let Some(entry) = self.groups.get(name) {
            core::hint::cold_path();
            let stage = entry.stage.unwrap_or("#anonymous");
            log::warn!(
                "The JobGroup `{}` already exists in schedule `{:?}`'s stage `{:?}`, skipped. \n\t{}",
                name,
                self.label,
                stage,
                caller,
            );
            return false;
        }

        self.is_changed = true;

        let key = stage.key;
        let anonymous = stage.is_anonymous();

        if !anonymous {
            self.try_init_stage(stage);
        }

        // Create and insert all jobs of the group.
        for &job in group.jobs {
            let job_name = job.name();
            let Some(db) = JobDB::get(job_name) else {
                core::hint::cold_path();
                log::error!(
                    "Missing job `{job_name}` in group `{name}`, perhaps \
                    JobDB is not registered. Please call `JobDB::collect` \
                    before any schedule operation. \n\t{caller}"
                );
                continue;
            };

            if self.jobs.contains(job) {
                continue;
            }

            let node = self.jobs.insert(job, (db.ctor)(name));
            self.ordering.insert_node(node);

            for run_if_ctor in db.run_if {
                let run_if = run_if_ctor(name);
                let run_if_id = run_if.id();
                let run_if_flags = run_if.flags();
                let run_if_node = self.jobs.insert(run_if_id, run_if);
                self.ordering.insert_node(run_if_node);
                // The condition belongs to the group: it runs strictly after
                // the group's begin marker (`jobs[0]`).
                if let Some(g_begin) = self.jobs.get_node(group.jobs[0]) {
                    self.ordering.insert_strong_order(g_begin, run_if_node);
                }
                self.ordering.insert_strong_order(run_if_node, node);
                self.hierarchies
                    .entry(node)
                    .or_default()
                    .children
                    .insert(run_if_node);
                self.hierarchies
                    .entry(run_if_node)
                    .or_default()
                    .parents
                    .insert(node);
                if multi_thread && run_if_flags.intersects(SystemFlags::DEFERRED) {
                    self.insert_sync_point(run_if_node, node);
                }
            }
        }

        // Insert ordering constraints; edges referencing missing jobs are skipped.
        for &(before, after) in group.order {
            let before = group.jobs[before as usize];
            let after = group.jobs[after as usize];
            self.insert_edge::<true>(before, after, caller);
        }
        for &(before, after) in group.weak_order {
            let before = group.jobs[before as usize];
            let after = group.jobs[after as usize];
            self.insert_edge::<false>(before, after, caller);
        }
        for &(before, after) in group.relaxed_order {
            let before = group.jobs[before as usize];
            let after = group.jobs[after as usize];
            self.insert_edge::<false>(before, after, caller);
        }

        // Multi-threaded executors need apply-deferred helper jobs
        // inserted between deferred jobs and their successors.
        if multi_thread {
            for (before, after) in group.order.iter().chain(group.weak_order) {
                // [0] is `GroupBegin`, [1] is `GroupEnd`.
                if *before == 0 || *after == 1 {
                    continue;
                }
                let before: JobId = group.jobs[(*before) as usize];
                let after: JobId = group.jobs[(*after) as usize];
                let Some(before_node) = self.jobs.get_node(before) else {
                    continue;
                };
                let Some(after_node) = self.jobs.get_node(after) else {
                    continue;
                };
                self.insert_sync_point(before_node, after_node);
            }
        }

        let stage = (!anonymous).then_some(key);
        let entry = GroupEntry { stage, group };

        // Record the group together with its stage membership.
        self.groups.insert(name, entry);

        if !anonymous {
            let stage = self.stages.get_mut(key).expect("inserted above");
            stage.groups.insert(name);

            // `jobs[0]` is `GroupBegin`, `jobs[1]` is `GroupEnd`.
            if let Some(g_begin) = self.jobs.get_node(group.jobs[0]) {
                self.ordering.insert_strong_order(stage.begin, g_begin);
            }
            if let Some(g_end) = self.jobs.get_node(group.jobs[1]) {
                self.ordering.insert_weak_order(g_end, stage.end);
                // no need to insert sync point, because StageEnd is noop and deferred.
            }
        }

        true
    }

    /// Inserts a group looked up from the global [`JobGroup`] registry by
    /// name, into the given [`ScheduleStage`].
    ///
    /// Returns `false` if the group is not registered or already exists.
    ///
    /// [`JobGroup`]: crate::job::JobGroup
    /// [`ScheduleStage`]: crate::schedule::ScheduleStage
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn insert_group_by_name(&mut self, name: &str, stage: impl ScheduleStage) -> bool {
        let Some(group) = JobGroup::get(name).copied() else {
            core::hint::cold_path();
            log::error!("Missing JobGroup named `{name}`, perhaps it is not registered.");
            return false;
        };
        let stage = StageInfo::resolve(&stage);
        self.insert_group_object(group, &stage, DebugLocation::caller())
    }

    /// Inserts a group from a [`JobGroupLabel`] into the given
    /// [`ScheduleStage`].
    ///
    /// The label's `name()` is tried against the global registry first; only
    /// if that fails is the group constructed from `group()`.
    ///
    /// Returns `false` if the group already exists.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::schedule::AnonymousSchedule;
    ///
    /// #[job_fn(type = InputRead, name = "input_read")]
    /// fn input_read() {}
    ///
    /// #[job_fn(type = PlayerMove, name = "player_move")]
    /// fn player_move() {}
    ///
    /// job_group! {
    ///     type: Gameplay,
    ///     name: "gameplay",
    ///     jobs: [InputRead, PlayerMove],
    ///     order: [[InputRead, PlayerMove]],
    /// }
    ///
    /// // Load the statically-registered jobs and groups first, so the group
    /// // can resolve every job from the global registries.
    /// JobDB::collect();
    /// JobGroup::collect();
    ///
    /// let mut schedule = Schedule::new(AnonymousSchedule);
    /// // Inserts every job of the group plus its ordering constraints.
    /// assert!(schedule.insert_group::<Gameplay>(()));
    /// assert!(schedule.contains_group("gameplay"));
    /// assert!(schedule.contains_job(JobId::new("input_read", "gameplay")));
    /// ```
    ///
    /// [`JobGroupLabel`]: crate::job::JobGroupLabel
    /// [`ScheduleStage`]: crate::schedule::ScheduleStage
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn insert_group<G: JobGroupLabel>(&mut self, stage: impl ScheduleStage) -> bool {
        let name = G::name();
        let stage = StageInfo::resolve(&stage);
        if let Some(group) = JobGroup::get(name) {
            self.insert_group_object(*group, &stage, DebugLocation::caller())
        } else {
            ::core::hint::cold_path();
            let group = G::group();
            JobGroup::register(group);
            self.insert_group_object(group, &stage, DebugLocation::caller())
        }
    }
}

// -----------------------------------------------------------------------------
// Remove Group

impl Schedule {
    /// Removes the group with the given name and every job created for it.
    ///
    /// Job ids that no longer exist are skipped.  Returns `false` if the
    /// group is not registered in this schedule.
    #[inline(never)]
    pub fn remove_group_by_name(&mut self, name: &str) -> bool {
        let Some(entry) = self.groups.remove(name) else {
            core::hint::cold_path();
            return false;
        };

        self.is_changed = true;

        // Scrub the group from its stage's membership set.
        if let Some(key) = entry.stage
            && let Some(stage) = self.stages.get_mut(key)
        {
            stage.groups.remove(name);
        }

        for &id in entry.group.jobs {
            let _ = self.remove_job(id);
        }

        true
    }

    /// Removes the group identified by the [`JobGroupLabel`].
    ///
    /// [`JobGroupLabel`]: crate::job::JobGroupLabel
    pub fn remove_group<G: JobGroupLabel>(&mut self) -> bool {
        self.remove_group_by_name(G::name())
    }
}

// -----------------------------------------------------------------------------
// Insert Stage

impl Schedule {
    #[inline(never)]
    fn remove_stage_internal(&mut self, name: &str) -> bool {
        let Some(entry) = self.stages.remove(name) else {
            return false;
        };

        if let Some(job_id) = self.jobs.get_id(entry.begin) {
            self.remove_job(job_id);
        }
        if let Some(job_id) = self.jobs.get_id(entry.end) {
            self.remove_job(job_id);
        }
        for job in entry.jobs {
            if let Some(job_id) = self.jobs.get_id(job) {
                self.remove_job(job_id);
            }
        }
        for group in entry.groups {
            self.remove_group_by_name(group);
        }

        true
    }

    /// Insert a schedule stage (if it does not exist).
    ///
    /// Note that this will not modify the execution order of the stages.
    /// You need to have used Stage's `stage_begin` and `stage_end` functions
    /// to insert the order yourself.
    pub fn insert_stage(&mut self, stage: impl ScheduleStage) -> bool {
        let info = StageInfo::resolve(&stage);
        self.try_init_stage(&info)
    }

    /// Remove a stage and also remove all Jobs and JobGroups belonging to the current stage.
    pub fn remove_stage(&mut self, stage: impl ScheduleStage) -> bool {
        let name = stage.stage_name();
        self.remove_stage_internal(&name)
    }
}

// -----------------------------------------------------------------------------
// Build Pipeline Internal

impl Schedule {
    /// Moves compiled job objects back into the buffer so the schedule can
    /// be rebuilt from scratch.
    ///
    /// Jobs keep their access tables, so only newly inserted jobs are
    /// re-initialized and re-registered by the next rebuild.
    #[inline(never)]
    fn recycle_schedule(&mut self) {
        let schedule = &mut self.schedule;
        let jobs = &mut self.jobs;

        schedule.conflict = ConflictTable::new(0);
        schedule.incoming.clear();
        schedule.outgoing.clear();
        schedule.strong_incoming.clear();
        schedule.strong_outgoing.clear();
        schedule.flags.clear();
        schedule.pool = Bump::new(200); // placeholder

        let jobs_vec = core::mem::take(&mut schedule.jobs);
        let nodes = core::mem::take(&mut schedule.nodes);
        let access_tables = core::mem::take(&mut schedule.access_tables);

        for ((&node, job), access) in nodes.iter().zip(jobs_vec).zip(access_tables) {
            jobs.recycle(node, job, access);
        }
    }

    /// Initializes newly inserted jobs and rebuilds the conflict graph.
    #[inline(never)]
    fn init_systems(&mut self, world: &World) {
        let multi_threaded = self.executor.kind() == ExecutorKind::MultiThreaded;
        let jobs = &mut self.jobs;
        let conflict = &mut self.confict;

        // Newly inserted jobs are recorded in `uninit` (their slots lack an
        // access table); recycled jobs keep theirs and skip re-initialization.
        let mut uninit = core::mem::take(&mut jobs.uninit);
        uninit.sort();
        uninit.dedup();

        if zlim_task::cfg::multi_thread!() && uninit.len() > 2 {
            zlim_task::MainTaskPool::get().scope(|s| {
                let jobs = jobs as *mut Jobs;
                uninit.iter().for_each(|&node| {
                    // SAFETY: each node is unique and the scope joins before
                    // `jobs` is used again.
                    if let Some(entry) = unsafe { &mut *jobs }.get_entry_mut(node) {
                        s.spawn(async move {
                            let job = entry.object.as_mut().expect("job should be present");
                            job.initialize(world);
                            let mut table = AccessTable::new();
                            job.register_access(&mut table);
                            entry.access = Some(table);
                        });
                    }
                });
            });
        } else {
            // Initialize new jobs and collect their access tables.
            uninit.iter().for_each(|&node| {
                if let Some(job) = jobs.get_job_mut(node) {
                    job.initialize(world);
                    let mut table = AccessTable::new();
                    job.register_access(&mut table);
                    jobs.set_access_table(node, table);
                }
            });
        }

        // The conflict graph is only consulted by the multi-threaded
        // executor.  Single-threaded schedules run serially, so we skip the
        // pairwise access comparison entirely and initialize faster.
        if multi_threaded {
            // Compute conflicts between the new jobs and every job in the
            // schedule.  Conflicts among pre-existing jobs are unaffected by
            // the insertions and remain valid.
            let nodes = jobs.present_nodes();

            uninit.iter().for_each(|&a| {
                let job_a = jobs.get_job(a).unwrap();
                let access_a = jobs.access_table(a).unwrap();

                if job_a.flags().intersects(SystemFlags::EXCLUSIVE) {
                    conflict.set_exclusive(a);
                    return;
                }

                nodes.iter().for_each(|&b| {
                    if a == b {
                        return;
                    }
                    let Some(access_b) = jobs.access_table(b) else {
                        return;
                    };
                    if !access_a.parallelizable(access_b) {
                        conflict.set_conflict(a, b);
                    }
                });
            });
        }
    }

    /// Rebuilds the executor-ready representation from the ordering graph.
    #[inline(never)]
    fn build_schedule(&mut self) {
        let jobs = &mut self.jobs;
        let ordering = &mut self.ordering;
        let conflict = &mut self.confict;
        let schedule = &mut self.schedule;

        debug_assert!(schedule.jobs.is_empty());
        debug_assert!(schedule.flags.is_empty());
        debug_assert!(schedule.nodes.is_empty());
        debug_assert!(schedule.access_tables.is_empty());
        debug_assert!(schedule.incoming.is_empty());
        debug_assert!(schedule.outgoing.is_empty());
        debug_assert!(schedule.strong_incoming.is_empty());
        debug_assert!(schedule.strong_outgoing.is_empty());

        // Merge weak and strong ordering; strong edges also participate in
        // the topological order so they block readiness.
        let mut dag = ordering.weak_order.clone();
        for (a, b) in ordering.order.all_edges() {
            dag.insert_edge(a, b);
        }

        let nodes = match dag.toposort() {
            Ok(nodes) => nodes,
            Err(err) => self.handle_toposort_error(err),
        };

        schedule.nodes.extend_from_slice(nodes);
        let nodes = schedule.nodes.as_slice();

        for &node in nodes {
            let (job, access) = jobs.take(node);
            schedule.access_tables.push(access);
            schedule.jobs.push(job);
        }
        for job in &schedule.jobs {
            schedule.flags.push(job.flags());
        }

        // Map node to execution index.
        let mut indices: HashMap<Node, usize> = HashMap::with_capacity(nodes.len());

        for (index, n) in nodes.iter().enumerate() {
            indices.insert(*n, index);
        }

        schedule.incoming.resize(nodes.len(), 0);
        schedule.outgoing.resize(nodes.len(), &[]);
        schedule.strong_incoming.resize(nodes.len(), 0);
        schedule.strong_outgoing.resize(nodes.len(), &[]);

        let mut outgoing: Vec<Vec<u16>> = vec![Vec::new(); nodes.len()];
        let mut strong_outgoing: Vec<Vec<u16>> = vec![Vec::new(); nodes.len()];

        // Normal dependencies from the merged graph.
        nodes.iter().enumerate().for_each(|(idx, &n)| {
            dag.neighbors(n).for_each(|to| {
                let to_idx = indices[&to];
                schedule.incoming[to_idx] += 1;
                outgoing[idx].push(to_idx as u16);
            });
        });
        ::core::mem::drop(dag);

        // Strong (conditional) dependencies from the strong order graph.
        nodes.iter().enumerate().for_each(|(idx, &n)| {
            ordering.order.neighbors(n).for_each(|to| {
                let to_idx = indices[&to];
                schedule.strong_incoming[to_idx] += 1;
                strong_outgoing[idx].push(to_idx as u16);
            });
        });
        ::core::mem::drop(indices);

        // The capacity of first memory page in Bump.
        let hint = {
            // Some formulas that looks a bit strange.
            // 1. 64usize is used to limit the minimum capacity
            // 2. The size of an edge is 2B (u16), and all nodes in JobGroup are related to
            //    Begin and End There is an edge, therefore there is len * 4usize
            // 3. The number of custom edges should be exponential to itself.
            //    (1usize << (k >> 2)) ≈≈ len ^ 1.2
            let len = nodes.len();
            let k = usize::BITS - len.leading_ones();
            64usize + len * (4usize + (1usize << (k >> 2)))
        };

        // Move adjacency lists into the bump pool for compact storage.
        schedule.pool = Bump::new(hint);

        outgoing.iter().enumerate().for_each(|(idx, slice)| {
            let item: &[u16] = schedule.pool.alloc_slice(slice.as_slice());
            schedule.outgoing[idx] = unsafe { core::mem::transmute::<&[u16], &[u16]>(item) };
        });
        ::core::mem::drop(outgoing);

        strong_outgoing.iter().enumerate().for_each(|(idx, slice)| {
            let item: &[u16] = schedule.pool.alloc_slice(slice.as_slice());
            schedule.strong_outgoing[idx] = unsafe { core::mem::transmute::<&[u16], &[u16]>(item) };
        });
        ::core::mem::drop(strong_outgoing);

        // Build the fixed conflict matrix.  The matrix is only consulted by
        // the multi-threaded executor; single-threaded schedules keep an
        // empty table (every `is_conflict` query reports a conflict), which
        // skips the O(n²) fill and the matrix allocation entirely.
        let multi_threaded = self.executor.kind() == ExecutorKind::MultiThreaded;
        // Although there is always no conflict in singe-threaded, enough space is still given.
        let mut conflict_table = ConflictTable::new(nodes.len());

        if multi_threaded {
            nodes.iter().enumerate().for_each(|(idx_a, &a)| {
                if conflict.is_exclusive(a) {
                    unsafe {
                        conflict_table.set_exclusive(idx_a as u16);
                    }
                    return;
                }

                for (offset, &b) in nodes[(idx_a + 1)..].iter().enumerate() {
                    let idx_b = (idx_a + offset + 1) as u16;
                    let idx_a = idx_a as u16;
                    if conflict.is_conflict(a, b) {
                        unsafe {
                            conflict_table.set_conflict(idx_a, idx_b);
                            conflict_table.set_conflict(idx_b, idx_a);
                        }
                    }
                }
            });
        }

        schedule.conflict = conflict_table;
    }

    #[cold]
    #[inline(never)]
    fn handle_toposort_error(&mut self, err: ToposortError) -> ! {
        let id_of = |node: Node| -> JobId { self.jobs.get_id(node).expect("should exist") };

        match err {
            ToposortError::Loop(node) => {
                let id = id_of(node);
                panic!(
                    "Update schedule `{:?}` failed, self-loop detected at job `{id}` (node `{node}`).",
                    self.label
                );
            }
            ToposortError::Cycle(cycles) => {
                let cycles: Vec<Vec<JobId>> = cycles
                    .iter()
                    .map(|cycle| cycle.iter().map(|&node| id_of(node)).collect())
                    .collect();
                panic!(
                    "Update schedule `{:?}` failed, cycles detected: `{cycles:?}`.",
                    self.label
                );
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Update & Run

impl Schedule {
    /// Rebuilds the executable schedule if jobs or groups changed.
    ///
    /// Calling this repeatedly without any structural changes is cheap.
    #[inline]
    pub fn update(&mut self, world: &World) {
        if self.is_changed {
            core::hint::cold_path();
            #[cfg(feature = "trace")]
            let _span = zlim_log::info_span!("update schedule", name = ?self.label).entered();
            self.recycle_schedule();
            self.init_systems(world);
            self.build_schedule();
            self.is_changed = false;
        }

        if !self.executor_initialized {
            core::hint::cold_path();
            self.executor.init(&self.schedule);
            self.executor_initialized = true;
        }
    }

    /// Executes the schedule once.
    ///
    /// This performs [`Schedule::update`] first, then runs all jobs through
    /// the configured executor.  Jobs advance the world tick as they run
    /// (see [`World::advance_tick`]), and the executor flushes deferred
    /// commands once the run completes.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_core::schedule::AnonymousSchedule;
    ///
    /// #[job_fn(type = Greet, name = "greet")]
    /// fn greet() {}
    ///
    /// let mut schedule = Schedule::new(AnonymousSchedule);
    /// schedule.insert::<Greet>(());
    ///
    /// let mut world = World::alloc();
    /// // Rebuilds the schedule if its contents changed, then runs every job.
    /// schedule.run(&mut world);
    /// assert_eq!(schedule.jobs().len(), 1);
    /// ```
    ///
    /// [`Schedule::update`]: Schedule::update
    /// [`World::advance_tick`]: crate::world::World::advance_tick
    #[inline]
    pub fn run(&mut self, world: &mut World) {
        if self.jobs.is_empty() {
            return;
        }

        #[cfg(feature = "trace")]
        let _span = zlim_log::info_span!("run schedule", name = ?self.label).entered();

        world.flush();

        self.update(world);

        let handler = world.error_handler();
        self.executor.exec(&mut self.schedule, world, handler);

        // world.flush(); // Should be flushed by Executor.
    }

    /// Clamps all systems's stored change-detection ticks against now
    /// to keep them within a valid range after tick wrap-around.
    pub fn clamp_ticks(&mut self, now: Tick) {
        for entry in self.jobs.jobs.iter_mut() {
            if let Some(job) = entry.object.as_mut() {
                job.clamp_ticks(now);
            }
        }
        for job in self.schedule.jobs.iter_mut() {
            job.clamp_ticks(now);
        }
    }
}

// -----------------------------------------------------------------------------
