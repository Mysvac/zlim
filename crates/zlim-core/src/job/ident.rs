//! Job identifier used to key jobs in the scheduler.

use core::fmt::{Debug, Display};
use core::hash::{Hash, Hasher};

use zlim_utils::hash::FixedState;

// -----------------------------------------------------------------------------
// JobId

/// A unique identifier for a job instance.
///
/// A `JobId` combines the job's `name` with its `group` (empty for standalone
/// jobs) into a stable, hashable identity used by the scheduler.
///
/// # Example
///
/// ```rust
/// use zlim_core::job::JobId;
///
/// // A job inside a group carries both names:
/// let id = JobId::new("my_job", "my_group");
/// assert_eq!(id.name(), "my_job");
/// assert_eq!(id.group(), "my_group");
///
/// // Standalone jobs use an empty group:
/// let standalone = JobId::new("my_job", "");
/// assert_eq!(standalone.group(), "");
/// ```
#[derive(Clone, Copy)]
pub struct JobId {
    name: &'static str,
    group: &'static str,
    hash: u64,
}

impl JobId {
    /// Creates a new job id from a job name and group name.
    #[inline(never)]
    pub fn new(name: &'static str, group: &'static str) -> Self {
        let mut state = FixedState::HASHER;
        group.hash(&mut state);
        name.hash(&mut state);
        Self {
            name,
            group,
            hash: state.finish(),
        }
    }

    /// Create a JobId that does not belong to any group.
    ///
    /// Its group will be marked as `JobGrop::ANONYMOUS`.
    #[inline(never)]
    pub fn isolated(name: &'static str) -> Self {
        use super::group::JobGroup;

        let mut state = FixedState::HASHER;
        JobGroup::ANONYMOUS.hash(&mut state);
        name.hash(&mut state);

        Self {
            name,
            group: JobGroup::ANONYMOUS,
            hash: state.finish(),
        }
    }

    /// Returns the job's name.
    #[inline(always)]
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the group the job belongs to (empty for standalone jobs).
    #[inline(always)]
    pub fn group(&self) -> &'static str {
        self.group
    }
}

impl PartialEq for JobId {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.name == other.name && self.group == other.group
    }
}

impl Eq for JobId {}

impl Hash for JobId {
    #[inline(always)]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl Display for JobId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.group.is_empty() {
            write!(f, "{}(isolated)", self.name)
        } else {
            write!(f, "{}(in {})", self.name, self.group)
        }
    }
}

impl Debug for JobId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self, f)
    }
}

// -----------------------------------------------------------------------------
