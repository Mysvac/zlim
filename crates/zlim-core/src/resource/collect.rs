//! Bulk (CTOR-driven) resource registration.

use std::sync::PoisonError;
use zlim_log as log;

use super::db::{ID_REGISTRY, PATH_REGISTRY, ResourceDB, TYPE_REGISTRY};
use super::resource::Resource;

// -----------------------------------------------------------------------------
// Collect
// -----------------------------------------------------------------------------

impl ResourceDB {
    /// Runs all deferred resource registrations submitted via the
    /// [`register_resource!`] macro.
    ///
    /// Non generic types marked with [`Resource`] derive macro will be
    /// automatically registered.
    ///
    /// This is called once at engine startup to batch-collect registration
    /// tokens from across the crate graph. Pre-reserving registry capacity
    /// before iteration improves registration throughput. The function is
    /// guarded by a [`std::sync::Once`] so it is safe to call multiple times.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_reflect::derive::TypePath;
    ///
    /// #[derive(TypePath, Resource)]
    /// struct Score(u32);
    ///
    /// // Force a registration so the collect pass has something to gather;
    /// // in the engine this runs once at startup. Calling it again is a no-op.
    /// ResourceDB::of::<Score>();
    /// ResourceDB::collect();
    /// ResourceDB::collect();
    ///
    /// // The type ends up in the global registry, reachable by type id.
    /// let db = ResourceDB::get_by_type(core::any::TypeId::of::<Score>()).unwrap();
    /// assert_eq!(db.type_name, "Score");
    /// ```
    ///
    /// [`Resource`]: crate::derive::Resource
    /// [`register_resource!`]: macro@crate::register_resource
    pub fn collect() {
        #[cold]
        #[inline(never)]
        fn collect_internal() {
            use __internal__::__ResourceReg__ as Reg;
            const PRE: usize = 100;

            let start = zlim_os::time::Instant::now();
            log::debug!("Collecting ResourceDB registrations...");

            {
                // pre-reserve, for better register speed.
                ID_REGISTRY
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .reserve(PRE);
                TYPE_REGISTRY
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .reserve(PRE);
                PATH_REGISTRY
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .reserve(PRE);
            }

            zlim_reg::iter::<Reg>().for_each(|r| {
                (r.0)();
            });

            let len: usize = {
                // post-reserve, for better hash performance.
                let len: usize = TYPE_REGISTRY
                    .read()
                    .unwrap_or_else(PoisonError::into_inner)
                    .len();
                let add: usize = len >> 1;
                TYPE_REGISTRY
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .reserve(add);
                PATH_REGISTRY
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .reserve(add);
                len
            };

            log::debug!(
                "ResourceDB({len}) collection finished in {:?}",
                start.elapsed()
            );
        }

        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(collect_internal);
    }
}

// -----------------------------------------------------------------------------
// __internal__
// -----------------------------------------------------------------------------

/// Internal module, public for resource registration.
#[doc(hidden)]
pub mod __internal__ {
    use super::{Resource, ResourceDB};

    /// A registration token that defers [`Resource::register`] for a type.
    ///
    /// Collecting these tokens via [`zlim_reg::collect!`] enables bulk
    /// registration at startup instead of incurring the cold-path cost on
    /// every first access.
    #[repr(transparent)]
    pub struct __ResourceReg__(pub(super) fn() -> &'static ResourceDB);

    impl __ResourceReg__ {
        /// Creates a registration token for type `T`.
        #[inline(always)]
        pub const fn of<R: Resource>() -> Self {
            Self(<R as Resource>::register)
        }
    }

    zlim_reg::collect!(__ResourceReg__);
}

// -----------------------------------------------------------------------------
// register_resource!
// -----------------------------------------------------------------------------

/// Registers one or more [`Resource`] types for deferred collection.
///
/// This macro submits registration tokens that are later collected by
/// [`ResourceDB::collect`]. Use it at the crate root or in a module to ensure
/// resource types are discoverable by the engine at startup.
///
/// Non-generic types marked with the [`Resource`] derive macro are
/// registered automatically and do not need this macro; it is mainly useful
/// for generic types, which cannot be auto-registered.
///
/// # Examples
///
/// ```no_run
/// use zlim_core::prelude::*;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Resource)]
/// struct MyResource;
///
/// #[derive(TypePath, Resource)]
/// struct AnotherResource;
///
/// register_resource!(MyResource, AnotherResource);
///
/// // Bulk registration runs once per program lifetime — the engine does
/// // this automatically at startup:
/// ResourceDB::collect();
/// assert!(ResourceDB::get_by_type(core::any::TypeId::of::<MyResource>()).is_some());
/// ```
///
/// [`Resource`]: crate::resource::Resource
/// [`ResourceDB::collect`]: crate::resource::ResourceDB::collect
#[macro_export]
macro_rules! register_resource {
    ($($ty:ty),* $(,)?) => {
        const _: () = {
            $(
                $crate::__macro_exports__::__submit!(
                    $crate::resource::__internal__::__ResourceReg__::of::<$ty>()
                    => $crate::resource::__internal__::__ResourceReg__
                );
            )*
        };
    };
}
