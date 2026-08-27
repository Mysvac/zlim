//! Bulk (CTOR-driven) component registration.

use std::sync::PoisonError;

use super::component::Component;
use super::db::{ComponentDB, ID_REGISTRY, PATH_REGISTRY, TYPE_REGISTRY};

// -----------------------------------------------------------------------------
// Collect
// -----------------------------------------------------------------------------

impl ComponentDB {
    /// Triggers bulk registration of all component types submitted via the
    /// [`register_component!`] macro or `#[derive(Component)]`.
    ///
    /// Internally iterates the `__ComponentReg__` registry, calling each
    /// registration function. The process is guarded by [`std::sync::Once`]
    /// so it only runs once per program lifetime.
    ///
    /// [`register_component!`]: crate::register_component
    pub fn collect() {
        #[cold]
        #[inline(never)]
        fn collect_internal() {
            use __internal__::__ComponentReg__ as Reg;
            const PRE: usize = 100;

            let start = zlim_os::time::Instant::now();
            zlim_log::debug!("Collecting ComponentDB registrations...");

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

            zlim_log::debug!(
                "ComponentDB({len}) collection finished in {:?}",
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

#[doc(hidden)]
pub mod __internal__ {
    use super::{Component, ComponentDB};

    /// A registration token that defers [`Component::register`] for a type.
    ///
    /// Collecting these tokens via [`zlim_reg::collect!`] enables bulk
    /// registration at startup instead of incurring the cold-path cost
    /// on every first access.
    #[repr(transparent)]
    pub struct __ComponentReg__(pub(super) fn() -> &'static ComponentDB);

    impl __ComponentReg__ {
        /// Creates a registration token for type `T`.
        #[inline(always)]
        pub const fn of<C: Component>() -> Self {
            Self(<C as Component>::register)
        }
    }

    zlim_reg::collect!(__ComponentReg__);
}

// -----------------------------------------------------------------------------
// register_component!
// -----------------------------------------------------------------------------

/// Submits one or more component types for bulk registration.
///
/// Equivalent to calling [`Component::register`] for each listed type,
/// but defers the actual work until [`ComponentDB::collect`] is called.
/// This amortizes the cold-path cost of lazy registration at startup.
/// The engine runs the bulk collection pass automatically once during
/// initialization (registered through `init`), so components submitted
/// here are already registered before user code runs.
///
/// `#[derive(Component)]` submits the type implicitly, so this macro is
/// only needed for manual `Component` implementations or when you want to
/// force registration upfront.
///
/// # Example
///
/// ```no_run
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Transform;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Velocity;
///
/// register_component!(Transform, Velocity);
///
/// // Bulk registration runs once per program lifetime — the engine does
/// // this automatically at startup:
/// ComponentDB::collect();
/// ```
#[macro_export]
macro_rules! register_component {
    ($($ty:ty),* $(,)?) => {
        const _: () = {
            $(
                $crate::__macro_exports__::__submit!(
                    $crate::component::__internal__::__ComponentReg__::of::<$ty>()
                    => $crate::component::__internal__::__ComponentReg__
                );
            )*
        };
    };
}
