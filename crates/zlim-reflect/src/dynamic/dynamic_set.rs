use core::fmt::{self, Debug, Formatter};

use zlim_utils::hash::HashSet;
use zlim_utils::hash::NoopState;

use crate::Reflect;
use crate::info::ReflectKind;
use crate::ops::ApplyError;
use crate::ops::CloneError;
use crate::ops::Set;

use super::{impl_dynamic_reflect_cast, impl_dynamic_type_info, impl_dynamic_type_path};

// ----------------------------------------------------------------------------
// DynamicSet
// ----------------------------------------------------------------------------

/// A dynamic container representing a set of unique values.
///
/// Values must support hashing via [`Reflect::reflect_hash`] and equality via
/// [`Reflect::reflect_eq`].
///
/// # Conversion
///
/// `DynamicSet` can be constructed from any type that implements
/// [`Set`](crate::ops::Set) via [`from_ref`](Self::from_ref), and
/// can be converted back to a concrete type via
/// [`from_reflect`](crate::Reflect::from_reflect).
///
/// # Examples
///
/// ```no_run
/// use zlim_reflect::dynamic::DynamicSet;
/// use zlim_reflect::ops::Set;
///
/// // Construct from standard reflected types.
/// let mut set = DynamicSet::new();
/// set.insert(Box::new(1i32));
/// set.insert(Box::new(2i32));
/// set.insert(Box::new(1i32)); // duplicate — ignored
///
/// assert_eq!(set.value_len(), 2);
/// ```
#[derive(Default)]
pub struct DynamicSet {
    table: HashSet<Box<dyn Reflect>, NoopState>,
}

impl_dynamic_type_path!(DynamicSet);
impl_dynamic_type_info!(DynamicSet);

impl DynamicSet {
    /// Creates an empty `DynamicSet`.
    #[inline]
    pub const fn new() -> Self {
        Self {
            table: HashSet::with_hasher(NoopState),
        }
    }

    /// Creates an empty `DynamicSet` with at least the specified capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            table: HashSet::with_capacity_and_hasher(capacity, NoopState),
        }
    }

    /// Inserts a boxed value into the set.
    ///
    /// - If the set did not have this value present, `true` is returned.
    /// - If the set did have this value present, `false` is returned.
    ///
    /// # Panics
    ///
    /// Panics if the value is not `reflect_eq` to itself.
    pub fn insert(&mut self, value: Box<dyn Reflect>) -> bool {
        debug_assert!(
            value.reflect_eq(&*value),
            "The value is not `reflect_eq` to itself: `{}`.",
            value.reflect_type_path(),
        );

        self.table.insert(value)
    }

    /// Constructs a `DynamicSet` by cloning each element from a [`Set`].
    ///
    /// This is the fallible version of [`Reflect::reflect_clone`]; it
    /// propagates any [`CloneError`] from inner elements.
    #[inline(never)]
    pub fn from_ref(value: &dyn Set) -> Result<Self, CloneError> {
        let hint = value.value_len();
        let mut set = Self::with_capacity(hint);
        for v in value.iter_values() {
            set.insert(v.reflect_clone()?);
        }
        Ok(set)
    }

    /// Fallible clone that propagates [`CloneError`] from inner elements.
    ///
    /// Unlike [`reflect_clone`](Reflect::reflect_clone) which panics on
    /// failure, this returns `Err` so callers can inspect the error.
    pub fn try_clone(&self) -> Result<Self, CloneError> {
        let mut set = DynamicSet::with_capacity(self.table.len());
        for value in self.table.iter() {
            set.insert(value.reflect_clone()?);
        }
        Ok(set)
    }
}

// ----------------------------------------------------------------------------
// Reflect
// ----------------------------------------------------------------------------

impl Reflect for DynamicSet {
    impl_dynamic_reflect_cast!(Set);

    #[inline]
    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        self.try_clone().map(|x| Box::new(x) as Box<dyn Reflect>)
    }

    #[inline]
    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        crate::impls::set_apply(self, value)
    }

    #[inline]
    fn reflect_hash(&self) -> u64 {
        crate::impls::set_hash(self)
    }

    #[inline]
    fn reflect_eq(&self, other: &dyn Reflect) -> bool {
        crate::impls::set_eq(self, other)
    }

    #[inline]
    fn reflect_debug(&self, f: &mut Formatter) -> fmt::Result {
        crate::impls::set_debug(self, f)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>>
    where
        Self: Sized,
    {
        // Fast path: already a DynamicSet.
        if value.is::<Self>() {
            return value.downcast::<Self>();
        }

        if value.reflect_kind() != ReflectKind::Set {
            return Err(value);
        }

        let mut value = value.reflect_owned().into_set().unwrap();
        let mut dynamic = DynamicSet::with_capacity(value.value_len());
        for value in value.drain_all() {
            dynamic.insert(value);
        }

        Ok(Box::new(dynamic))
    }
}

impl Debug for DynamicSet {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        crate::impls::set_debug(self, f)
    }
}

// ----------------------------------------------------------------------------
// Set
// ----------------------------------------------------------------------------

impl Set for DynamicSet {
    fn value(&self, value: &dyn Reflect) -> Option<&dyn Reflect> {
        self.table.get(value).map(|x| &**x)
    }

    fn value_len(&self) -> usize {
        self.table.len()
    }

    fn iter_values(&self) -> Box<dyn Iterator<Item = &dyn Reflect> + '_> {
        Box::new(self.table.iter().map(|v| &**v))
    }

    fn insert_value(&mut self, value: Box<dyn Reflect>) -> Result<bool, Box<dyn Reflect>> {
        Ok(self.insert(value))
    }

    fn remove_value(&mut self, value: &dyn Reflect) -> bool {
        self.table.remove(value)
    }

    fn retain_value(&mut self, f: &mut dyn FnMut(&dyn Reflect) -> bool) {
        self.table.retain(move |value| f(&**value));
    }

    fn drain_all(&mut self) -> Vec<Box<dyn Reflect>> {
        self.table.drain().collect()
    }
}
