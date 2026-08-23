#![expect(clippy::module_inception, reason = "For better structure.")]

use core::cmp::Reverse;
use std::collections::BinaryHeap;

use zlim_log as log;
use zlim_utils::debug::DebugLocation;
use zlim_utils::hash::{HashMap, HashSet, NoopState};
use zlim_utils::mem::Bump;

use super::{ConflictTable, ExecutorKind, JobSchedule};
use super::{Dag, Node, ToposortError};
use super::{InternedScheduleLabel, JobExecutor, ScheduleLabel};
use super::{MultiThreadedExecutor, SingleThreadedExecutor};
use crate::job::{Job, JobDB, JobGroup, JobGroupLabel, JobId, JobLabel};
use crate::system::{AccessTable, SystemError, SystemFlags};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------

enum Slot<T> {
    Empty(u16),
    Data((u16, T)),
}

struct JobObject {
    job: Box<dyn Job>,
    access: AccessTable,
}

#[derive(Default)]
struct Allocator {
    slots: Vec<Slot<JobId>>,
    mapper: HashMap<JobId, Node, NoopState>,
    reusable: BinaryHeap<Reverse<u16>>,
}

#[derive(Default)]
struct Buffer {
    slots: Vec<Slot<Option<JobObject>>>,
    uninit: Vec<Node>,
}

#[derive(Default)]
struct OrderingGraph {
    order: Dag,
    weak_order: Dag,
}

#[derive(Default)]
struct ConflictGraph {
    exclusive: HashSet<Node>,
    conflicts: HashMap<Node, HashSet<Node>>,
}

// -----------------------------------------------------------------------------
// Allocator Implementation

impl Allocator {
    fn get_node(&self, id: JobId) -> Option<Node> {
        self.mapper.get(&id).copied()
    }

    fn get_id(&self, node: Node) -> Option<JobId> {
        let slot = self.slots.get(node.index())?;
        match slot {
            Slot::Empty(_) => None,
            Slot::Data((tag, id)) => (*tag == node.tag).then_some(*id),
        }
    }

    fn contains(&self, id: JobId) -> bool {
        self.mapper.contains_key(&id)
    }

    fn insert(&mut self, id: JobId) -> Node {
        if let Some(node) = self.mapper.get(&id).copied() {
            *self.slots.get_mut(node.index()).unwrap() = Slot::Data((node.tag, id));
            return node;
        }

        if let Some(Reverse(idx)) = self.reusable.pop() {
            let slot = self.slots.get_mut(idx as usize).unwrap();
            let tag: u16 = match slot {
                Slot::Empty(t) => *t,
                Slot::Data((t, _)) => *t,
            };
            let new_tag = tag.wrapping_add(1);
            let node = Node { idx, tag: new_tag };
            *slot = Slot::Data((new_tag, id));
            self.mapper.insert(id, node);
            return node;
        }

        assert! {
            self.slots.len() < u16::MAX as usize,
            "too many Job in a Schedule, cannot exceed u16::MAX",
        }

        let idx = self.slots.len() as u16;
        let tag = 1;
        let node = Node { idx, tag };

        self.slots.push(Slot::Data((tag, id)));
        self.mapper.insert(id, node);

        node
    }

    fn remove(&mut self, id: JobId) -> Option<Node> {
        let node = self.mapper.remove(&id)?;

        let slot = self.slots.get_mut(node.index()).unwrap();

        match slot {
            Slot::Data((tag, id)) if *tag == node.tag => {
                let current_tag = *tag;
                *slot = Slot::Empty(current_tag);
                self.reusable.push(Reverse(node.idx));
                Some(node)
            }
            _ => None,
        }
    }
}

// -----------------------------------------------------------------------------
// Buffer Implementation

impl Buffer {
    fn insert(&mut self, node: Node, job: Box<dyn Job>) {
        let obj = JobObject {
            job,
            access: AccessTable::new(),
        };
        while self.slots.len() <= node.index() {
            self.slots.push(Slot::Empty(0));
        }

        unsafe {
            *self.slots.get_unchecked_mut(node.index()) = Slot::Data((node.tag, Some(obj)));
        }

        self.uninit.push(node);
    }

    fn remove(&mut self, node: Node) {
        let index = node.index();

        if let Some(slot) = self.slots.get_mut(index) {
            if let Slot::Data((tag, _)) = slot
                && *tag == node.tag
            {
                *slot = Slot::Empty(node.tag);
            }
            if let Some(index) = self.uninit.iter().position(|value| *value == node) {
                self.uninit.swap_remove(index);
            }
        }
    }

    /// Reinserts a compiled job together with its access table.
    ///
    /// Unlike [`Buffer::insert`], recycled jobs are not added to the
    /// uninitialized list: their access table stays valid and they skip
    /// re-initialization on the next rebuild.
    ///
    /// [`Buffer::insert`]: Buffer::insert
    fn recycle(&mut self, node: Node, job: Box<dyn Job>, access: AccessTable) {
        let obj = JobObject { job, access };
        debug_assert!(self.slots.len() > node.index());

        unsafe {
            *self.slots.get_unchecked_mut(node.index()) = Slot::Data((node.tag, Some(obj)));
        }
    }

    /// Iterates the nodes of every job currently stored in the buffer.
    fn nodes(&self) -> Vec<Node> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(idx, slot)| match slot {
                Slot::Data((tag, Some(_))) => Some(Node {
                    idx: idx as u16,
                    tag: *tag,
                }),
                _ => None,
            })
            .collect()
    }

    fn get_job(&self, node: Node) -> Option<&JobObject> {
        let slot = self.slots.get(node.index())?;

        if let Slot::Data((tag, data)) = slot {
            debug_assert_eq!(*tag, node.tag);
            data.as_ref()
        } else {
            None
        }
    }

    fn get_job_mut(&mut self, node: Node) -> Option<&mut JobObject> {
        let slot = self.slots.get_mut(node.index())?;

        if let Slot::Data((tag, data)) = slot {
            debug_assert_eq!(*tag, node.tag);
            data.as_mut()
        } else {
            None
        }
    }

    fn take_job(&mut self, node: Node) -> JobObject {
        let slot = self.slots.get_mut(node.index()).unwrap();

        if let Slot::Data((tag, data)) = slot {
            debug_assert_eq!(*tag, node.tag);
            data.take().unwrap()
        } else {
            unreachable!()
        }
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
            a_set.iter().for_each(|b| {
                if let Some(b_set) = self.conflicts.get_mut(b) {
                    b_set.remove(&node);
                }
            });
        }
    }
}

// -----------------------------------------------------------------------------
// Groups

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
        SystemFlags::NO_OP
            .union(SystemFlags::EXCLUSIVE)
            .union(SystemFlags::NON_SEND)
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
        let buf = format!("#SyncPoint<{name}, {after}>");
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
/// use zlim_core::schedule::ScheduleLabel;
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
/// schedule.insert::<PhysicsStep>();
/// schedule.insert::<RenderStep>();
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
    allocator: Allocator,
    buffer: Buffer,
    ordering: OrderingGraph,
    /// Access-conflict graph between jobs.  Only maintained when the
    /// executor is [`ExecutorKind::MultiThreaded`]; single-threaded
    /// schedules leave it empty.
    confict: ConflictGraph,
    schedule: JobSchedule,
    groups: HashMap<&'static str, JobGroup>,
    sync_points: HashMap<JobId, Vec<JobId>, NoopState>,
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
    /// use zlim_core::schedule::ScheduleLabel;
    ///
    /// #[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
    /// struct Startup;
    ///
    /// let schedule = Schedule::new(Startup);
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
    /// use zlim_core::schedule::{
    ///     ExecutorKind, MultiThreadedExecutor, ScheduleLabel, SingleThreadedExecutor,
    /// };
    ///
    /// #[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
    /// struct Update;
    ///
    /// // Force serial execution regardless of the platform default.
    /// let serial = Schedule::with_executor(Update, Box::new(SingleThreadedExecutor::new()));
    /// assert_eq!(serial.executor_kind(), ExecutorKind::SingleThreaded);
    ///
    /// // Or explicitly opt into parallel execution.
    /// let parallel = Schedule::with_executor(Update, Box::new(MultiThreadedExecutor::new()));
    /// assert_eq!(parallel.executor_kind(), ExecutorKind::MultiThreaded);
    /// ```
    pub fn with_executor(label: impl ScheduleLabel, executor: Box<dyn JobExecutor>) -> Self {
        Self {
            label: label.intern(),
            allocator: Default::default(),
            buffer: Default::default(),
            ordering: Default::default(),
            confict: Default::default(),
            schedule: Default::default(),
            groups: Default::default(),
            sync_points: HashMap::with_hasher(NoopState),
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
        self.allocator.mapper.keys().copied()
    }

    /// Iterates the names of all inserted groups in this schedule.
    pub fn groups(&self) -> impl ExactSizeIterator<Item = &'static str> + '_ {
        self.groups.keys().copied()
    }

    /// Returns `true` if the job is registered in this schedule.
    pub fn contains_job(&self, id: JobId) -> bool {
        self.allocator.contains(id)
    }

    /// Returns `true` if the group is registered in this schedule.
    pub fn contains_group(&self, name: &str) -> bool {
        self.groups.contains_key(name)
    }

    /// Returns the executor kind of this schedule.
    pub fn executor_kind(&self) -> ExecutorKind {
        self.executor.kind()
    }
}

// -----------------------------------------------------------------------------
// Modify

impl Schedule {
    /// Inserts a standalone job described by `db`.
    ///
    /// The job id uses the [`JobGroup::ANONYMOUS`] group name; no group
    /// storage is created for it.  Returns `false` if the job already exists.
    #[inline(never)]
    fn insert_db(&mut self, db: JobDB) -> bool {
        let id = JobId::isolated(db.name);

        if self.allocator.get_node(id).is_some() {
            core::hint::cold_path();
            return false;
        }

        self.is_changed = true;

        let node = self.allocator.insert(id);
        self.buffer.insert(node, (db.ctor)(JobGroup::ANONYMOUS));
        self.ordering.insert_node(node);

        true
    }

    /// Inserts a standalone job looked up from the global [`JobDB`] registry
    /// by name.
    ///
    /// Returns `false` if the job is not registered or already exists.
    ///
    /// [`JobDB`]: crate::job::JobDB
    pub fn insert_by_name(&mut self, name: &str) -> bool {
        let Some(db) = JobDB::get(name) else {
            core::hint::cold_path();
            log::warn!("Missing Job named `{name}`, perhaps JobDB is not registered.");
            return false;
        };
        self.insert_db(db)
    }

    /// Inserts a standalone job from a [`JobLabel`].
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
    /// assert!(schedule.insert::<CollisionCheck>());
    /// // Inserting the same job twice returns `false`.
    /// assert!(!schedule.insert::<CollisionCheck>());
    /// ```
    ///
    /// [`JobLabel`]: crate::job::JobLabel
    pub fn insert<L: JobLabel>(&mut self) -> bool {
        let name = L::name();
        if let Some(db) = JobDB::get(name) {
            self.insert_db(db)
        } else {
            self.insert_db(L::database())
        }
    }

    /// Removes the job with the given id.
    ///
    /// Returns `false` if the job is not registered in this schedule.
    #[inline(never)]
    fn remove_job(&mut self, id: JobId) -> bool {
        let Some(node) = self.allocator.remove(id) else {
            core::hint::cold_path();
            return false;
        };

        self.is_changed = true;
        self.buffer.remove(node);
        self.ordering.remove_node(node);
        self.confict.remove(node);

        if let Some(points) = self.sync_points.remove(&id) {
            ::core::hint::cold_path();
            for point in points {
                let _ = self.remove_job(point);
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
        let Some(b) = self.allocator.get_node(before) else {
            ::core::hint::cold_path();
            let label = self.label;
            log::warn!("Missing Job `{before}` in Schedule `{label:?}`, skiped.\n\t{caller}");
            return;
        };
        let Some(a) = self.allocator.get_node(after) else {
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
    /// See [`JobGroup`]'s documentation for `StrongOrder` information.
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

            let Some(before_node) = self.allocator.get_node(before) else {
                // The warning has already been triggered once during the order
                // insertion, there is no need to output it again here.
                continue;
            };
            let Some(after_node) = self.allocator.get_node(after) else {
                // The warning has already been triggered once during the order
                // insertion, there is no need to output it again here.
                continue;
            };

            let object = self.buffer.get_job(before_node).unwrap();
            if !object.job.flags().intersects(SystemFlags::DEFERRED) {
                continue;
            }

            let point = SyncPoint::new(before, after);
            let job_id = point.id;

            if !self.allocator.contains(job_id) {
                let job_node = self.allocator.insert(job_id);

                self.buffer.insert(job_node, Box::new(point));
                self.ordering.insert_node(job_node); // optional

                self.ordering.insert_strong_order(before_node, job_node);
                self.ordering.insert_weak_order(job_node, after_node);

                self.confict.set_exclusive(job_node);

                self.sync_points.entry(before).or_default().push(job_id);
                self.sync_points.entry(after).or_default().push(job_id);
            }
        }
    }

    /// Insert a weak order by given sequence.
    ///
    /// See [`JobGroup`]'s documentation for `WeakOrder` information.
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

            let Some(before_node) = self.allocator.get_node(before) else {
                // The warning has already been triggered once during the order
                // insertion, there is no need to output it again here.
                continue;
            };
            let Some(after_node) = self.allocator.get_node(after) else {
                // The warning has already been triggered once during the order
                // insertion, there is no need to output it again here.
                continue;
            };

            let object = self.buffer.get_job(before_node).unwrap();
            if !object.job.flags().intersects(SystemFlags::DEFERRED) {
                continue;
            }

            let point = SyncPoint::new(before, after);
            let job_id = point.id;

            if !self.allocator.contains(job_id) {
                let job_node = self.allocator.insert(job_id);

                self.buffer.insert(job_node, Box::new(point));
                self.ordering.insert_node(job_node); // optional

                self.ordering.insert_strong_order(before_node, job_node);
                self.ordering.insert_weak_order(job_node, after_node);

                self.confict.set_exclusive(job_node);

                self.sync_points.entry(before).or_default().push(job_id);
                self.sync_points.entry(after).or_default().push(job_id);
            }
        }
    }

    /// Insert a relaxed order by given sequence.
    ///
    /// See [`JobGroup`]'s documentation for `RelaxedOrder` information.
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

impl Schedule {
    /// Inserts every job of `group` together with its ordering constraints.
    ///
    /// Returns `false` if the group already exists.  Jobs missing from the
    /// global [`JobDB`] registry are skipped with a warning.
    ///
    /// [`JobDB`]: crate::job::JobDB
    #[inline(never)]
    fn insert_group_object(&mut self, group: JobGroup, caller: DebugLocation) -> bool {
        let name = group.name;

        if self.groups.contains_key(name) {
            core::hint::cold_path();
            return false;
        }

        self.is_changed = true;

        // Create and insert all jobs of the group.
        for &job in group.jobs {
            let job_name = job.name();
            let Some(db) = JobDB::get(job_name) else {
                core::hint::cold_path();
                log::warn!(
                    "Missing job `{job_name}` in group `{name}`, \
                    perhaps JobDB is not registered.\n\t{caller}"
                );
                continue;
            };

            if !self.allocator.contains(job) {
                let node = self.allocator.insert(job);
                self.buffer.insert(node, (db.ctor)(name));
                self.ordering.insert_node(node);
            }
        }

        // Insert ordering constraints; edges referencing missing jobs are
        // skipped.
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

        if self.executor.kind() == ExecutorKind::SingleThreaded {
            self.groups.insert(name, group);
            return true;
        }

        // Multi-threaded executors need apply-deferred helper jobs
        // inserted between deferred jobs and their successors.
        for (before, after) in group.order.iter().chain(group.weak_order) {
            // [0] is `GroupBegin`, [1] is `GroupEnd`.
            if *before == 0 || *after == 1 {
                continue;
            }

            let before: JobId = group.jobs[(*before) as usize];
            let after: JobId = group.jobs[(*after) as usize];

            let Some(before_node) = self.allocator.get_node(before) else {
                // The warning has already been triggered once during the order
                // insertion, there is no need to output it again here.
                continue;
            };
            let Some(after_node) = self.allocator.get_node(after) else {
                // The warning has already been triggered once during the order
                // insertion, there is no need to output it again here.
                continue;
            };

            let object = self.buffer.get_job(before_node).unwrap();
            if !object.job.flags().intersects(SystemFlags::DEFERRED) {
                continue;
            }

            let point = SyncPoint::new(before, after);
            let job_id = point.id;

            if !self.allocator.contains(job_id) {
                let job_node = self.allocator.insert(job_id);

                self.buffer.insert(job_node, Box::new(point));
                self.ordering.insert_node(job_node); // optional

                self.ordering.insert_strong_order(before_node, job_node);
                self.ordering.insert_weak_order(job_node, after_node);

                self.confict.set_exclusive(job_node);

                self.sync_points.entry(before).or_default().push(job_id);
                self.sync_points.entry(after).or_default().push(job_id);
            }
        }

        self.groups.insert(name, group);

        true
    }

    /// Inserts a group looked up from the global [`JobGroup`] registry by
    /// name.
    ///
    /// Returns `false` if the group is not registered or already exists.
    ///
    /// [`JobGroup`]: crate::job::JobGroup
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn insert_group_by_name(&mut self, name: &str) -> bool {
        let Some(group) = JobGroup::get(name).copied() else {
            core::hint::cold_path();
            return false;
        };
        self.insert_group_object(group, DebugLocation::caller())
    }

    /// Inserts a group from a [`JobGroupLabel`].
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
    /// assert!(schedule.insert_group::<Gameplay>());
    /// assert!(schedule.contains_group("gameplay"));
    /// assert!(schedule.contains_job(JobId::new("input_read", "gameplay")));
    /// ```
    ///
    /// [`JobGroupLabel`]: crate::job::JobGroupLabel
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn insert_group<G: JobGroupLabel>(&mut self) -> bool {
        let name = G::name();
        if let Some(group) = JobGroup::get(name) {
            self.insert_group_object(*group, DebugLocation::caller())
        } else {
            self.insert_group_object(G::group(), DebugLocation::caller())
        }
    }

    /// Removes the group with the given name and every job created for it.
    ///
    /// Job ids that no longer exist are skipped.  Returns `false` if the
    /// group is not registered in this schedule.
    #[inline(never)]
    pub fn remove_group_by_name(&mut self, name: &str) -> bool {
        let Some(group) = self.groups.remove(name) else {
            core::hint::cold_path();
            return false;
        };

        self.is_changed = true;

        for &id in group.jobs {
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
        let buffer = &mut self.buffer;

        schedule.conflict = ConflictTable::new(0);
        schedule.incoming.clear();
        schedule.outgoing.clear();
        schedule.strong_incoming.clear();
        schedule.strong_outgoing.clear();
        schedule.flags.clear();
        schedule.pool = Bump::new(256); // placeholder

        let jobs = core::mem::take(&mut schedule.jobs);
        let nodes = core::mem::take(&mut schedule.nodes);
        let access_tables = core::mem::take(&mut schedule.access_tables);

        for ((&node, job), access) in nodes.iter().zip(jobs).zip(access_tables) {
            buffer.recycle(node, job, access);
        }
    }

    /// Initializes newly inserted jobs and rebuilds the conflict graph.
    #[inline(never)]
    fn init_systems(&mut self, world: &World) {
        let multi_threaded = self.executor.kind() == ExecutorKind::MultiThreaded;
        let buffer = &mut self.buffer;
        let conflict = &mut self.confict;

        let mut uninit = core::mem::take(&mut buffer.uninit);

        uninit.sort();
        uninit.dedup();

        if zlim_task::cfg::multi_thread!() && uninit.len() > 2 {
            zlim_task::MainTaskPool::get().scope(|s| {
                let buf = buffer as *mut Buffer;
                uninit.iter().for_each(|&node| {
                    // SAFETY: already deduplicated above
                    if let Some(obj) = unsafe { &mut *buf }.get_job_mut(node) {
                        s.spawn(async move {
                            obj.job.initialize(world);
                            let mut table = AccessTable::new();
                            obj.job.register_access(&mut table);
                            obj.access = table;
                        });
                    }
                });
            });
        } else {
            // Initialize new jobs and collect their access tables.
            uninit.iter().for_each(|&node| {
                if let Some(obj) = buffer.get_job_mut(node) {
                    obj.job.initialize(world);
                    let mut table = AccessTable::new();
                    obj.job.register_access(&mut table);
                    obj.access = table;
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
            let nodes = buffer.nodes();

            uninit.iter().for_each(|&a| {
                let obj_a = buffer.get_job(a).unwrap();

                if obj_a.job.flags().intersects(SystemFlags::EXCLUSIVE) {
                    conflict.set_exclusive(a);
                    return;
                }

                nodes.iter().for_each(|&b| {
                    if a == b {
                        return;
                    }
                    let Some(obj_b) = buffer.get_job(b) else {
                        return;
                    };
                    if !obj_a.access.parallelizable(&obj_b.access) {
                        conflict.set_conflict(a, b);
                    }
                });
            });
        }
    }

    /// Rebuilds the executor-ready representation from the ordering graph.
    #[inline(never)]
    fn build_schedule(&mut self) {
        let buffer = &mut self.buffer;
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
            let obj = buffer.take_job(node);
            schedule.access_tables.push(obj.access);
            schedule.jobs.push(obj.job);
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
            let k = len.highest_one().unwrap_or(0);
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
        let id_of = |node: Node| -> JobId { self.allocator.get_id(node).expect("should exist") };

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
    /// schedule.insert::<Greet>();
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
        self.update(world);

        let handler = world.error_handler();
        self.executor.exec(&mut self.schedule, world, handler);

        // world.flush(); // Should be flushed by Executor.
    }
}

// -----------------------------------------------------------------------------
