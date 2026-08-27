//! Required-component support.
//!
//! A component may declare required components through its
//! [`Component::REQUIRED`] constant.  Whenever the component is registered,
//! collected into a bundle, or inserted into an entity, its required
//! components are handled automatically:
//!
//! - **register** — required components are registered recursively;
//! - **collect** — required components are added to the target table's
//!   component set;
//! - **write** — required components that were not provided explicitly are
//!   initialised with their `Default` value.
//!
//! Every required component must implement [`Default`].

use super::{Component, ComponentCollector, ComponentWriter};

// -----------------------------------------------------------------------------
// Required

/// A v-table that bundles the three required-component operations —
/// registration, collection, and writing — for a [`RequiredComponents`]
/// type.
///
/// Built via [`Required::from`] and stored in [`Component::REQUIRED`], it
/// lets the ECS drive required components uniformly without knowing their
/// concrete types.
///
/// # Example
///
/// ```rust
/// use core::any::TypeId;
/// use zlim_core::prelude::*;
/// use zlim_core::component::Required;
///
/// #[derive(TypePath, Component, Clone, Default)]
/// struct Health {
///     value: f32,
/// }
///
/// #[derive(TypePath, Component, Clone, Default)]
/// struct Armor {
///     value: f32,
/// }
///
/// // The derive builds the v-table from `#[require(...)]`:
/// #[derive(TypePath, Component, Clone)]
/// #[require(Health, Armor)]
/// struct Player;
///
/// // ...but the container can also be driven manually:
/// let required = Required::from::<(Health, Armor)>();
/// required.register(); // registers both types (idempotent)
///
/// // The manual registration registered both types:
/// assert!(ComponentDB::get_by_type(TypeId::of::<Health>()).is_some());
/// assert!(ComponentDB::get_by_type(TypeId::of::<Armor>()).is_some());
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Required {
    register: fn(),
    collect: fn(&mut ComponentCollector),
    write: unsafe fn(&mut ComponentWriter),
}

impl Required {
    /// Creates a [`Required`] container from a [`RequiredComponents`] type.
    #[inline(always)]
    pub const fn from<T: RequiredComponents>() -> Self {
        Self {
            register: T::required_register,
            collect: T::required_collect,
            write: T::required_write,
        }
    }

    /// Registers all required components.
    ///
    /// This includes the type itself.
    #[inline(always)]
    pub fn register(&self) {
        (self.register)()
    }

    /// Collects (and registers) all required components using the given
    /// collector.  This includes the type itself.
    #[inline(always)]
    pub fn collect(&self, collector: &mut ComponentCollector) {
        (self.collect)(collector)
    }

    /// Writes all required components using the given writer.
    ///
    /// # Safety
    /// See [`ComponentWriter`] and [`RequiredComponents`].
    #[inline(always)]
    pub unsafe fn write(&self, writer: &mut ComponentWriter) {
        unsafe { (self.write)(writer) }
    }
}

// -----------------------------------------------------------------------------
// RequiredComponents

/// A trait for types that have required components.
///
/// This trait defines the operations needed to manage component
/// dependencies: registration, collection, and writing.  It is implemented
/// for single components (which must implement [`Default`]) and for tuples
/// of up to 12 components, allowing complex dependency trees to be expressed
/// through composition.
///
/// # Safety
///
/// This trait is unsafe because incorrect implementations could lead to:
/// - Missing component registrations
/// - Invalid component writes
/// - Memory unsafety in the component system
///
/// Implementations must ensure that all required components are properly
/// registered, collected, and written.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a `RequiredComponents`",
    label = "invalid `RequiredComponents`",
    note = "A required component can be a single component or a tuple of up to 12 \
            components; every component must implement `Default`."
)]
pub unsafe trait RequiredComponents {
    /// Registers all required components.
    ///
    /// The order is not important, and duplicate registrations are allowed.
    fn required_register();

    /// Collects all required components using the given collector.
    ///
    /// The order is not important, and duplicate collection is allowed.
    fn required_collect(collector: &mut ComponentCollector);

    /// Writes all required components using the given writer.
    ///
    /// # Safety
    /// - It may write to memory locations that must be valid
    /// - The writer's internal state must be properly initialised
    fn required_write(writer: &mut ComponentWriter);
}

unsafe impl RequiredComponents for () {
    #[inline(always)]
    fn required_register() {}
    #[inline(always)]
    fn required_collect(_collector: &mut ComponentCollector) {}
    #[inline(always)]
    fn required_write(_writer: &mut ComponentWriter) {}
}

/// A single required component: must implement [`Default`].
///
/// Writing also recurses into the component's own required components, so
/// transitive dependencies are filled in as well.
unsafe impl<T: Component + Default> RequiredComponents for T {
    #[inline(always)]
    fn required_register() {
        T::register();
    }

    #[inline(always)]
    fn required_collect(collector: &mut ComponentCollector) {
        collector.collect_required::<T>();
    }

    #[inline(always)]
    fn required_write(writer: &mut ComponentWriter) {
        writer.write_if_uninit::<T>(T::default);

        if let Some(required) = T::REQUIRED {
            unsafe { required.write(writer) };
        }
    }
}

// -----------------------------------------------------------------------------
// Tuple implementations

macro_rules! impl_required_for_tuple {
    (0: []) => {};
    (1 : [ $index:tt : $name:ident ]) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        unsafe impl<$name: RequiredComponents> RequiredComponents for ($name,) {
            #[inline(always)]
            fn required_register() {
                <$name>::required_register();
            }

            #[inline(always)]
            fn required_collect(collector: &mut ComponentCollector) {
                <$name>::required_collect(collector);
            }

            #[inline(always)]
            fn required_write(writer: &mut ComponentWriter) {
                <$name>::required_write(writer);
            }
        }
    };
    ($num:literal : [$($index:tt : $name:ident),*]) => {
        #[cfg_attr(docsrs, doc(hidden))]
        unsafe impl<$($name: RequiredComponents),*> RequiredComponents for ($($name,)*) {
            fn required_register() {
                $( <$name>::required_register(); )*
            }

            fn required_collect(collector: &mut ComponentCollector) {
                $( <$name>::required_collect(collector); )*
            }

            fn required_write(writer: &mut ComponentWriter) {
                $( <$name>::required_write(writer); )*
            }
        }
    };
}

zlim_utils::range_invoke!(impl_required_for_tuple, 12);

// -----------------------------------------------------------------------------
