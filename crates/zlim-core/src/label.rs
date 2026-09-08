//! Interning primitives used by ECS label systems.
//!
//! This module provides a generic interner and type-erased equality/hash tools
//! used by label traits such as `ScheduleLabel`.
//!
//! Most users will interact through derive macros and interned label handles,
//! while contributors may implement low-level traits here.

use core::any::Any;
use core::hash::Hash;
use core::ops::Deref;
use core::{fmt::Debug, hash::Hasher};
use std::sync::{PoisonError, RwLock};

use zlim_utils::ext::CachePadded;
use zlim_utils::hash::HashSet;
use zlim_utils::mem::Global;

// -----------------------------------------------------------------------------
// Dyn Hash/Eq

/// Type-erased equality for label trait objects.
pub trait DynEq: Any {
    /// Compares two dynamic values for equality.
    fn dyn_eq(&self, other: &dyn DynEq) -> bool;
}

/// Type-erased hashing for label trait objects.
pub trait DynHash: Any {
    /// Hashes this dynamic value into the provided hasher.
    fn dyn_hash(&self, state: &mut dyn Hasher);
}

impl<T: Any + Eq> DynEq for T {
    fn dyn_eq(&self, other: &dyn DynEq) -> bool {
        if let Some(other) = <dyn Any>::downcast_ref::<T>(other) {
            self == other
        } else {
            false
        }
    }
}

impl<T: Any + Hash> DynHash for T {
    fn dyn_hash(&self, mut state: &mut dyn Hasher) {
        T::hash(self, &mut state);
        self.type_id().hash(&mut state);
    }
}

// -----------------------------------------------------------------------------
// Internable

/// A value that can be interned into a stable `'static` reference.
pub trait Internable: Hash + Eq + 'static {
    /// Returns `true` if the two references point to the same value.
    fn ref_eq(&self, other: &Self) -> bool;

    /// Feeds the reference to the hasher.
    fn ref_hash(&self, state: &mut dyn Hasher);
}

// -----------------------------------------------------------------------------
// Interned

/// A lightweight handle to an interned value.
pub struct Interned<T: ?Sized + Internable>(pub &'static T);

impl<T: ?Sized + Internable> Copy for Interned<T> {}

impl<T: ?Sized + Internable> Clone for Interned<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized + Internable> Deref for Interned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<T: ?Sized + Internable> PartialEq for Interned<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.ref_eq(other.0)
    }
}

impl<T: ?Sized + Internable> Eq for Interned<T> {}

impl<T: ?Sized + Internable> Hash for Interned<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.ref_hash(state);
    }
}

impl<T: ?Sized + Internable + Debug> Debug for Interned<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: ?Sized + Internable> From<&Interned<T>> for Interned<T> {
    fn from(value: &Interned<T>) -> Self {
        *value
    }
}

// -----------------------------------------------------------------------------
// Interner

/// Thread-safe interner for values implementing [`Internable`].
///
/// In the Label system, this is used to canonicalize dynamic labels into
/// unique `'static` references, enabling fast comparisons, stable hashing,
/// and cheap copies via [`Interned<T>`].
///
/// # Memory Behavior
///
/// New unique values may be leaked to produce stable `'static` references.
/// This is intentional for label-like domains with a small bounded set.
pub struct Interner<T: ?Sized + 'static>(CachePadded<RwLock<HashSet<&'static T>>>);

impl<T: ?Sized> Interner<T> {
    /// Creates a new empty interner
    #[expect(clippy::new_without_default, reason = "need const fn")]
    pub const fn new() -> Self {
        Self(CachePadded::new(RwLock::new(HashSet::new())))
    }
}

impl<T: ?Sized + Internable> Interner<T> {
    /// Attempts to retrieve the interned version of a value.
    #[inline(never)]
    pub fn get(&self, value: &T) -> Option<Interned<T>> {
        let set = self.0.read().unwrap_or_else(PoisonError::into_inner);
        Some(Interned(*set.get(value)?))
    }

    /// Interns a value, ensuring a single shared instance exists for equal values.
    #[inline(never)] // use dynamic object to reduce compile-overload
    pub fn intern(&self, value: &T, f: &dyn Fn() -> &'static T) -> Interned<T> {
        let mut set = self.0.write().unwrap_or_else(PoisonError::into_inner);
        Interned(*set.get_or_insert_with(value, |_| f()))
    }
}

// -----------------------------------------------------------------------------
// Interner

#[doc(hidden)]
#[inline(always)]
pub fn leak<T: Sized>(v: T) -> &'static T {
    unsafe {
        let layout = core::alloc::Layout::new::<T>();
        // Do not use `alloc_unchecked` to avoid generic fn.
        let ptr = Global::alloc(layout).cast::<T>();
        core::ptr::write(ptr.as_ptr(), v);
        &mut *ptr.as_ptr()
    }
}

// -----------------------------------------------------------------------------
// Label

/// Defines a label trait and its global interner.
///
/// This macro generates:
/// - A trait with dynamic clone and intern support.
/// - An implementation for [`Interned<dyn Trait>`]-style values.
/// - Dynamic `Eq`/`Hash` behavior for the trait object.
/// - A static [`Interner`] used by `intern()`.
///
/// The 2-argument form creates a trait with only the default methods.
/// The extended form accepts additional trait methods and an implementation
/// block for `Interned<dyn Trait>`.
///
/// For example, `ScheduleLabel` is a trait with multiple concrete
/// implementations. Using [`Interned`] gives each label value a canonical
/// `'static` reference and ensures each distinct logical value is stored once.
///
/// # Warning
///
/// The label type's [`Clone`] implementation, should not call
/// label trait's `intern` internally. Otherwise, deadlock may occur.
///
/// # Examples
///
/// ```
/// use zlim_core::define_label;
///
/// define_label!(
///     /// Example label trait.
///     ExampleLabel,
///     EXAMPLE_LABEL_INTERNER
/// );
///
/// #[derive(Clone, Debug, Hash, PartialEq, Eq)]
/// struct MainSchedule;
///
/// impl ExampleLabel for MainSchedule {
///     fn clone(v: &Self) -> Self { v.clone() }
/// }
///
/// let a = MainSchedule.intern();
/// let b = MainSchedule.intern();
/// assert_eq!(a, b);
/// ```
#[macro_export]
macro_rules! define_label {
    (
        $(#[$label_attr:meta])*
        $label_trait_name:ident,
        $interner_name:ident $(,)?
    ) => {

        $(#[$label_attr])*
        pub trait $label_trait_name: Send + Sync + ::core::fmt::Debug + $crate::label::DynEq + $crate::label::DynHash {
            fn clone(v: &Self) -> Self
            where
                Self: Sized;

            /// Returns the canonical interned handle corresponding to `self`.
            fn intern(&self) -> $crate::label::Interned<dyn $label_trait_name>
            where
                Self: Sized
            {
                if let Some(v) = $interner_name.get(self) {
                    return v;
                }
                ::core::hint::cold_path();

                let f = move || {
                    let cloned = $label_trait_name::clone(self);
                    $crate::label::leak(cloned) as &'static dyn $label_trait_name
                };
                $interner_name.intern(self, &f)
            }
        }

        #[diagnostic::do_not_recommend]
        impl $label_trait_name for $crate::label::Interned<dyn $label_trait_name> {
            #[inline(always)]
            fn clone(v: &Self) -> Self { *v }

            #[inline(always)]
            fn intern(&self) -> Self { *self }
        }

        impl ::core::hash::Hash for dyn $label_trait_name {
            fn hash<H: ::core::hash::Hasher>(&self, state: &mut H) {
                self.dyn_hash(state);
            }
        }

        impl ::core::cmp::PartialEq for dyn $label_trait_name {
            fn eq(&self, other: &Self) -> bool {
                self.dyn_eq(other)
            }
        }

        impl ::core::cmp::Eq for dyn $label_trait_name {}

        impl $crate::label::Internable for dyn $label_trait_name {
            fn ref_eq(&self, other: &Self) -> bool {
                let x_ptr = ::core::ptr::from_ref::<Self>(self);
                let y_ptr = ::core::ptr::from_ref::<Self>(other);

                // Test that both the type id and pointer address are equivalent.
                self.type_id() == other.type_id() && ::core::ptr::addr_eq(x_ptr, y_ptr)
            }

            fn ref_hash(&self, mut state: &mut dyn ::core::hash::Hasher) {
                // Hash the type id...
                ::core::hash::Hash::hash(&self.type_id(), &mut state);

                // ...and the pointer address.
                // Cast to a unit `()` first to discard any pointer metadata.
                let ptr = ::core::ptr::from_ref::<Self>(self) as *const ();
                ::core::hash::Hash::hash(&ptr, &mut state);
            }
        }

        static $interner_name: $crate::label::Interner<dyn $label_trait_name> = $crate::label::Interner::new();
    };
}

// -----------------------------------------------------------------------------
