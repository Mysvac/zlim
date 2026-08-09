use core::fmt::{self, Debug, Formatter};

use crate::Reflect;
use crate::info::ReflectKind;
use crate::ops::ApplyError;
use crate::ops::CloneError;
use crate::ops::List;
use crate::ops::ListItemIter;

use super::{impl_dynamic_reflect_cast, impl_dynamic_type_info, impl_dynamic_type_path};

// -----------------------------------------------------------------------------
// DynamicList
// -----------------------------------------------------------------------------

/// A dynamic container representing a growable list.
///
/// Supports push/pop operations at both ends, matching the semantics of
/// [`Vec<T>`] and similar collections.
///
/// # Conversion
///
/// `DynamicList` can be constructed from any type that implements
/// [`List`](crate::ops::List) via [`from_ref`](Self::from_ref), and
/// can be converted back to a concrete type via
/// [`from_reflect`](crate::Reflect::from_reflect).
///
/// # Examples
///
/// ```no_run
/// use zlim_reflect::dynamic::DynamicList;
/// use zlim_reflect::ops::List;
///
/// // Construct from standard reflected types.
/// let mut dynamic = DynamicList::new();
/// dynamic.push_back(Box::new(10i32));
/// dynamic.push_back(Box::new(20i32));
/// dynamic.push_back(Box::new(30i32));
///
/// assert_eq!(dynamic.item_len(), 3);
/// assert!(dynamic.pop_back().is_some());
/// assert_eq!(dynamic.item_len(), 2);
/// ```
#[derive(Default)]
pub struct DynamicList {
    values: Vec<Box<dyn Reflect>>,
}

impl_dynamic_type_path!(DynamicList);
impl_dynamic_type_info!(DynamicList);

impl DynamicList {
    /// Creates an empty `DynamicList`.
    #[inline]
    pub const fn new() -> Self {
        Self { values: Vec::new() }
    }

    /// Creates an empty `DynamicList` with at least the specified capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            values: Vec::with_capacity(capacity),
        }
    }

    /// Appends a boxed [`Reflect`] value to the end of the list.
    #[inline]
    pub fn push(&mut self, value: Box<dyn Reflect>) {
        self.values.push(value);
    }

    /// Constructs a `DynamicList` by cloning each element from a [`List`].
    ///
    /// This is the fallible version of [`Reflect::reflect_clone`]; it
    /// propagates any [`CloneError`] from inner elements.
    #[inline(never)]
    pub fn from_ref(value: &dyn List) -> Result<Self, CloneError> {
        let hint = value.item_len();
        let mut values = Vec::with_capacity(hint);
        for v in value.iter_items() {
            values.push(v.reflect_clone()?);
        }
        Ok(Self { values })
    }

    /// Fallible clone that propagates [`CloneError`] from inner elements.
    ///
    /// Unlike [`reflect_clone`](Reflect::reflect_clone) which panics on
    /// failure, this returns `Err` so callers can inspect the error.
    pub fn try_clone(&self) -> Result<Self, CloneError> {
        let hint = self.values.len();
        let mut values = Vec::with_capacity(hint);
        for v in &self.values {
            values.push(v.reflect_clone()?);
        }
        Ok(Self { values })
    }
}

// -----------------------------------------------------------------------------
// Reflect
// -----------------------------------------------------------------------------

impl Reflect for DynamicList {
    impl_dynamic_reflect_cast!(List);

    #[inline]
    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        self.try_clone().map(|x| Box::new(x) as Box<dyn Reflect>)
    }

    #[inline]
    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        crate::impls::list_apply(self, value)
    }

    #[inline]
    fn reflect_hash(&self) -> u64 {
        crate::impls::list_hash(self)
    }

    #[inline]
    fn reflect_eq(&self, other: &dyn Reflect) -> bool {
        crate::impls::list_eq(self, other)
    }

    #[inline]
    fn reflect_debug(&self, f: &mut Formatter) -> fmt::Result {
        crate::impls::list_debug(self, f)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>>
    where
        Self: Sized,
    {
        if value.is::<Self>() {
            return value.downcast::<Self>();
        }

        if value.reflect_kind() != ReflectKind::List {
            return Err(value);
        }

        let mut value = value.reflect_owned().into_list().unwrap();

        Ok(Box::new(Self {
            values: value.drain_all(),
        }))
    }
}

impl Debug for DynamicList {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        crate::impls::list_debug(self, f)
    }
}

// -----------------------------------------------------------------------------
// List
// -----------------------------------------------------------------------------

impl List for DynamicList {
    #[inline]
    fn item(&self, index: usize) -> Option<&dyn Reflect> {
        self.values.get(index).map(|v| &**v)
    }

    #[inline]
    fn item_mut(&mut self, index: usize) -> Option<&mut dyn Reflect> {
        self.values.get_mut(index).map(|v| &mut **v)
    }

    #[inline]
    fn item_len(&self) -> usize {
        self.values.len()
    }

    #[inline]
    fn iter_items(&self) -> ListItemIter<'_> {
        ListItemIter::new(self)
    }

    #[inline]
    fn push_back(&mut self, value: Box<dyn Reflect>) -> Result<(), Box<dyn Reflect>> {
        self.values.push(value);
        Ok(())
    }

    #[inline]
    fn push_front(&mut self, value: Box<dyn Reflect>) -> Result<(), Box<dyn Reflect>> {
        self.values.insert(0, value);
        Ok(())
    }

    #[inline]
    fn pop_back(&mut self) -> Option<Box<dyn Reflect>> {
        self.values.pop()
    }

    #[inline]
    fn pop_front(&mut self) -> Option<Box<dyn Reflect>> {
        if self.values.is_empty() {
            None
        } else {
            Some(self.values.remove(0))
        }
    }

    #[inline]
    fn drain_all(&mut self) -> Vec<Box<dyn Reflect>> {
        core::mem::take(&mut self.values)
    }
}
