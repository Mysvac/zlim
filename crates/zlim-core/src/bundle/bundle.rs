#![expect(clippy::module_inception, reason = "For better structure.")]

use zlim_ptr::OwningPtr;

use super::helper::{ComponentCollector, ComponentWriter};
use crate::component::Component;
use crate::ops::EntityOwned;

// -----------------------------------------------------------------------------
// Bundle
// -----------------------------------------------------------------------------

/// A set of components (and sub-bundles) that can be written to an entity
/// in a single spawn operation.
///
/// # Role
///
/// `Bundle` is the trait that powers entity spawning.  When you call
/// `world.spawn(bundle)`, the ECS:
///
/// 1. Calls [`collect`] to register every component type the bundle needs.
/// 2. Determines the target archetype from the collected component set.
/// 3. Allocates a row and calls [`write`] to copy component data into
///    storage.
/// 4. Calls [`apply_effect`] for any post-spawn side effects (only when
///    [`NEED_APPLY_EFFECT`] is `true`).
///
/// [`collect`]: Bundle::collect
/// [`write`]: Bundle::write
/// [`apply_effect`]: Bundle::apply_effect
/// [`NEED_APPLY_EFFECT`]: Bundle::NEED_APPLY_EFFECT
///
/// # Safety
///
/// Implementing this trait is `unsafe` because the ECS relies on the
/// implementor correctly reporting its component requirements and
/// writing data at the correct memory offsets.  Incorrect implementations
/// can cause undefined behavior.
///
/// Prefer using `#[derive(Bundle)]` or composing built-in bundles (tuples,
/// individual components) rather than implementing this trait manually.
///
/// # Derive macro
///
/// ```ignore
/// #[derive(Component, /* ... */)]
/// struct Position { x: f32, y: f32 }
///
/// #[derive(Component, /* ... */)]
/// struct Velocity { dx: f32, dy: f32 }
///
/// #[derive(Bundle)]
/// struct MovableBundle {
///     position: Position,
///     velocity: Velocity,
/// }
/// ```
///
/// # Tuple implementations
///
/// Tuples up to arity 12 implement `Bundle`.  This lets you spawn with
/// inline component lists:
///
/// ```ignore
/// world.spawn((Position { x: 0., y: 0. }, Velocity { dx: 1., dy: 0. }));
/// ```
///
/// # Duplicate components
///
/// When a bundle (or a tuple) contains the same component type more than
/// once, the **last** occurrence wins.  Earlier writes are overwritten.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a bundle",
    label = "invalid bundle",
    note = "Consider annotating `{Self}` with `#[derive(Bundle)]`."
)]
pub unsafe trait Bundle: Sized + Sync + Send + 'static {
    /// Whether this bundle requires [`apply_effect`] after writing.
    ///
    /// Set this to `true` when the bundle needs to perform post-spawn
    /// work that requires access to the newly-created entity handle.
    ///
    /// For pure-data bundles (the common case), leave this `false`.
    ///
    /// [`apply_effect`]: Bundle::apply_effect
    const NEED_APPLY_EFFECT: bool;

    /// Registers and collects all component types required by this bundle.
    ///
    /// The collector is responsible for ensuring every component type is
    /// known to the world's component registry.  This is called before
    /// archetype resolution.
    fn collect(collector: &mut ComponentCollector);

    /// Writes all component data from this bundle into storage.
    ///
    /// # Safety
    ///
    /// - `data` must be a valid, properly-aligned `OwningPtr` to `Self`.
    /// - `writer` must target a valid row in the correct table.
    /// - The caller must have already called [`collect`] and resolved the
    ///   target archetype.
    ///
    /// [`collect`]: Bundle::collect
    unsafe fn write(data: OwningPtr<'_>, writer: &mut ComponentWriter);

    /// Performs post-spawn side effects after all components have been
    /// written.
    ///
    /// Only called when [`NEED_APPLY_EFFECT`] is `true`.  This receives
    /// the original bundle data (consumed) and a mutable handle to the
    /// newly-spawned entity.
    ///
    /// # Safety
    ///
    /// - `data` must be a valid, properly-aligned `OwningPtr` to `Self`.
    /// - The entity must have been spawned immediately before this call.
    unsafe fn apply_effect(data: OwningPtr<'_>, entity: &mut EntityOwned<'_>);
}

// -----------------------------------------------------------------------------
// DataBundle
// -----------------------------------------------------------------------------

/// Marker supertrait for [`Bundle`] types that contain only pure data and
/// never produce post-spawn side effects.
///
/// All [`Component`] types and the empty tuple `()` implement this trait
/// automatically.  A tuple or `#[derive(Bundle)]` struct also implements
/// `DataBundle` when **every** one of its fields implements `DataBundle`.
///
/// # Contract
///
/// Implementing this trait guarantees that [`Bundle::NEED_APPLY_EFFECT`]
/// is `false` and [`Bundle::apply_effect`] is a no-op.
///
/// # Safety
/// `Self::NEED_APPLY_EFFECT == false`
pub unsafe trait DataBundle: Bundle {}

// -----------------------------------------------------------------------------
// Blanket impl: every Component is a Bundle
// -----------------------------------------------------------------------------

/// Every individual [`Component`] is automatically a [`Bundle`] (and a
/// [`DataBundle`]).  This lets you pass a single component directly to
/// spawn functions without wrapping it in a tuple or struct.
unsafe impl<T: Component> Bundle for T {
    const NEED_APPLY_EFFECT: bool = false;

    fn collect(collector: &mut ComponentCollector) {
        collector.collect::<T>();
    }

    unsafe fn write(data: OwningPtr<'_>, writer: &mut ComponentWriter) {
        unsafe { writer.write::<T>(data) };
    }

    unsafe fn apply_effect(_: OwningPtr<'_>, _: &mut EntityOwned<'_>) {}
}

unsafe impl<T: Component> DataBundle for T {}

// -----------------------------------------------------------------------------
// Tuple bundle impls (0..=12)
// -----------------------------------------------------------------------------

/// Generates [`Bundle`] and [`DataBundle`] implementations for tuples.
///
/// Each tuple element's [`collect`], [`write`], and [`apply_effect`] are
/// forwarded in declaration order.  [`NEED_APPLY_EFFECT`] is the logical
/// OR of all elements' flags.
///
/// [`collect`]: Bundle::collect
/// [`write`]: Bundle::write
/// [`apply_effect`]: Bundle::apply_effect
macro_rules! impl_bundle_for_tuple {
    (0: []) => {
        unsafe impl DataBundle for () {}

        unsafe impl Bundle for () {
            const NEED_APPLY_EFFECT: bool = false;
            fn collect(_collector: &mut ComponentCollector) {}
            unsafe fn write(_: OwningPtr<'_>, _: &mut ComponentWriter) {}
            unsafe fn apply_effect(_: OwningPtr<'_>, _: &mut EntityOwned<'_>) {}
        }
    };
    (1 : [ $index:tt : $name:ident ]) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(
            docsrs,
            doc = "This trait is implemented for tuples up to 12 items long.\n"
        )]
        unsafe impl<$name: DataBundle> DataBundle for ($name,) {}

        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(
            docsrs,
            doc = "This trait is implemented for tuples up to 12 items long.\n"
        )]
        #[cfg_attr(
            docsrs,
            doc = "For larger data, consider using #[derive(Bundle)] to create custom types."
        )]
        unsafe impl<$name: Bundle> Bundle for ($name,) {
            const NEED_APPLY_EFFECT: bool =
                <$name as Bundle>::NEED_APPLY_EFFECT;

            fn collect(collector: &mut ComponentCollector) {
                <$name>::collect(collector);
            }

            unsafe fn write(
                data: OwningPtr<'_>,
                writer: &mut ComponentWriter,
            ) {
                let offset = ::core::mem::offset_of!(Self, 0);
                unsafe { <$name>::write(data.byte_add(offset), writer) };
            }

            unsafe fn apply_effect(
                data: OwningPtr<'_>,
                entity: &mut EntityOwned<'_>,
            ) {
                let offset = ::core::mem::offset_of!(Self, 0);
                unsafe {
                    <$name>::apply_effect(data.byte_add(offset), entity)
                };
            }
        }
    };
    ($num:literal : [$($index:tt : $name:ident),*]) => {
        #[cfg_attr(docsrs, doc(hidden))]
        unsafe impl<$($name: DataBundle),*> DataBundle for ($($name,)*) {}

        #[cfg_attr(docsrs, doc(hidden))]
        unsafe impl<$($name: Bundle),*> Bundle for ($($name,)*) {
            const NEED_APPLY_EFFECT: bool = false
                $( || <$name as Bundle>::NEED_APPLY_EFFECT )*;

            fn collect(collector: &mut ComponentCollector) {
                $( <$name>::collect(collector); )*
            }

            unsafe fn write(
                mut data: OwningPtr<'_>,
                writer: &mut ComponentWriter,
            ) {
                $(unsafe {
                    let offset = ::core::mem::offset_of!(Self, $index);
                    <$name>::write(data.take_field(offset), writer);
                })*
            }

            unsafe fn apply_effect(
                mut data: OwningPtr<'_>,
                entity: &mut EntityOwned<'_>,
            ) {
                $(unsafe {
                    let offset = ::core::mem::offset_of!(Self, $index);
                    <$name>::apply_effect(data.take_field(offset), entity);
                })*
            }
        }
    };
}

zlim_utils::range_invoke!(impl_bundle_for_tuple, 12);

// -----------------------------------------------------------------------------
