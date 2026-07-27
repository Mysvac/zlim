// ----------------------------------------------------------------------------
// LocalExecutor & MainExecutor

mod common;

use common::{LocalExecutor, MainExecutor};

// ----------------------------------------------------------------------------
// TaskPool Implementaions

pub use impls::{Scope, TaskPool, TaskPoolBuilder, block_on, run_local};

zlim_cfg::switch! {
    #[cfg(feature = "single_thread")] => {
        pub use zlim_cfg::disabled as multi_thread;
        pub use zlim_cfg::enabled as single_thread;
        mod single;
        use single as impls;
    }
    zlim_os::cfg::wasm => {
        pub use zlim_cfg::disabled as multi_thread;
        pub use zlim_cfg::enabled as single_thread;
        mod wasm;
        use wasm as impls;
    }
    zlim_os::cfg::android => {
        pub use zlim_cfg::enabled as multi_thread;
        pub use zlim_cfg::disabled as single_thread;
        mod multi;
        use multi as impls;
    }
    zlim_os::cfg::windows => {
        pub use zlim_cfg::enabled as multi_thread;
        pub use zlim_cfg::disabled as single_thread;
        mod multi;
        use multi as impls;
    }
    zlim_os::cfg::linux => {
        pub use zlim_cfg::enabled as multi_thread;
        pub use zlim_cfg::disabled as single_thread;
        mod multi;
        use multi as impls;
    }
    zlim_os::cfg::macos => {
        pub use zlim_cfg::enabled as multi_thread;
        pub use zlim_cfg::disabled as single_thread;
        mod multi;
        use multi as impls;
    }
    _ => {
        pub use zlim_cfg::disabled as multi_thread;
        pub use zlim_cfg::enabled as single_thread;
        mod single;
        use single as impls;
    }
}

// ----------------------------------------------------------------------------
// Static TaskPool

macro_rules! taskpool {
    ($(#[$attr:meta])* ($static:ident, $type:ident)) => {
        static $static: ::std::sync::OnceLock<$type> = ::std::sync::OnceLock::new();

        $(#[$attr])*
        #[derive(Debug)]
        #[repr(transparent)]
        pub struct $type(TaskPool);

        impl $type {
            #[doc = concat!(
                        " Gets the global [`", stringify!($type), "`] instance,",
                        " or initializes it with `f`.",
                    )]
            pub fn get_or_init(f: impl FnOnce() -> TaskPool) -> &'static Self {
                $static.get_or_init(|| Self(f()))
            }

            #[doc = concat!(
                        " Attempts to get the global [`", stringify!($type), "`] instance, ",
                        "or returns `None` if it is not initialized.",
                    )]
            pub fn try_get() -> Option<&'static Self> {
                $static.get()
            }

            #[doc = concat!(" Gets the global [`", stringify!($type), "`] instance.")]
            #[doc = ""]
            #[doc = " # Panics"]
            #[doc = " Panics if the global instance has not been initialized yet."]
            pub fn get() -> &'static Self {
                $static.get().expect(concat!(
                    "The ",
                    stringify!($type),
                    " has not been initialized yet. Please call ",
                    stringify!($type),
                    "::get_or_init beforehand."
                ))
            }
        }

        impl ::core::ops::Deref for $type {
            type Target = TaskPool;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    };
}

taskpool! {
    /// A newtype for the main task pool.
    ///
    /// Note: "Main" here refers to this being the primary task pool,
    /// not that it only runs on the main thread.
    ///
    /// This pool handles CPU-intensive work that must be completed in single
    /// frame. It also serves as the underlying executor for various parallel
    /// algorithms and data processing operations within the engine.
    ///
    /// For work that may span multiple frames without blocking frame delivery,
    /// use [`AsyncTaskPool`]. For IO-bound operations, use [`IoTaskPool`].
    ///
    /// See [`TaskPool`] documentation for details.
    (MAIN_TASK_POOL, MainTaskPool)
}

taskpool! {
    /// A newtype for a task pool for *Async* CPU-intensive work.
    ///
    /// i.g. tasks that may span across multiple frames.
    ///
    /// See [`TaskPool`] documentation for details.
    (ASYNC_TASK_POOL, AsyncTaskPool)
}

taskpool! {
    /// A newtype for a task pool for IO-intensive work.
    ///
    /// i.e. tasks that spend very little time in a "woken" state.
    ///
    /// See [`TaskPool`] documentation for details.
    (IO_TASK_POOL, IoTaskPool)
}
