use core::any::{Any, TypeId};
use core::fmt::{self, Debug, Formatter};
use core::hash::Hash;

use zlim_ptr::{Ptr, PtrMut};

use super::{ApplyError, CloneError};
use super::{ReflectMut, ReflectOwned, ReflectRef};
use crate::info::DynamicTyped;
use crate::info::ReflectKind;
use crate::path::{DynamicTypePath, TypePath};

pub use zlim_reflect_derive::Reflect;

// -----------------------------------------------------------------------------
// Reflect

/// The core reflection trait.
///
/// Every reflected type must implement this trait. Use
/// `#[derive(Reflect)]` to generate an implementation for your types.
///
/// It provides default implementations for hashing, equality comparison,
/// debug formatting, and value conversion ([`from_reflect`]).
///
/// The default hash and equality for [`Opaque`] types are text-based (via
/// [`stringify`]), which avoids issues with IEEE 754 float semantics.
///
/// See the [module-level documentation](crate::ops) for information about
/// the sub-traits ([`Struct`], [`Tuple`], [`Enum`], etc.) that provide
/// shape-specific access methods.
///
/// # Derive macro
///
/// Use `#[derive(Reflect)]` to automatically implement reflection for
/// custom types — structs, tuple structs, and enums. The derive macro
/// generates implementations for both this trait and the appropriate
/// sub-trait ([`Struct`] for named-field structs, [`Tuple`] for tuple
/// structs, [`Enum`] for enums, etc.).
///
/// ```no_run
/// use zlim_reflect::Reflect;
///
/// #[derive(Reflect)]
/// struct Foo { x: i32, y: String }
/// ```
///
/// Standard-trait attributes like `#[reflect(Clone)]`, `#[reflect(Eq)]`,
/// `#[reflect(Hash)]`, and `#[reflect(Debug)]` enable optimised fast
/// paths that delegate directly to the corresponding standard-library
/// trait methods instead of going through field-by-field reflection.
///
/// ```no_run
/// # use zlim_reflect::Reflect;
/// #
/// #[derive(Reflect, Clone, Default, Debug)]
/// #[reflect(Clone, Default, Debug)]
/// struct Foo { x: i32, y: String }
/// ```
///
/// Field-level attributes such as `#[reflect(ignore)]`, `#[reflect(default)]`,
/// and `#[reflect(clone)]` further tune the generated behaviour.
///
/// If a field is marked as `ignore`, it is usually necessary to use `default` in conjunction,
/// otherwise many reflection operations will always fail (because this field cannot be constructed)
///
/// ```no_run
/// # use zlim_reflect::Reflect;
/// # use core::marker::PhantomData;
/// #[derive(Reflect)]
/// struct Foo {
///     x: i32,
///     #[reflect(ignore, default)]
///     _marker: PhantomData<()>,
/// }
/// ```
///
/// See the [derive macro documentation] for a complete reference of all
/// supported attributes.
///
/// [derive macro documentation]: macro@crate::Reflect
///
/// # Reflect Assignment
///
/// Three methods transfer data into a reflected value, each with different
/// strictness and performance characteristics:
///
/// - [`reflect_assign`] — same-type only. Swaps memory directly when
///   `TypeId` matches; fast but fails for any other type.
///
/// - [`from_reflect`] — constructs `Self` from a boxed value. Types may
///   differ but fields must be compatible. Can unpack and move fields
///   directly, avoiding extra copies for complex types.
///
/// - [`reflect_apply`] — copies data into `self` by reference. Types may
///   differ. Falls back to [`stringify`] / [`apply_str`] for [`Opaque`]
///   types, so it is more lenient than [`from_reflect`], but slower
///   because it must clone the source data.
///
/// [`reflect_assign`]: Self::reflect_assign
/// [`from_reflect`]: Self::from_reflect
/// [`reflect_apply`]: Self::reflect_apply
/// [`Opaque`]: crate::ops::Opaque
/// [`stringify`]: crate::ops::Opaque::stringify
/// [`apply_str`]: crate::ops::Opaque::apply_str
///
/// # Examples
///
/// ```
/// use zlim_reflect::Reflect;
/// use zlim_reflect::info::ReflectKind;
///
/// #[derive(Reflect)]
/// struct Point {
///     x: f32,
///     y: f32,
/// }
///
/// let point = Point { x: 1.0, y: 2.0 };
///
/// // Inspect the reflection kind.
/// assert_eq!(point.reflect_kind(), ReflectKind::Struct);
///
/// // Clone through reflection.
/// let cloned = point.reflect_clone().unwrap();
/// let restored: &Point = cloned.downcast_ref().unwrap();
/// assert_eq!(restored.x, 1.0);
///
/// // Hash and equality through reflection.
/// let other = Point { x: 1.0, y: 2.0 };
/// assert!(point.reflect_eq(&other));
/// assert_eq!(point.reflect_hash(), other.reflect_hash());
///
/// // Convert from a boxed reflected value.
/// let boxed: Box<dyn Reflect> = Box::new(Point { x: 3.0, y: 4.0 });
/// let result: Box<Point> = Point::from_reflect(boxed).unwrap();
/// assert_eq!(result.x, 3.0);
/// ```
///
/// [`from_reflect`]: Self::from_reflect
/// [`Opaque`]: crate::ops::Opaque
/// [`stringify`]: crate::ops::Opaque::stringify
/// [`Struct`]: crate::ops::Struct
/// [`Tuple`]: crate::ops::Tuple
/// [`Enum`]: crate::ops::Enum
#[diagnostic::on_unimplemented(
    message = "`{Self}` does not implement `Reflect`",
    label = "invalid `Reflect`",
    note = "consider annotating `{Self}` with `#[derive(Reflect)]`"
)]
pub trait Reflect: DynamicTypePath + DynamicTyped + Send + Sync + Any {
    /// Returns `true` if this value is a dynamic type.
    ///
    /// Dynamic types (e.g. [`DynamicStruct`], [`DynamicMap`]) are temporary
    /// data-conversion intermediaries. They report their [`TypeInfo`] as
    /// [`OpaqueInfo`] regardless of their actual [`ReflectKind`], and they
    /// do not implement [`TypeDatabase`].
    ///
    /// Use this method to distinguish dynamic types from regular reflected
    /// types when operating on `&dyn Reflect` — dynamic types should not be
    /// stored long-term or used as fields of custom reflected types.
    ///
    /// The default implementation returns `false`. Dynamic types override
    /// this to return `true`.
    ///
    /// [`DynamicStruct`]: crate::dynamic::DynamicStruct
    /// [`DynamicMap`]: crate::dynamic::DynamicMap
    /// [`TypeInfo`]: crate::info::TypeInfo
    /// [`OpaqueInfo`]: crate::info::OpaqueInfo
    /// [`ReflectKind`]: crate::info::ReflectKind
    /// [`TypeDatabase`]: crate::db::TypeDatabase
    #[inline(always)]
    fn is_dynamic(&self) -> bool {
        false
    }

    /// Returns the [`ReflectKind`] for this type.
    ///
    /// The kind is the top-level shape discriminant — `Struct`, `Tuple`,
    /// `Array`, `Opaque`, etc. Use this to branch on the structural form
    /// of a reflected value before accessing its data.
    ///
    /// [`ReflectKind`]: crate::info::ReflectKind
    fn reflect_kind(&self) -> ReflectKind;

    /// Returns an immutable [`ReflectRef`] view of this value.
    ///
    /// The returned variant must be consistent with
    /// [`reflect_kind`]; otherwise downstream functions may panic.
    ///
    /// [`reflect_kind`]: Self::reflect_kind
    fn reflect_ref(&self) -> ReflectRef<'_>;

    /// Returns a mutable [`ReflectMut`] view of this value.
    ///
    /// The returned variant must be consistent with
    /// [`reflect_kind`]; otherwise downstream functions may panic.
    ///
    /// [`reflect_kind`]: Self::reflect_kind
    fn reflect_mut(&mut self) -> ReflectMut<'_>;

    /// Consumes `self` and returns a owned [`ReflectOwned`] view.
    ///
    /// The returned variant must be consistent with
    /// [`reflect_kind`]; otherwise downstream functions may panic.
    ///
    /// [`reflect_kind`]: Self::reflect_kind
    fn reflect_owned(self: Box<Self>) -> ReflectOwned;

    /// Clones the reflected value.
    ///
    /// # Behaviour
    ///
    /// If the type was annotated with `#[reflect(Clone)]`, this method
    /// delegates directly to [`Clone::clone`].
    ///
    /// Otherwise the default implementation performs field-by-field
    /// reflection cloning and then reconstructs `Self` from the cloned
    /// fields.
    ///
    /// This path is usually slower than the native `Clone` implementation
    /// but works for any reflected type without requiring the `Clone` trait.
    ///
    /// # Contract
    ///
    /// `reflect_clone` must not change the value's type — the returned
    /// trait object must have the same concrete [`TypeId`] as `self`.
    /// If this contract is violated, some functions may panic internally
    /// (though never with undefined behavior).
    ///
    /// [`Clone::clone`]: Clone::clone
    /// [`TypeId`]: core::any::TypeId
    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError>;

    /// Applies a reflected value to `self`.
    ///
    /// This is the reflection equivalent of the assignment operator:
    /// it replaces `self`'s contents with the data from `value`.
    ///
    /// # Warning
    ///
    /// For non-fixed-capacity containers ([`Set`], [`Map`], [`List`]) the
    /// default implementation includes a recovery mechanism: on failure
    /// the original elements are restored, so the container usually
    /// returns to its pre-apply state.
    ///
    /// For fixed-capacity types ([`Struct`], [`Tuple`], [`Enum`], [`Array`])
    /// no such recovery exists — a failure may leave `self` in a partially-updated
    /// state.
    ///
    /// # Dispatch Order
    ///
    /// 1. **Same type** — if `TypeId` matches, try clone-and-assign directly
    ///    (fast path).
    ///
    /// 2. **Type database** — if a registered conversion function exists from
    ///    `value`'s type to `Self`'s type, try reflect_clone `value` and use
    ///    it.
    ///
    /// 3. **Kind-specific** — cast `value` to the corresponding ops trait
    ///    ([`Struct`], [`List`], [`Opaque`], etc.) and apply **field-by-field**.
    ///
    /// # Field-by-field Apply Rules:
    ///
    /// - [`Set`], [`Map`], [`List`]: All existing elements are drained first,
    ///   then each element from `value` is cloned and inserted.  If all
    ///   elements are successfully copied, the function succeeds.  If any
    ///   clone or insert fails, the drained elements are restored and the
    ///   error is returned.
    ///
    /// - [`Tuple`], [`Array`]: The length is checked first — if mismatched,
    ///   the function fails immediately.  When lengths match, each field is
    ///   applied via `reflect_apply` in order.  If a mid-way field fails,
    ///   earlier fields remain updated (partial application).
    ///
    /// - [`Struct`]: Fields are matched by name (order does not matter).
    ///   Extra fields in `value` are silently ignored.  Each matched field
    ///   is applied via `reflect_apply`.  Mid-way failure may leave some
    ///   fields updated and others unchanged.
    ///
    /// - [`Enum`]: Processing degrades to the variant level.  If the variant
    ///   names match but the [`VariantKind`] differs, the function fails
    ///   immediately.  If both name and kind match, the variant data is
    ///   applied following the [`Tuple`] or [`Struct`] rules above.  If the
    ///   variant names differ, a new variant must be constructed from
    ///   `value` and then assigned to `self`; construction failure also
    ///   returns an error.
    ///
    /// - [`Opaque`]: The source is serialized via [`stringify`] and the
    ///   resulting string is applied via [`apply_str`].
    ///
    /// [`VariantKind`]: crate::info::VariantKind
    /// [`stringify`]: crate::ops::Opaque::stringify
    /// [`apply_str`]: crate::ops::Opaque::apply_str
    /// [`Struct`]: crate::ops::Struct
    /// [`Tuple`]: crate::ops::Tuple
    /// [`Enum`]: crate::ops::Enum
    /// [`Array`]: crate::ops::Array
    /// [`List`]: crate::ops::List
    /// [`Set`]: crate::ops::Set
    /// [`Map`]: crate::ops::Map
    /// [`Opaque`]: crate::ops::Opaque
    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError>;

    /// Computes a deterministic hash for the reflected value.
    ///
    /// This hash is designed for use with the fixed-seed hasher
    /// [`reflect_hasher`], not for cryptographic purposes.
    ///
    /// # Behaviour
    ///
    /// If the type was annotated with `#[reflect(Hash)]`, this method
    /// delegates directly to [`Hash::hash`].
    ///
    /// Otherwise the default implementation performs field-by-field
    /// and [`TypeId`] reflection hashing.
    ///
    /// For [`Opaque`] types the default implementation is computed from
    /// the text representation ([`stringify`]), which gives correct
    /// results for types like `f32` / `f64` where IEEE 754 equality
    /// (`NaN != NaN`) would otherwise break hash-table lookups.
    ///
    /// [`TypeId`]: core::any::TypeId
    /// [`Opaque`]: crate::ops::Opaque
    /// [`stringify`]: crate::ops::Opaque::stringify
    /// [`reflect_hasher`]: crate::impls::reflect_hasher
    #[inline]
    fn reflect_hash(&self) -> u64 {
        // Separate to ensure only one compilation and avoid code bloating.
        #[inline(never)]
        fn default_hash(this: ReflectRef<'_>) -> u64 {
            use crate::impls;
            match this {
                ReflectRef::Opaque(r) => impls::opaque_hash(r),
                ReflectRef::Struct(r) => impls::struct_hash(r),
                ReflectRef::Tuple(r) => impls::tuple_hash(r),
                ReflectRef::Array(r) => impls::array_hash(r),
                ReflectRef::List(r) => impls::list_hash(r),
                ReflectRef::Map(r) => impls::map_hash(r),
                ReflectRef::Set(r) => impls::set_hash(r),
                ReflectRef::Enum(r) => impls::enum_hash(r),
            }
        }

        default_hash(self.reflect_ref())
    }

    /// Compares two reflected values for equality.
    ///
    /// This is a **strict** comparison: different concrete types
    /// (determined by [`TypeId`]) always compare as unequal, regardless
    /// of their contents. For structs the field declaration order must
    /// also match.
    ///
    /// # Behaviour
    ///
    /// If the type was annotated with `#[reflect(Eq)]`, this method
    /// delegates directly to [`PartialEq::eq`] (if downcast successed).
    ///
    /// Otherwise, use default implementation instread:
    ///
    /// 1. Compares [`TypeId`] — different IDs → not equal.
    /// 2. Delegates to the kind-specific `reflect_eq` on inner fields.
    ///
    /// For [`Opaque`] types the default comparison is text-based (via
    /// [`stringify`]), which avoids IEEE 754 corner cases.
    ///
    /// # Note
    ///
    /// The default implementation compares fields in declaration order.
    ///
    /// Hash-based containers (sets, maps) should override this to provide
    /// order-independent equality.
    ///
    /// Or a more extreme implementation, which only compares whether it
    /// belongs to the same object through pointers, is also feasible.
    ///
    /// [`TypeId`]: core::any::TypeId
    /// [`ReflectKind`]: crate::info::ReflectKind
    /// [`Opaque`]: crate::ops::Opaque
    /// [`stringify`]: crate::ops::Opaque::stringify
    #[inline]
    fn reflect_eq(&self, other: &dyn Reflect) -> bool {
        // Separate to ensure only one compilation and avoid code bloating.
        #[inline(never)]
        fn default_eq(this: ReflectRef<'_>, o: &dyn Reflect) -> bool {
            use crate::impls;
            match this {
                ReflectRef::Opaque(r) => impls::opaque_eq(r, o),
                ReflectRef::Struct(r) => impls::struct_eq(r, o),
                ReflectRef::Tuple(r) => impls::tuple_eq(r, o),
                ReflectRef::Array(r) => impls::array_eq(r, o),
                ReflectRef::List(r) => impls::list_eq(r, o),
                ReflectRef::Map(r) => impls::map_eq(r, o),
                ReflectRef::Set(r) => impls::set_eq(r, o),
                ReflectRef::Enum(r) => impls::enum_eq(r, o),
            }
        }

        default_eq(self.reflect_ref(), other)
    }

    /// Formats the reflected value for debug output.
    ///
    /// # Behaviour
    ///
    /// If the type was annotated with `#[reflect(Debug)]`, this method
    /// delegates directly to [`Debug::fmt`].
    ///
    /// Otherwise, use default implementation instread.
    ///
    /// The default implementation delegates to kind-specific formatting,
    /// for example:
    ///
    /// - `Struct<Type>({ field1: val1, field2: val2 })`
    /// - `Opaque<Type>(value)`
    ///
    /// [`Debug`]: core::fmt::Debug
    #[inline]
    fn reflect_debug(&self, f: &mut Formatter) -> fmt::Result {
        // Separate to ensure only one compilation and avoid code bloating.
        #[inline(never)]
        fn default_debug(this: ReflectRef<'_>, f: &mut Formatter) -> fmt::Result {
            use crate::impls;
            match this {
                ReflectRef::Opaque(r) => impls::opaque_debug(r, f),
                ReflectRef::Struct(r) => impls::struct_debug(r, f),
                ReflectRef::Tuple(r) => impls::tuple_debug(r, f),
                ReflectRef::Array(r) => impls::array_debug(r, f),
                ReflectRef::List(r) => impls::list_debug(r, f),
                ReflectRef::Map(r) => impls::map_debug(r, f),
                ReflectRef::Set(r) => impls::set_debug(r, f),
                ReflectRef::Enum(r) => impls::enum_debug(r, f),
            }
        }

        default_debug(self.reflect_ref(), f)
    }

    /// Returns a type-erased immutable pointer to this value.
    #[inline]
    fn reflect_ptr(&self) -> Ptr<'_> {
        Ptr::from_ref(self)
    }

    /// Returns a type-erased mutable pointer to this value.
    #[inline]
    fn reflect_ptr_mut(&mut self) -> PtrMut<'_> {
        PtrMut::from_mut(self)
    }

    /// Assigns a boxed reflected value to `self`.
    ///
    /// Semantically this is equivalent to:
    ///
    /// ```ignore
    /// match value.downcast::<Self>() {
    ///     Ok(v) => { *self = *v; Ok(()) },
    ///     Err(e) => Err(e),
    /// }
    /// ```
    ///
    /// When the concrete types match (determined by [`TypeId`]), the
    /// value is assigned; otherwise `Err(value)` is returned and `self`
    /// is unchanged.
    ///
    /// The default implementation uses a workaround because `Self` is
    /// `?Sized` in the trait definition — `*self` and `downcast::<Self>`
    /// are unavailable.  It determines the value's size at runtime
    /// through the vtable and performs a direct memory swap.
    ///
    /// The implementation generated by `#[derive(Reflect)]` uses the
    /// simpler and more efficient `*self = *value.downcast::<Self>()?`
    /// form directly.
    ///
    /// [`TypeId`]: core::any::TypeId
    fn reflect_assign(&mut self, mut value: Box<dyn Reflect>) -> Result<(), Box<dyn Reflect>> {
        if TypeId::of::<Self>() == (*value).type_id() {
            // We would like to write `*self = *value.downcast::<Self>()`,
            // but in a trait method `Self` may be `!Sized` (though in
            // practice it almost always is). Instead we determine the
            // concrete type's size at runtime through the vtable and
            // swap the memory directly.
            // In the code generated by macros, `downcast` directly.
            #[expect(unsafe_code, reason = "Hack")]
            unsafe {
                let size: usize = ::core::mem::size_of_val::<dyn Reflect>(&*value);
                debug_assert_eq!(::core::mem::size_of_val::<Self>(self), size);
                let src: *mut u8 = self as *mut Self as *mut u8;
                let dst: *mut u8 = (*value).reflect_ptr_mut().as_ptr();
                core::ptr::swap_nonoverlapping::<u8>(src, dst, size);
            }
            Ok(())
        } else {
            Err(value)
        }
    }

    /// Borrows `&self` as `&dyn Reflect`.
    ///
    /// This is an upcast helper: since `Self: Reflect`, this simply returns
    /// `self` as a trait object. Useful in generic contexts where you have
    /// a `T: Reflect` and need a `&dyn Reflect`.
    #[inline(always)]
    fn as_reflect(&self) -> &dyn Reflect
    where
        Self: Sized,
    {
        self
    }

    /// Borrows `&mut self` as `&mut dyn Reflect`.
    ///
    /// The mutable counterpart of [`as_reflect`].
    ///
    /// [`as_reflect`]: Self::as_reflect
    #[inline(always)]
    fn as_mut_reflect(&mut self) -> &mut dyn Reflect
    where
        Self: Sized,
    {
        self
    }

    /// Boxes `self` into a `Box<dyn Reflect>`.
    #[inline(always)]
    fn into_boxed_reflect(self) -> Box<dyn Reflect>
    where
        Self: Sized,
    {
        Box::new(self)
    }

    /// Reconstructs a `&dyn Reflect` from a raw [`Ptr`].
    ///
    /// This is the inverse of [`reflect_ptr`] — it recovers a trait object
    /// reference from a previously-obtained type-erased pointer.
    ///
    /// # Safety
    ///
    /// `p` must point to a valid, initialized value of type `Self` with
    /// the correct alignment (alignment is debug-asserted).
    ///
    /// [`Ptr`]: zlim_ptr::Ptr
    /// [`reflect_ptr`]: Self::reflect_ptr
    #[expect(unsafe_code, reason = "unsafe API")]
    #[inline(always)]
    unsafe fn reflect_from_ptr(p: Ptr<'_>) -> &'_ dyn Reflect
    where
        Self: Sized,
    {
        p.debug_assert_aligned::<Self>();
        unsafe { p.deref::<Self>() }
    }

    /// Reconstructs a `&mut dyn Reflect` from a raw [`PtrMut`].
    ///
    /// This is the inverse of [`reflect_ptr_mut`].
    ///
    /// # Safety
    ///
    /// `p` must point to a valid, initialized, uniquely-referenced value
    /// of type `Self` with the correct alignment (alignment is
    /// debug-asserted).
    ///
    /// [`PtrMut`]: zlim_ptr::PtrMut
    /// [`reflect_ptr_mut`]: Self::reflect_ptr_mut
    #[expect(unsafe_code, reason = "unsafe API")]
    #[inline(always)]
    unsafe fn reflect_from_ptr_mut(p: PtrMut<'_>) -> &'_ mut dyn Reflect
    where
        Self: Sized,
    {
        p.debug_assert_aligned::<Self>();
        unsafe { p.deref::<Self>() }
    }
    /// Constructs `Self` from a boxed reflected value.
    ///
    /// # Rules
    ///
    /// 1. **Same type** — direct downcast and return (fast path).
    ///
    /// 2. **Type database** — if the [`TypeDB`] has a registered conversion
    ///    function from `value`'s type to `Self`, use it.
    ///
    /// 3. **Field compatibility** — check whether the immediate fields of
    ///    `value` are compatible with `Self`. This check is **non-recursive**:
    ///    only the top-level fields are examined; fields of fields are not
    ///    inspected. A type's own convertibility is determined solely by
    ///    (type equality | database lookup) — it will never attempt to
    ///    recursively check nested fields.
    ///
    /// 4. **Construct** — if compatible, unpack `value` (via the relevant
    ///    `unpack` or `drain_all` method) and construct `Self` field by field.
    ///    The following sections are detailed explanations about this step.
    ///
    /// # Strict vs Lenient
    ///
    /// - **Tuples and arrays** are **strict**: the length must match exactly.
    ///   Missing or extra elements cause conversion failure.
    ///
    /// - **Structs and enum struct variants** are **lenient**: extra fields in
    ///   the source are silently ignored. However, every field that is *not*
    ///   annotated with `#[reflect(default)]` must be present — otherwise
    ///   conversion fails.
    ///
    /// # Default fields
    ///
    /// Struct and enum struct-variant fields annotated with
    /// `#[reflect(default)]` use their default value when the source
    /// does not contain a corresponding field.
    ///
    /// # Defaultable Struct
    ///
    /// Structs annotated with `#[reflect(Default)]` are constructed via
    /// `Default::default()` first, then each matching field is applied
    /// individually.  This means converting from a completely unrelated
    /// source (one with zero overlapping fields) silently succeeds and
    /// yields the target's `Default` value — which may be surprising.
    ///
    /// # Opaque Specialization
    ///
    /// [`Opaque`] types may provide specialized `from_reflect` implementations.
    /// Because all opaque values can be serialized to a string via [`stringify`],
    /// they can convert between different concrete types by serializing the source
    /// and deserializing into the target type.
    ///
    /// [`TypeDB`]: crate::db::TypeDB
    /// [`Opaque`]: crate::ops::Opaque
    /// [`stringify`]: crate::ops::Opaque::stringify
    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>>
    where
        Self: Sized;
}

// -----------------------------------------------------------------------------
// Methods

impl dyn Reflect {
    /// Returns `true` if the underlying value is of type `T`.
    ///
    /// This is shorthand for `self.type_id() == TypeId::of::<T>()`.
    #[inline(always)]
    pub fn is<T: Any>(&self) -> bool {
        self.type_id() == TypeId::of::<T>()
    }

    /// Downcasts from `&dyn Reflect` to `&T`.
    ///
    /// Returns `None` if the underlying type is not `T`.
    #[inline(always)]
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        <dyn Any>::downcast_ref(self)
    }

    /// Downcasts from `&mut dyn Reflect` to `&mut T`.
    ///
    /// Returns `None` if the underlying type is not `T`.
    #[inline(always)]
    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        <dyn Any>::downcast_mut(self)
    }

    /// Downcasts a `Box<dyn Reflect>` to `Box<T>`.
    ///
    /// Returns `Err(self)` if the underlying type is not `T`.
    ///
    /// # Safety
    ///
    /// The type check (`self.is::<T>()`) guarantees that the subsequent
    /// unchecked downcast is safe.
    #[inline]
    pub fn downcast<T: Any>(self: Box<dyn Reflect>) -> Result<Box<T>, Box<dyn Reflect>> {
        if self.is::<T>() {
            #[expect(unsafe_code, reason = "type is already checked")]
            Ok(unsafe { <Box<dyn Any>>::downcast::<T>(self).unwrap_unchecked() })
        } else {
            Err(self)
        }
    }

    /// Downcasts and unboxes a `Box<dyn Reflect>` to `T`.
    ///
    /// This is equivalent to `Self::downcast(self).map(|b| *b)` but avoids
    /// an extra heap allocation.
    ///
    /// Returns `Err(self)` if the underlying type is not `T`.
    ///
    /// # Safety
    ///
    /// The type check (`self.is::<T>()`) guarantees that the subsequent
    /// unchecked downcast is safe.
    #[inline]
    pub fn take<T: Any>(self: Box<dyn Reflect>) -> Result<T, Box<dyn Reflect>> {
        if self.is::<T>() {
            #[expect(unsafe_code, reason = "type is already checked")]
            Ok(unsafe { *<Box<dyn Any>>::downcast::<T>(self).unwrap_unchecked() })
        } else {
            Err(self)
        }
    }
}

// -----------------------------------------------------------------------------
// Traits

impl TypePath for dyn Reflect {
    #[inline]
    fn type_path() -> &'static str {
        "dyn zlim_reflect::Reflect"
    }

    #[inline]
    fn type_name() -> &'static str {
        "dyn Reflect"
    }

    const IDENT: &str = "dyn Reflect";
    const MODULE: Option<&str> = Some("zlim_reflect");
    const CRATE: Option<&str> = Some("zlim_reflect");
}

impl Debug for dyn Reflect {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.reflect_debug(f)
    }
}

impl Hash for dyn Reflect {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.reflect_hash());
    }
}

impl PartialEq for dyn Reflect {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.reflect_eq(other)
    }
}

impl Eq for dyn Reflect {}

// -----------------------------------------------------------------------------
