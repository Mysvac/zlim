//! The [`Bundle`] trait.

#![expect(clippy::module_inception, reason = "For better structure.")]

use core::any::TypeId;

use zlim_ptr::OwningPtr;

use crate::bundle::BundleWriter;
use crate::component::{Component, ComponentCollector, ComponentWriter, Components};

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
///
/// A bundle carries data only: once its components are written, nothing else
/// runs for it — in particular the newly spawned entity is never handed back
/// to the bundle.
///
/// [`collect_explicit`] collects only the bundle's own components and is
/// not invoked by the current spawn pipeline; [`collect_required`]
/// subsumes it.
///
/// [`collect_explicit`]: Bundle::collect_explicit
/// [`collect_required`]: Bundle::collect_required
/// [`write_explicit`]: Bundle::write_explicit
/// [`write_required`]: Bundle::write_required
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
/// use zlim_reflect::TypePath;
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
/// struct Position { x: f32, y: f32 }
///
/// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
/// struct Velocity { dx: f32, dy: f32 }
///
/// #[derive(Bundle)]
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
/// use zlim_reflect::TypePath;
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

    /// Pushes every component this bundle carries onto `writer`, in
    /// declaration order, reading them out of `data`.
    ///
    /// A caller that holds a bundle *by value* pushes it with
    /// [`BundleWriter::push`], which turns the value into a pointer and calls
    /// this.  Walking the pointer instead of the fields is what keeps the
    /// components off the stack: `offset_of!` locates each one inside the value
    /// that is already there, and each component is then copied once, straight
    /// into the scratch space.
    ///
    /// # Safety
    ///
    /// - `data` must point to a valid, initialised, properly-aligned `Self`.
    /// - `data` must stay valid for the duration of the call, and the
    ///   components pushed from it are copied, never moved out.
    ///
    /// [`BundleWriter::push`]: crate::bundle::BundleWriter::push
    unsafe fn push_to(data: OwningPtr<'_>, writer: &mut BundleWriter, infos: Option<&Components>);

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
}

// -----------------------------------------------------------------------------
// Blanket impl: every Component is a Bundle
// -----------------------------------------------------------------------------

/// Every individual [`Component`] is automatically a [`Bundle`].  This lets
/// you pass a single component directly to spawn functions without wrapping it
/// in a tuple or struct.
unsafe impl<T: Component> Bundle for T {
    #[inline]
    fn collect_explicit(collector: &mut ComponentCollector) {
        collector.collect_explicit::<T>();
    }

    #[inline]
    fn collect_required(collector: &mut ComponentCollector) {
        collector.collect_required::<T>();
    }

    #[inline]
    unsafe fn push_to(data: OwningPtr<'_>, writer: &mut BundleWriter, infos: Option<&Components>) {
        writer.push_owning::<T>(data, infos);
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
}

// -----------------------------------------------------------------------------
// Tuple bundle impls (0..=12)
// -----------------------------------------------------------------------------

/// Generates [`Bundle`] implementations for tuples.
///
/// Each tuple element's [`collect_explicit`], [`collect_required`], [`push_to`],
/// [`write_explicit`], and [`write_required`] calls are forwarded in
/// declaration order.
///
/// [`collect_explicit`]: Bundle::collect_explicit
/// [`collect_required`]: Bundle::collect_required
/// [`push_to`]: Bundle::push_to
/// [`write_explicit`]: Bundle::write_explicit
/// [`write_required`]: Bundle::write_required
macro_rules! impl_bundle_for_tuple {
    (0: []) => {
        unsafe impl Bundle for () {
            fn collect_explicit(_collector: &mut ComponentCollector) {}
            fn collect_required(_collector: &mut ComponentCollector) {}
            unsafe fn push_to(_: OwningPtr<'_>, _: &mut BundleWriter, _: Option<&Components>) {}
            unsafe fn write_explicit(_: OwningPtr<'_>, _: &mut ComponentWriter) {}
            unsafe fn write_required(_writer: &mut ComponentWriter) {}
        }
    };
    (1 : [ $index:tt : $name:ident ]) => {
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
            #[inline]
            fn collect_explicit(collector: &mut ComponentCollector) {
                <$name as Bundle>::collect_explicit(collector);
            }

            #[inline]
            fn collect_required(collector: &mut ComponentCollector) {
                <$name as Bundle>::collect_required(collector);
            }

            #[inline]
            unsafe fn push_to(data: OwningPtr<'_>, writer: &mut BundleWriter, infos: Option<&Components>) {
                let offset = ::core::mem::offset_of!(Self, 0);
                unsafe { <$name as Bundle>::push_to(data.byte_add(offset), writer, infos) };
            }

            #[inline]
            unsafe fn write_explicit(data: OwningPtr<'_>, writer: &mut ComponentWriter) {
                let offset = ::core::mem::offset_of!(Self, 0);
                unsafe { <$name as Bundle>::write_explicit(data.byte_add(offset), writer) };
            }

            #[inline]
            unsafe fn write_required(writer: &mut ComponentWriter) {
                unsafe { <$name as Bundle>::write_required(writer) };
            }
        }
    };
    ($num:literal : [$($index:tt : $name:ident),*]) => {
        #[cfg_attr(docsrs, doc(hidden))]
        unsafe impl<$($name: Bundle),*> Bundle for ($($name,)*) {
            fn collect_explicit(collector: &mut ComponentCollector) {
                $( <$name as Bundle>::collect_explicit(collector); )*
            }

            fn collect_required(collector: &mut ComponentCollector) {
                $( <$name as Bundle>::collect_required(collector); )*
            }

            unsafe fn push_to(mut data: OwningPtr<'_>, writer: &mut BundleWriter, infos: Option<&Components>) {
                $(unsafe {
                    let offset = ::core::mem::offset_of!(Self, $index);
                    <$name as Bundle>::push_to(data.take_field(offset), writer, infos);
                })*
            }

            unsafe fn write_explicit(mut data: OwningPtr<'_>, writer: &mut ComponentWriter) {
                $(unsafe {
                    let offset = ::core::mem::offset_of!(Self, $index);
                    <$name as Bundle>::write_explicit(data.take_field(offset), writer);
                })*
            }

            unsafe fn write_required(writer: &mut ComponentWriter) {
                $(unsafe { <$name as Bundle>::write_required(writer); })*
            }
        }
    };
}

zlim_utils::range_invoke!(impl_bundle_for_tuple, 12);

// -----------------------------------------------------------------------------
