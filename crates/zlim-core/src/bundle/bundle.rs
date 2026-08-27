//! The [`Bundle`] and [`DataBundle`] traits.

#![expect(clippy::module_inception, reason = "For better structure.")]

use core::any::TypeId;

use zlim_ptr::OwningPtr;

use crate::component::{Component, ComponentCollector, ComponentWriter};
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
/// `world.spawn(bundle, parent)`, the ECS:
///
/// 1. Calls [`collect_required`] to register the bundle's component set —
///    the bundle's own components **plus** every component *required* by
///    them (through their [`Component::REQUIRED`] constant), recursively.
/// 2. Resolves the target table from the collected (sorted, deduplicated)
///    component set.
/// 3. Allocates a row in that table and calls [`write_explicit`] to copy
///    the bundle's component data into storage.
/// 4. Calls [`write_required`] to initialise required components that were
///    not provided explicitly with their `Default` values.
/// 5. Calls [`apply_effect`] for any post-spawn side effects (only when
///    [`NEED_APPLY_EFFECT`] is `true`).
///
/// [`collect_explicit`] collects only the bundle's own components and is
/// not invoked by the current spawn pipeline; [`collect_required`]
/// subsumes it.
///
/// [`collect_explicit`]: Bundle::collect_explicit
/// [`collect_required`]: Bundle::collect_required
/// [`write_explicit`]: Bundle::write_explicit
/// [`write_required`]: Bundle::write_required
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
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
/// struct Position { x: f32, y: f32 }
///
/// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
/// struct Velocity { dx: f32, dy: f32 }
///
/// #[derive(Bundle)]
/// #[bundle(data)] // derive DataBundle
/// struct MovableBundle {
///     position: Position,
///     velocity: Velocity,
/// }
///
/// let mut world = World::alloc();
///
/// let bundle = MovableBundle {
///     position: Position { x: 0.0, y: 0.0 },
///     velocity: Velocity { dx: 1.0, dy: 0.0 },
/// };
/// let entity = world.spawn(bundle, None);
///
/// assert_eq!(entity.get::<Position>(), Some(&Position { x: 0.0, y: 0.0 }));
/// assert_eq!(entity.get::<Velocity>(), Some(&Velocity { dx: 1.0, dy: 0.0 }));
/// ```
///
/// # Tuple implementations
///
/// Tuples up to arity 12 implement `Bundle`.  This lets you spawn with
/// inline component lists:
///
/// ```rust, no_run
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
/// struct Position { x: f32, y: f32 }
///
/// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
/// struct Velocity { dx: f32, dy: f32 }
///
/// let mut world = World::alloc();
///
/// let bundle = (Position { x: 0.0, y: 0.0 }, Velocity { dx: 1.0, dy: 0.0 });
///
/// let entity = world.spawn(bundle, None); // None: parent is none
///
/// assert_eq!(entity.get::<Position>(), Some(&Position { x: 0.0, y: 0.0 }));
/// assert_eq!(entity.get::<Velocity>(), Some(&Velocity { dx: 1.0, dy: 0.0 }));
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
    /// `#[derive(Bundle)]` computes this as the logical OR of all field
    /// types' flags, while adding `#[bundle(data)]` requires every field
    /// to be a [`DataBundle`], so the flag is always `false`.
    ///
    /// [`apply_effect`]: Bundle::apply_effect
    const NEED_APPLY_EFFECT: bool;

    /// Registers and collects the bundle's own component types, **without**
    /// following required components.
    ///
    /// The collector is responsible for ensuring every component type is
    /// known to the world's component registry.  Unlike [`collect_required`],
    /// this method does not recurse into required components; the current
    /// spawn pipeline collects through [`collect_required`].
    ///
    /// [`collect_required`]: Bundle::collect_required
    fn collect_explicit(collector: &mut ComponentCollector);

    /// Registers and collects all component types this bundle needs —
    /// the bundle's own components **plus** every required component,
    /// recursively.
    ///
    /// The collected set determines the target table, so required
    /// components are present in storage even when they are not written
    /// explicitly.
    fn collect_required(collector: &mut ComponentCollector);

    /// Writes all explicit component data from this bundle into storage.
    ///
    /// # Safety
    ///
    /// - `data` must be a valid, properly-aligned `OwningPtr` to `Self`.
    /// - `writer` must target a valid row in the correct table.
    /// - The caller must have already called [`collect_required`] and
    ///   resolved the target table.
    ///
    /// [`collect_required`]: Bundle::collect_required
    unsafe fn write_explicit(data: OwningPtr<'_>, writer: &mut ComponentWriter);

    /// Writes required components that were **not** provided explicitly,
    /// initialising them with their `Default` values.
    ///
    /// This runs after [`write_explicit`], so components already written
    /// (or marked via [`assume_init`](ComponentWriter::assume_init)) are
    /// skipped.
    ///
    /// # Safety
    ///
    /// - The writer must target a valid row in the table produced by
    ///   [`collect_required`].
    ///
    /// [`write_explicit`]: Bundle::write_explicit
    /// [`collect_required`]: Bundle::collect_required
    unsafe fn write_required(writer: &mut ComponentWriter);

    /// Performs post-spawn side effects after all components have been
    /// written.
    ///
    /// Only called when [`Bundle::NEED_APPLY_EFFECT`] is `true`.  This receives
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
/// automatically.  Tuples implement `DataBundle` when **every** element
/// implements it, and a `#[derive(Bundle)]` struct implements it when
/// declared with `#[bundle(data)]` (which also requires every field to be
/// a `DataBundle`).
///
/// # Contract
///
/// Implementing this trait guarantees that [`Bundle::NEED_APPLY_EFFECT`]
/// is `false` and [`Bundle::apply_effect`] is a no-op.
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Position { x: f32, y: f32 }
///
/// #[derive(TypePath, Component, Clone)]
/// struct Velocity { dx: f32, dy: f32 }
///
/// // `#[bundle(data)]` marks the struct as a pure-data bundle.
/// #[derive(Bundle)]
/// #[bundle(data)]
/// struct MovableBundle {
///     position: Position,
///     velocity: Velocity,
/// }
///
/// fn assert_data_bundle<B: DataBundle>() {}
///
/// assert_data_bundle::<MovableBundle>();
///
/// // `data` bundles never run a post-spawn side effect.
/// assert!(!MovableBundle::NEED_APPLY_EFFECT);
/// ```
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

    #[inline]
    fn collect_explicit(collector: &mut ComponentCollector) {
        collector.collect_explicit::<T>();
    }

    #[inline]
    fn collect_required(collector: &mut ComponentCollector) {
        collector.collect_required::<T>();
    }

    #[inline]
    unsafe fn write_explicit(data: OwningPtr<'_>, writer: &mut ComponentWriter) {
        // SAFETY: `data` is a valid, aligned instance of `T` and `T` is
        // present in the writer's target row (it was collected beforehand).
        unsafe { writer.write_raw(TypeId::of::<T>(), data) };
    }

    #[inline]
    unsafe fn write_required(writer: &mut ComponentWriter) {
        if let Some(required) = T::REQUIRED {
            unsafe { required.write(writer) };
        }
    }

    #[inline(always)]
    unsafe fn apply_effect(_: OwningPtr<'_>, _: &mut EntityOwned<'_>) {}
}

unsafe impl<T: Component> DataBundle for T {}

// -----------------------------------------------------------------------------
// Tuple bundle impls (0..=12)
// -----------------------------------------------------------------------------

/// Generates [`Bundle`] and [`DataBundle`] implementations for tuples.
///
/// Each tuple element's [`collect_explicit`], [`collect_required`],
/// [`write_explicit`], [`write_required`], and [`apply_effect`] calls are
/// forwarded in declaration order.  [`NEED_APPLY_EFFECT`] is the logical
/// OR of all elements' flags.
///
/// [`collect_explicit`]: Bundle::collect_explicit
/// [`collect_required`]: Bundle::collect_required
/// [`write_explicit`]: Bundle::write_explicit
/// [`write_required`]: Bundle::write_required
/// [`apply_effect`]: Bundle::apply_effect
/// [`NEED_APPLY_EFFECT`]: Bundle::NEED_APPLY_EFFECT
macro_rules! impl_bundle_for_tuple {
    (0: []) => {
        unsafe impl DataBundle for () {}

        unsafe impl Bundle for () {
            const NEED_APPLY_EFFECT: bool = false;
            fn collect_explicit(_collector: &mut ComponentCollector) {}
            fn collect_required(_collector: &mut ComponentCollector) {}
            unsafe fn write_explicit(_: OwningPtr<'_>, _: &mut ComponentWriter) {}
            unsafe fn write_required(_writer: &mut ComponentWriter) {}
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

            fn collect_explicit(collector: &mut ComponentCollector) {
                <$name>::collect_explicit(collector);
            }

            fn collect_required(collector: &mut ComponentCollector) {
                <$name>::collect_required(collector);
            }

            unsafe fn write_explicit(
                data: OwningPtr<'_>,
                writer: &mut ComponentWriter,
            ) {
                let offset = ::core::mem::offset_of!(Self, 0);
                unsafe { <$name>::write_explicit(data.byte_add(offset), writer) };
            }

            unsafe fn write_required(writer: &mut ComponentWriter) {
                unsafe { <$name>::write_required(writer) };
            }

            unsafe fn apply_effect(
                data: OwningPtr<'_>,
                entity: &mut EntityOwned<'_>,
            ) {
                if <Self as Bundle>::NEED_APPLY_EFFECT {
                    let offset = ::core::mem::offset_of!(Self, 0);
                    unsafe {
                        <$name>::apply_effect(data.byte_add(offset), entity)
                    };
                }
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

            fn collect_explicit(collector: &mut ComponentCollector) {
                $( <$name>::collect_explicit(collector); )*
            }

            fn collect_required(collector: &mut ComponentCollector) {
                $( <$name>::collect_required(collector); )*
            }

            unsafe fn write_explicit(
                mut data: OwningPtr<'_>,
                writer: &mut ComponentWriter,
            ) {
                $(unsafe {
                    let offset = ::core::mem::offset_of!(Self, $index);
                    <$name>::write_explicit(data.take_field(offset), writer);
                })*
            }

            unsafe fn write_required(writer: &mut ComponentWriter) {
                $(unsafe { <$name>::write_required(writer); })*
            }

            unsafe fn apply_effect(
                mut data: OwningPtr<'_>,
                entity: &mut EntityOwned<'_>,
            ) {
                if <Self as Bundle>::NEED_APPLY_EFFECT {
                    $(unsafe {
                        let offset = ::core::mem::offset_of!(Self, $index);
                        <$name>::apply_effect(data.take_field(offset), entity);
                    })*
                }
            }
        }
    };
}

zlim_utils::range_invoke!(impl_bundle_for_tuple, 12);

// -----------------------------------------------------------------------------
