use core::any::TypeId;
use core::hash::{BuildHasher, Hash};

use zlim_utils::hash::{FixedState, NoopState, SparseState};
use zlim_utils::hash::{HashMap, HashSet};

use crate::Reflect;
use crate::db::{TypeDB, TypeDatabase};
use crate::impls::impl_simple_type_path;
use crate::impls::{CLONE_TYPE_ERROR, COMPATIBLE_ERROR, is_convertable};
use crate::info::{GenericInfo, Generics, InfoCell, MapInfo};
use crate::info::{SetInfo, TypeInfo, TypeParamInfo, Typed};
use crate::ops::{ApplyError, CloneError, Map, ReflectRef, Set};
use crate::path::TypePath;

// -----------------------------------------------------------------------------
// TypePath
// -----------------------------------------------------------------------------

impl_simple_type_path!(FixedState:  "zlim_utils", "hash", "FixedState");
impl_simple_type_path!(NoopState:   "zlim_utils", "hash", "NoopState");
impl_simple_type_path!(SparseState: "zlim_utils", "hash", "SparseState");

impl_simple_type_path!(@HashSet<K, S>:    "zlim_utils", "hash", "HashSet");
impl_simple_type_path!(@HashMap<K, V, S>: "zlim_utils", "hash", "HashMap");

// -----------------------------------------------------------------------------
// HashSet — Typed
// -----------------------------------------------------------------------------

impl<T, S> Typed for HashSet<T, S>
where
    T: Reflect + Typed + Eq + Hash,
    S: TypePath + BuildHasher + Default + Send + Sync,
{
    fn type_info() -> &'static TypeInfo {
        static CELL: InfoCell = InfoCell::new();
        CELL.get_or_init::<Self>(|| {
            TypeInfo::Set(SetInfo::new::<Self, T>().with_generics(Generics::new(&[
                GenericInfo::Type(TypeParamInfo::new::<T>("T")),
                GenericInfo::Type(TypeParamInfo::new::<S>("S").with_default::<FixedState>()),
            ])))
        })
    }
}

// -----------------------------------------------------------------------------
// HashSet — Set
// -----------------------------------------------------------------------------

impl<T, S> Set for HashSet<T, S>
where
    T: Reflect + Typed + Eq + Hash,
    S: TypePath + BuildHasher + Default + Send + Sync,
{
    fn value(&self, value: &dyn Reflect) -> Option<&dyn Reflect> {
        let item: &T = value.downcast_ref()?;
        self.get(item).map(|x| x as &dyn Reflect)
    }

    fn value_len(&self) -> usize {
        self.len()
    }

    fn iter_values(&self) -> Box<dyn Iterator<Item = &dyn Reflect> + '_> {
        Box::new(self.iter().map(|x| x as &dyn Reflect))
    }

    fn insert_value(&mut self, value: Box<dyn Reflect>) -> Result<bool, Box<dyn Reflect>> {
        let value = T::from_reflect(value)?;
        Ok(self.insert(*value))
    }

    fn remove_value(&mut self, value: &dyn Reflect) -> bool {
        let Some(item) = value.downcast_ref::<T>() else {
            return false;
        };
        self.remove(item)
    }

    fn retain_value(&mut self, f: &mut dyn FnMut(&dyn Reflect) -> bool) {
        self.retain(|v| f(v as &dyn Reflect));
    }

    fn drain_all(&mut self) -> Vec<Box<dyn Reflect>> {
        core::mem::take(self)
            .into_iter()
            .map(|x| Box::new(x) as Box<dyn Reflect>)
            .collect()
    }
}

// -----------------------------------------------------------------------------
// HashSet — Reflect
// -----------------------------------------------------------------------------

impl<T, S> Reflect for HashSet<T, S>
where
    T: Reflect + Typed + Eq + Hash,
    S: TypePath + BuildHasher + Default + Send + Sync,
{
    crate::impls::impl_reflect_kind!(Set);

    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        let mut set = Self::with_capacity_and_hasher(self.len(), S::default());
        for item in self.iter() {
            let it = item.reflect_clone()?;
            set.insert(it.take::<T>().expect(CLONE_TYPE_ERROR));
        }
        Ok(Box::new(set))
    }

    #[inline]
    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        crate::impls::set_apply(self, value)
    }

    #[inline]
    fn reflect_eq(&self, value: &dyn Reflect) -> bool {
        crate::impls::set_eq(self, value)
    }

    #[inline]
    fn reflect_hash(&self) -> u64 {
        crate::impls::set_hash(self)
    }

    #[inline]
    fn reflect_debug(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        crate::impls::set_debug(self, f)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>> {
        let value = match value.downcast::<Self>() {
            Ok(ret) => return Ok(ret),
            Err(e) => e,
        };

        let ReflectRef::Set(v) = value.reflect_ref() else {
            return Err(value);
        };

        if v.iter_values().any(|v| !v.is::<T>()) {
            return Err(value);
        }

        let mut set_v = value.reflect_owned().into_set().unwrap();

        let items: Vec<Box<dyn Reflect>> = set_v.drain_all();
        let mut set = Self::with_capacity_and_hasher(items.len(), S::default());
        for item in items {
            set.insert(item.take::<T>().expect(COMPATIBLE_ERROR));
        }
        Ok(Box::new(set))
    }
}

// -----------------------------------------------------------------------------
// HashSet — TypeDatabase
// -----------------------------------------------------------------------------

impl<T: TypeDatabase + Eq + Hash, S: TypePath + BuildHasher + Default + Send + Sync> TypeDatabase
    for HashSet<T, S>
{
    fn on_register(db: &'static TypeDB) {
        db.insert_defaultor::<Self>();
    }

    fn register_dependencies() {
        TypeDB::register::<T>();
    }
}

// -----------------------------------------------------------------------------
// HashMap — Typed
// -----------------------------------------------------------------------------

impl<K, V, S> Typed for HashMap<K, V, S>
where
    K: Reflect + Typed + Eq + Hash,
    V: Reflect + Typed,
    S: TypePath + BuildHasher + Default + Send + Sync,
{
    fn type_info() -> &'static TypeInfo {
        static CELL: InfoCell = InfoCell::new();
        CELL.get_or_init::<Self>(|| {
            TypeInfo::Map(MapInfo::new::<Self, K, V>().with_generics(Generics::new(&[
                GenericInfo::Type(TypeParamInfo::new::<K>("K")),
                GenericInfo::Type(TypeParamInfo::new::<V>("V")),
                GenericInfo::Type(TypeParamInfo::new::<S>("S").with_default::<FixedState>()),
            ])))
        })
    }
}

// -----------------------------------------------------------------------------
// HashMap — Map
// -----------------------------------------------------------------------------

impl<K, V, S> Map for HashMap<K, V, S>
where
    K: Reflect + Typed + Eq + Hash,
    V: Reflect + Typed,
    S: TypePath + BuildHasher + Default + Send + Sync,
{
    fn value(&self, key: &dyn Reflect) -> Option<&dyn Reflect> {
        let k: &K = key.downcast_ref()?;
        self.get(k).map(|x| x as &dyn Reflect)
    }

    fn value_mut(&mut self, key: &dyn Reflect) -> Option<&mut dyn Reflect> {
        let k: &K = key.downcast_ref()?;
        self.get_mut(k).map(|x| x as &mut dyn Reflect)
    }

    fn entry_len(&self) -> usize {
        self.len()
    }

    fn iter_entries(&self) -> Box<dyn Iterator<Item = (&dyn Reflect, &dyn Reflect)> + '_> {
        Box::new(
            self.iter()
                .map(|(k, v)| (k as &dyn Reflect, v as &dyn Reflect)),
        )
    }

    fn insert_entry(
        &mut self,
        key: Box<dyn Reflect>,
        value: Box<dyn Reflect>,
    ) -> Result<bool, (Box<dyn Reflect>, Box<dyn Reflect>)> {
        if !is_convertable(&*key, TypeId::of::<K>()) {
            return Err((key, value));
        }
        if !is_convertable(&*value, TypeId::of::<V>()) {
            return Err((key, value));
        }
        let key = K::from_reflect(key).expect(COMPATIBLE_ERROR);
        let value = V::from_reflect(value).expect(COMPATIBLE_ERROR);
        Ok(self.insert(*key, *value).is_some())
    }

    fn remove_entry(&mut self, key: &dyn Reflect) -> Option<Box<dyn Reflect>> {
        let k: &K = key.downcast_ref()?;
        self.remove(k).map(|x| Box::new(x) as Box<dyn Reflect>)
    }

    fn retain_entry(&mut self, f: &mut dyn FnMut(&dyn Reflect, &mut dyn Reflect) -> bool) {
        self.retain(|x, v| f(x as &dyn Reflect, v as &mut dyn Reflect));
    }

    fn drain_all(&mut self) -> Vec<(Box<dyn Reflect>, Box<dyn Reflect>)> {
        core::mem::take(self)
            .into_iter()
            .map(|(k, v)| {
                (
                    Box::new(k) as Box<dyn Reflect>,
                    Box::new(v) as Box<dyn Reflect>,
                )
            })
            .collect()
    }
}

// -----------------------------------------------------------------------------
// HashMap — Reflect
// -----------------------------------------------------------------------------

impl<K, V, S> Reflect for HashMap<K, V, S>
where
    K: Reflect + Typed + Eq + Hash,
    V: Reflect + Typed,
    S: TypePath + BuildHasher + Default + Send + Sync,
{
    crate::impls::impl_reflect_kind!(Map);

    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        let mut map = Self::with_capacity_and_hasher(self.len(), S::default());
        for (k, v) in self.iter() {
            let ck = k.reflect_clone()?;
            let cv = v.reflect_clone()?;
            map.insert(
                ck.take::<K>().expect(CLONE_TYPE_ERROR),
                cv.take::<V>().expect(CLONE_TYPE_ERROR),
            );
        }
        Ok(Box::new(map))
    }

    #[inline]
    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        crate::impls::map_apply(self, value)
    }

    #[inline]
    fn reflect_eq(&self, value: &dyn Reflect) -> bool {
        crate::impls::map_eq(self, value)
    }

    #[inline]
    fn reflect_hash(&self) -> u64 {
        crate::impls::map_hash(self)
    }

    #[inline]
    fn reflect_debug(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        crate::impls::map_debug(self, f)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>> {
        let value = match value.downcast::<Self>() {
            Ok(ret) => return Ok(ret),
            Err(e) => e,
        };

        let ReflectRef::Map(v) = value.reflect_ref() else {
            return Err(value);
        };

        if v.iter_entries().any(|(k, v)| !k.is::<K>() || !v.is::<V>()) {
            return Err(value);
        }

        let mut map_v = value.reflect_owned().into_map().unwrap();

        let entries: Vec<(Box<dyn Reflect>, Box<dyn Reflect>)> = map_v.drain_all();

        let mut map = Self::with_capacity_and_hasher(entries.len(), S::default());
        for (k, v) in entries {
            let key = k.take::<K>().expect(COMPATIBLE_ERROR);
            let val = v.take::<V>().expect(COMPATIBLE_ERROR);
            map.insert(key, val);
        }
        Ok(Box::new(map))
    }
}

// -----------------------------------------------------------------------------
// HashMap — TypeDatabase
// -----------------------------------------------------------------------------

impl<K, V, S> TypeDatabase for HashMap<K, V, S>
where
    K: TypeDatabase + Eq + Hash,
    V: TypeDatabase,
    S: TypePath + BuildHasher + Default + Send + Sync,
{
    fn on_register(db: &'static TypeDB) {
        db.insert_defaultor::<Self>();
    }

    fn register_dependencies() {
        TypeDB::register::<K>();
        TypeDB::register::<V>();
    }
}
