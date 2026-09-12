use core::sync::atomic::AtomicUsize;

use zlim_utils::ext::CachePadded;

// -----------------------------------------------------------------------------
// AssetServerStats

/// Tracks statistics of the asset server.
pub(crate) struct Stats {
    /// The number of load tasks that have been started.
    pub started_load_tasks: AtomicUsize,
}

// -----------------------------------------------------------------------------
// AssetServerData

pub(crate) struct AssetServerData {
    pub(crate) stats: CachePadded<Stats>,
    // TODO
}

// -----------------------------------------------------------------------------
// AssetServerStats Methods

// Simple methods, no need documentation
impl AssetServerData {
    #[expect(unused, reason = "todo")]
    #[inline]
    pub(crate) fn add_started_load_tasks(&self, num: usize) {
        use core::sync::atomic::Ordering::Relaxed;
        self.stats.started_load_tasks.fetch_add(num, Relaxed);
    }

    #[inline]
    pub(crate) fn get_started_load_tasks(&self) -> usize {
        use core::sync::atomic::Ordering::Relaxed;
        self.stats.started_load_tasks.load(Relaxed)
    }
}
