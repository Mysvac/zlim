// -----------------------------------------------------------------------------
// Modules

mod multi;
mod single;

pub use multi::MultiThreadedExecutor;
pub use single::SingleThreadedExecutor;

// -----------------------------------------------------------------------------
// Inline implementation

use fixedbitset::FixedBitSet;
use zlim_utils::mem::Bump;

use super::Node;
use crate::error::ErrorHandler;
use crate::job::Job;
use crate::schedule::InternedScheduleLabel;
use crate::system::{AccessTable, SystemFlags};
use crate::world::World;

// -----------------------------------------------------------------------------
// ConflictTable

/// A square conflict matrix for tracking pairwise conflicts between jobs.
pub struct ConflictTable {
    // We use complete matrices instead of triangles.
    // This has better cache affinity during traversal.
    lines: usize,
    table: FixedBitSet,
}

impl ConflictTable {
    /// Creates an empty conflict table with `lines * lines` bits.
    ///
    /// A table created with zero lines stays empty and reports **every**
    /// pair as conflicting (see [`ConflictTable::is_conflict`]).  This is
    /// used by single-threaded schedules, which run serially and never
    /// consult the matrix.
    ///
    /// # Panics
    ///
    /// Panics if `lines` exceeds `u16::MAX`.
    pub fn new(lines: usize) -> Self {
        assert!(
            lines <= (u16::MAX as usize),
            "The lines of `ConflictTable` cannot exceed u16::MAX"
        );
        Self {
            lines,
            table: FixedBitSet::with_capacity(lines * lines),
        }
    }

    /// Returns the number of rows (columns) in the table.
    ///
    /// The returned value is at most `u16::MAX`.  `0` means the table is
    /// empty, i.e. the schedule never computes conflict information.
    pub fn lines(&self) -> usize {
        self.lines
    }

    /// Marks `(a, b)` as conflicting in the table.
    ///
    /// # Safety
    /// `a` and `b` must be valid matrix indices in `[0, self.lines)`.
    pub unsafe fn set_conflict(&mut self, a: u16, b: u16) {
        let index = a as usize * self.lines + b as usize;
        debug_assert!(index <= self.lines * self.lines);
        unsafe { self.table.insert_unchecked(index) }
    }

    /// Marks every pair involving `a` as conflicting.
    ///
    /// This is used for exclusive systems.
    ///
    /// # Safety
    /// `a` must be a valid matrix index in `[0, self.lines)`.
    pub unsafe fn set_exclusive(&mut self, a: u16) {
        for line in 0..self.lines {
            let index = a as usize * self.lines + line;
            unsafe { self.table.insert_unchecked(index) };
        }
        for line in 0..self.lines {
            let index = a as usize + line * self.lines;
            unsafe { self.table.insert_unchecked(index) };
        }
    }

    /// Returns whether `(a, b)` conflicts.
    ///
    /// An empty table (zero lines, as produced for single-threaded
    /// schedules) reports **every** pair as conflicting, so unknown access
    /// patterns degrade to fully serialized execution instead of racing.
    ///
    /// # Safety
    /// `a` and `b` must be valid matrix indices in `[0, self.lines)`.
    #[inline(always)]
    pub fn is_conflict(&self, a: u16, b: u16) -> bool {
        if self.lines == 0 {
            return true;
        }
        let index = a as usize * self.lines + b as usize;
        debug_assert!(index <= self.lines * self.lines);
        unsafe { self.table.contains_unchecked(index) }
    }
}

// -----------------------------------------------------------------------------

impl Default for ConflictTable {
    fn default() -> Self {
        Self::new(0)
    }
}

// -----------------------------------------------------------------------------

/// The compiled, executor-ready representation of a schedule.
///
/// Produced by [`Schedule::update`] and consumed by [`JobExecutor`]
/// implementations. Jobs are stored in execution-index order together with
/// their flags, dependency counters/adjacency, access tables, and a conflict
/// lookup table.
///
/// [`Schedule::update`]: crate::schedule::Schedule::update
pub struct JobSchedule {
    pub(super) label: InternedScheduleLabel,
    pub(super) jobs: Vec<Box<dyn Job>>,
    pub(super) flags: Vec<SystemFlags>,
    pub(super) conflict: ConflictTable,
    pub(super) nodes: Vec<Node>,
    pub(super) incoming: Vec<u16>,
    pub(super) outgoing: Vec<&'static [u16]>,
    pub(super) strong_incoming: Vec<u16>,
    pub(super) strong_outgoing: Vec<&'static [u16]>,
    pub(super) pool: Bump,
    pub(super) access_tables: Vec<AccessTable>,
    #[cfg(feature = "trace")]
    pub(super) spans: Vec<zlim_log::Span>,
}

impl JobSchedule {
    /// Create a empty JobSchedule
    pub(crate) fn new(label: InternedScheduleLabel) -> Self {
        Self {
            label,
            jobs: Vec::new(),
            flags: Vec::new(),
            conflict: ConflictTable::new(0),
            nodes: Vec::new(),
            incoming: Vec::new(),
            outgoing: Vec::new(),
            strong_incoming: Vec::new(),
            strong_outgoing: Vec::new(),
            pool: Bump::new(256),
            access_tables: Vec::new(),
            #[cfg(feature = "trace")]
            spans: Vec::new(),
        }
    }
}

unsafe impl Sync for JobSchedule {}
unsafe impl Send for JobSchedule {}

/// A structured, read-mostly view over a compiled [`JobSchedule`].
///
/// Executors destructure this view to read dependency metadata while
/// mutating job objects in place.
pub struct JobScheduleView<'s> {
    /// Schedule Label
    pub label: InternedScheduleLabel,
    /// Mutable access to compiled system objects.
    pub jobs: &'s mut [Box<dyn Job>],
    /// Readonly access to all bitflags representing system requirements.
    pub flags: &'s [SystemFlags],
    /// Conflict lookup table used by the executor.
    ///
    /// Empty (zero lines) for single-threaded schedules; every query then
    /// reports a conflict.
    pub conflict: &'s ConflictTable,
    /// Stable system keys aligned with all other columns.
    pub nodes: &'s [Node],
    /// Number of normal dependency predecessors for each system index.
    pub incoming: &'s [u16],
    /// Normal dependency adjacency by system index.
    pub outgoing: &'s [&'s [u16]],
    /// Number of run-condition predecessors for each system index.
    pub strong_incoming: &'s [u16],
    /// Run-condition adjacency by system index.
    pub strong_outgoing: &'s [&'s [u16]],
    /// Readonly access to AccessTable.
    pub access_tables: &'s [AccessTable],
    /// job spans for tracing
    #[cfg(feature = "trace")]
    pub spans: &'s mut [zlim_log::Span],
}

impl JobSchedule {
    /// Returns a structured view over compiled schedule data.
    pub fn view(&mut self) -> JobScheduleView<'_> {
        let JobSchedule {
            label,
            jobs,
            flags,
            nodes,
            conflict,
            incoming,
            outgoing,
            strong_incoming,
            strong_outgoing,
            access_tables,
            #[cfg(feature = "trace")]
            spans,
            ..
        } = self;

        JobScheduleView {
            label: *label,
            jobs,
            flags,
            conflict,
            nodes,
            incoming,
            outgoing,
            strong_incoming,
            strong_outgoing,
            access_tables,
            #[cfg(feature = "trace")]
            spans,
        }
    }

    /// Returns compiled systems in execution-index order.
    pub fn jobs(&self) -> &[Box<dyn Job>] {
        &self.jobs
    }

    /// Returns mutable access to compiled systems.
    pub fn jobs_mut(&mut self) -> &mut [Box<dyn Job>] {
        &mut self.jobs
    }

    /// Returns compiled system nodes.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Returns the conflict lookup table.
    ///
    /// Always empty in single threaded mode.
    pub fn conflict(&self) -> &ConflictTable {
        &self.conflict
    }

    /// Returns normal dependency incoming counts.
    pub fn incoming(&self) -> &[u16] {
        &self.incoming
    }

    /// Returns normal dependency adjacency lists.
    pub fn outgoing(&self) -> &[&[u16]] {
        &self.outgoing
    }

    /// Returns run-condition dependency incoming counts.
    pub fn strong_incoming(&self) -> &[u16] {
        &self.strong_incoming
    }

    /// Returns run-condition dependency adjacency lists.
    pub fn strong_outgoing(&self) -> &[&[u16]] {
        &self.strong_outgoing
    }

    /// Returns read-only access to the compiled access tables.
    pub fn access_tables(&mut self) -> &[AccessTable] {
        &mut self.access_tables
    }
}

// -----------------------------------------------------------------------------
// ExecutorKind

/// Execution strategy used by a schedule.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ExecutorKind {
    /// Always run systems on a single thread.
    SingleThreaded,
    /// Run independent systems in parallel on multiple threads.
    MultiThreaded,
}

impl Default for ExecutorKind {
    fn default() -> Self {
        if zlim_task::cfg::multi_thread!() {
            Self::MultiThreaded
        } else {
            Self::SingleThreaded
        }
    }
}

// -----------------------------------------------------------------------------
// JobExecutor

/// Runtime interface for executing a compiled job schedule.
///
/// Implementors are responsible for traversing dependency metadata in
/// [`JobSchedule`] and invoking systems in a valid order while handling
/// errors through the provided [`ErrorHandler`].
///
/// [`ErrorHandler`]: crate::error::ErrorHandler
pub trait JobExecutor: Send + Sync {
    /// Returns the executor flavor.
    fn kind(&self) -> ExecutorKind;

    /// Initializes executor-internal state from a compiled schedule.
    ///
    /// Called when the schedule shape changes or when an executor is first used.
    fn init(&mut self, schedule: &JobSchedule);

    /// Executes one schedule tick.
    ///
    /// Implementations should respect dependency ordering and may parallelize
    /// independent systems depending on [`ExecutorKind`].
    fn exec(&mut self, schedule: &mut JobSchedule, world: &mut World, handler: ErrorHandler);
}

// -----------------------------------------------------------------------------
