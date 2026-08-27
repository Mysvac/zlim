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

use zlim_utils::hash::HashSet;

// -----------------------------------------------------------------------------
// Internable

/// A value that can be interned into a stable `'static` reference.
///
/// Implementations define how values are leaked, how pointer-level equality is
/// checked, and how pointer identity is hashed.
///
/// # Implementer Notes
///
/// `leak`, `ref_eq`, and `ref_hash` must describe the same identity model.
/// If two references are considered equal by [`Internable::ref_eq`], they must
/// also produce identical hashes via [`Internable::ref_hash`].
///
/// In most cases this means:
/// - `leak` returns a canonical `'static` reference,
/// - `ref_eq` compares reference identity,
/// - `ref_hash` hashes the same reference identity.
pub trait Internable: Hash + Eq + 'static {
    /// Creates a static reference to `self`, possibly leaking memory.
    fn leak(&self) -> &'static Self;
    /// Returns `true` if the two references point to the same value.
    fn ref_eq(&self, other: &Self) -> bool;
    /// Feeds the reference to the hasher.
    fn ref_hash(&self, state: &mut dyn Hasher);
}

// -----------------------------------------------------------------------------
// Interned

/// A lightweight handle to an interned value.
///
/// This type is primarily used by Label implementations:
/// - It stores a canonical `'static` reference, so cloning is just copying a pointer.
/// - Equality and hashing use identity semantics through
///   [`Internable::ref_eq`] and [`Internable::ref_hash`].
///
/// Equivalent label values resolve to the same interned instance.
///
/// # Examples
/// ```
/// # use core::hash::Hasher;
/// # use zlim_core::label::{Internable, Interner};
/// #
/// #[derive(Debug, Hash, PartialEq, Eq)]
/// enum Stage {
///     Update,
///     Render,
/// }
///
/// impl Internable for Stage {
///     fn leak(&self) -> &'static Self {
///         match self {
///             Stage::Update => &Stage::Update,
///             Stage::Render => &Stage::Render,
///         }
///     }
///
///     fn ref_eq(&self, other: &Self) -> bool {
///         core::ptr::eq(self, other)
///     }
///
///     fn ref_hash(&self, mut state: &mut dyn Hasher) {
///         core::ptr::hash(self, &mut state);
///     }
/// }
///
/// let interner = Interner::new();
/// let x = interner.intern(&Stage::Update);
/// let y = interner.intern(&Stage::Update);
/// assert_eq!(x, y);
/// ```
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
pub struct Interner<T: ?Sized + 'static>(RwLock<HashSet<&'static T>>);

impl<T: ?Sized> Interner<T> {
    /// Creates a new empty interner
    pub const fn new() -> Self {
        Self(RwLock::new(HashSet::new()))
    }
}

impl<T: ?Sized> Default for Interner<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ?Sized + Internable> Interner<T> {
    /// Returns the [`Interned<T>`] corresponding to `value`.
    ///
    /// On first encounter, the value may be leaked to obtain a stable `'static`
    /// reference. Subsequent calls with an equivalent value return an
    /// [`Interned<T>`] backed by the same reference.
    ///
    /// # Examples
    /// ```
    /// # use core::hash::Hasher;
    /// # use zlim_core::label::{Internable, Interner};
    /// #
    /// #[derive(Debug, Hash, PartialEq, Eq)]
    /// struct Marker;
    ///
    /// impl Internable for Marker {
    ///     fn leak(&self) -> &'static Self {
    ///         &Marker
    ///     }
    ///
    ///     fn ref_eq(&self, other: &Self) -> bool {
    ///         core::ptr::eq(self, other)
    ///     }
    ///
    ///     fn ref_hash(&self, mut state: &mut dyn Hasher) {
    ///         core::ptr::hash(self, &mut state);
    ///     }
    /// }
    ///
    /// let interner = Interner::new();
    /// let a = interner.intern(&Marker);
    /// let b = interner.intern(&Marker);
    /// assert_eq!(a, b);
    /// ```
    pub fn intern(&self, value: &T) -> Interned<T> {
        {
            let set = self.0.read().unwrap_or_else(PoisonError::into_inner);

            if let Some(val) = set.get(value) {
                return Interned(*val);
            }
        }

        {
            let mut set = self.0.write().unwrap_or_else(PoisonError::into_inner);

            let val = set.get_or_insert_with(value, |_| value.leak());
            Interned(*val)
        }
    }
}

// -----------------------------------------------------------------------------
// Dyn Hash/Eq

/// Type-erased equality for label trait objects.
///
/// This is used so `dyn LabelTrait` values can be compared without knowing the
/// concrete type at compile time.
pub trait DynEq: Any {
    /// Compares two dynamic values for equality.
    fn dyn_eq(&self, other: &dyn DynEq) -> bool;
}

/// Type-erased hashing for label trait objects.
///
/// Implementations should include both value hash and type identity to avoid
/// collisions between different concrete types with matching value bits.
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
/// # Examples
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
///     fn dyn_clone(&self) -> Box<dyn ExampleLabel> {
///         Box::new(self.clone())
///     }
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
        $crate::define_label!(
            $(#[$label_attr])*
            $label_trait_name,
            $interner_name,
            extra_methods: {},
            extra_methods_impl: {}
        );
    };
    (
        $(#[$label_attr:meta])*
        $label_trait_name:ident,
        $interner_name:ident,
        extra_methods: { $($trait_extra_methods:tt)* } ,
        extra_methods_impl: { $($interned_extra_methods_impl:tt)* } $(,)?
    ) => {

        $(#[$label_attr])*
        pub trait $label_trait_name: Send + Sync + ::core::fmt::Debug + $crate::label::DynEq + $crate::label::DynHash {

            $($trait_extra_methods)*

            #[doc = concat!("Clones this `", stringify!($label_trait_name), "`.")]
            fn dyn_clone(&self) -> ::std::boxed::Box<dyn $label_trait_name>;

            /// Returns the canonical interned handle corresponding to `self`.
            fn intern(&self) -> $crate::label::Interned<dyn $label_trait_name>
            where
                Self: Sized
            {
                $interner_name.intern(self)
            }
        }

        #[diagnostic::do_not_recommend]
        impl $label_trait_name for $crate::label::Interned<dyn $label_trait_name> {

            $($interned_extra_methods_impl)*

            fn dyn_clone(&self) -> ::std::boxed::Box<dyn $label_trait_name> {
                (**self).dyn_clone()
            }

            fn intern(&self) -> Self {
                *self
            }
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
            fn leak(&self) -> &'static Self {
                ::std::boxed::Box::leak(self.dyn_clone())
            }

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

        static $interner_name: $crate::label::Interner<dyn $label_trait_name> =
            $crate::label::Interner::new();
    };
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use core::hash::{Hash, Hasher};

    use super::{Internable, Interner};

    #[test]
    fn zero_sized_type() {
        #[derive(PartialEq, Eq, Hash, Debug)]
        pub struct A;

        impl Internable for A {
            fn leak(&self) -> &'static Self {
                &A
            }

            fn ref_eq(&self, other: &Self) -> bool {
                core::ptr::eq(self, other)
            }

            fn ref_hash(&self, mut state: &mut dyn Hasher) {
                core::ptr::hash(self, &mut state);
            }
        }

        let interner = Interner::default();
        let x = interner.intern(&A);
        let y = interner.intern(&A);
        assert_eq!(x, y);
    }

    #[test]
    fn fieldless_enum() {
        #[derive(PartialEq, Eq, Hash, Debug)]
        pub enum A {
            X,
            Y,
        }

        impl Internable for A {
            fn leak(&self) -> &'static Self {
                match self {
                    A::X => &A::X,
                    A::Y => &A::Y,
                }
            }

            fn ref_eq(&self, other: &Self) -> bool {
                core::ptr::eq(self, other)
            }

            fn ref_hash(&self, mut state: &mut dyn Hasher) {
                core::ptr::hash(self, &mut state);
            }
        }

        let interner = Interner::default();
        let x1 = interner.intern(&A::X);
        let x2 = interner.intern(&A::X);
        let y = interner.intern(&A::Y);
        assert_ne!(x1, y);
        assert_eq!(x1, x2);
    }
}

// -----------------------------------------------------------------------------
