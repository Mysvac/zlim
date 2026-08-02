use core::fmt::{self, Debug, Formatter};

use zlim_utils::hash::HashMap;
use zlim_utils::hash::NoopState;

use crate::Reflect;
use crate::info::ReflectKind;
use crate::ops::ApplyError;
use crate::ops::CloneError;
use crate::ops::Map;

use super::{impl_dynamic_reflect_cast, impl_dynamic_type_info, impl_dynamic_type_path};

// ----------------------------------------------------------------------------
// DynamicMap
// ----------------------------------------------------------------------------

/// A dynamic container representing a key-value map.
///
/// Keys must support hashing via [`Reflect::reflect_hash`] and equality via
/// [`Reflect::reflect_eq`].
///
/// # Conversion
///
/// `DynamicMap` can be constructed from any type that implements
/// [`Map`](crate::ops::Map) via [`from_ref`](Self::from_ref), and
/// can be converted back to a concrete type via
/// [`from_reflect`](crate::Reflect::from_reflect).
///
/// # Examples
///
/// ```no_run
/// use zlim_reflect::dynamic::DynamicMap;
/// use zlim_reflect::ops::Map;
///
/// // Construct from standard reflected types.
/// let mut map = DynamicMap::new();
/// map.insert(Box::new("key_a"), Box::new(100i32));
/// map.insert(Box::new("key_b"), Box::new(200i32));
///
/// assert_eq!(map.entry_len(), 2);
/// assert!(map.value(&*Box::new("key_a")).is_some());
/// ```
#[derive(Default)]
pub struct DynamicMap {
    table: HashMap<Box<dyn Reflect>, Box<dyn Reflect>, NoopState>,
}

impl_dynamic_type_path!(DynamicMap);
impl_dynamic_type_info!(DynamicMap);

impl DynamicMap {
    /// Creates an empty `DynamicMap`.
    #[inline]
    pub const fn new() -> Self {
        Self {
            table: HashMap::with_hasher(NoopState),
        }
    }

    /// Creates an empty `DynamicMap` with at least the specified capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            table: HashMap::with_capacity_and_hasher(capacity, NoopState),
        }
    }

    /// Inserts a boxed key-value pair into the map.
    ///
    /// Returns the old value if the key already existed.
    ///
    /// # Panics
    ///
    /// Panics if the key is not `reflect_eq` to itself.
    pub fn insert(
        &mut self,
        key: Box<dyn Reflect>,
        value: Box<dyn Reflect>,
    ) -> Option<Box<dyn Reflect>> {
        debug_assert!(
            key.reflect_eq(&*key),
            "The key is not `reflect_eq` to itself: `{}`.",
            key.reflect_type_path(),
        );
        self.table.insert(key, value)
    }

    /// Constructs a `DynamicMap` by cloning each entry from a [`Map`].
    ///
    /// This is the fallible version of [`Reflect::reflect_clone`]; it
    /// propagates any [`CloneError`] from inner keys or values.
    #[inline(never)]
    pub fn from_ref(value: &dyn Map) -> Result<Self, CloneError> {
        let hint = value.entry_len();
        let mut map = Self::with_capacity(hint);
        for (k, v) in value.iter_entries() {
            map.insert(k.reflect_clone()?, v.reflect_clone()?);
        }
        Ok(map)
    }

    /// Fallible clone that propagates [`CloneError`] from inner entries.
    ///
    /// Unlike [`reflect_clone`](Reflect::reflect_clone) which panics on
    /// failure, this returns `Err` so callers can inspect the error.
    pub fn try_clone(&self) -> Result<Self, CloneError> {
        let mut map = DynamicMap::with_capacity(self.table.len());
        for (key, value) in self.table.iter() {
            let k = key.reflect_clone()?;
            let v = value.reflect_clone()?;
            map.insert(k, v);
        }
        Ok(map)
    }
}

// ----------------------------------------------------------------------------
// Reflect
// ----------------------------------------------------------------------------

impl Reflect for DynamicMap {
    impl_dynamic_reflect_cast!(Map);

    #[inline]
    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        self.try_clone().map(|x| Box::new(x) as Box<dyn Reflect>)
    }

    #[inline]
    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        crate::impls::map_apply(self, value)
    }

    #[inline]
    fn reflect_hash(&self) -> u64 {
        crate::impls::map_hash(self)
    }

    #[inline]
    fn reflect_eq(&self, other: &dyn Reflect) -> bool {
        crate::impls::map_eq(self, other)
    }

    #[inline]
    fn reflect_debug(&self, f: &mut Formatter) -> fmt::Result {
        crate::impls::map_debug(self, f)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>>
    where
        Self: Sized,
    {
        // Fast path: already a DynamicMap.
        if value.is::<Self>() {
            return value.downcast::<Self>();
        }

        if value.reflect_kind() != ReflectKind::Map {
            return Err(value);
        }

        let mut value = value.reflect_owned().into_map().unwrap();

        let mut dynamic = DynamicMap::with_capacity(value.entry_len());
        for (key, val) in value.drain_all() {
            dynamic.insert(key, val);
        }

        Ok(Box::new(dynamic))
    }
}

impl Debug for DynamicMap {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        crate::impls::map_debug(self, f)
    }
}

// ----------------------------------------------------------------------------
// Map
// ----------------------------------------------------------------------------

impl Map for DynamicMap {
    fn value(&self, key: &dyn Reflect) -> Option<&dyn Reflect> {
        self.table.get(key).map(|x| &**x)
    }

    fn value_mut(&mut self, key: &dyn Reflect) -> Option<&mut dyn Reflect> {
        self.table.get_mut(key).map(|x| &mut **x)
    }

    fn entry_len(&self) -> usize {
        self.table.len()
    }

    fn iter_entries(&self) -> Box<dyn Iterator<Item = (&dyn Reflect, &dyn Reflect)> + '_> {
        Box::new(self.table.iter().map(|(k, v)| (&**k, &**v)))
    }

    fn insert_entry(
        &mut self,
        key: Box<dyn Reflect>,
        value: Box<dyn Reflect>,
    ) -> Result<bool, (Box<dyn Reflect>, Box<dyn Reflect>)> {
        Ok(self.insert(key, value).is_none())
    }

    fn remove_entry(&mut self, key: &dyn Reflect) -> Option<Box<dyn Reflect>> {
        self.table.remove(key)
    }

    fn retain_entry(&mut self, f: &mut dyn FnMut(&dyn Reflect, &mut dyn Reflect) -> bool) {
        self.table.retain(move |key, value| f(&**key, &mut **value));
    }

    fn drain_all(&mut self) -> Vec<(Box<dyn Reflect>, Box<dyn Reflect>)> {
        self.table.drain().collect()
    }
}
