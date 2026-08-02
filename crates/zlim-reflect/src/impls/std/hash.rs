use core::any::TypeId;
use core::hash::{BuildHasher, Hash};
use std::collections::{HashMap, HashSet};
use std::hash::RandomState;

use crate::Reflect;
use crate::db::{TypeDB, TypeDatabase};
use crate::impls::{CLONE_TYPE_ERROR, COMPATIBLE_ERROR, CONVERT_TYPE_ERROR, is_convertable};
use crate::info::{GenericInfo, Generics, InfoCell, MapInfo};
use crate::info::{SetInfo, TypeInfo, TypeParamInfo, Typed};
use crate::ops::{ApplyError, CloneError, Map, Set};
use crate::path::{PathCell, TypePath, concat};

// ----------------------------------------------------------------------------
// TypePath - RandomState
// ----------------------------------------------------------------------------

impl TypePath for RandomState {
    fn type_path() -> &'static str {
        "std::hash::RandomState"
    }
    fn type_name() -> &'static str {
        "RandomState"
    }
    const IDENT: &str = "RandomState";
    const CRATE: Option<&str> = Some("std");
    const MODULE: Option<&str> = Some("std::hash");
}

// ----------------------------------------------------------------------------
// TypePath - HashSet<K, S>
// ----------------------------------------------------------------------------

impl<K: TypePath, S: TypePath> TypePath for HashSet<K, S> {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&[
                "std::collections::HashSet",
                "<",
                K::type_path(),
                ", ",
                S::type_path(),
                ">",
            ])
        })
    }
    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&["HashSet", "<", K::type_name(), ", ", S::type_name(), ">"])
        })
    }
    const IDENT: &str = "HashSet";
    const CRATE: Option<&str> = Some("std");
    const MODULE: Option<&str> = Some("std::collections");
}

// ----------------------------------------------------------------------------
// TypePath - HashMap<K, V, S>
// ----------------------------------------------------------------------------

impl<K: TypePath, V: TypePath, S: TypePath> TypePath for HashMap<K, V, S> {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&[
                "std::collections::HashMap",
                "<",
                K::type_path(),
                ", ",
                V::type_path(),
                ", ",
                S::type_path(),
                ">",
            ])
        })
    }
    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&[
                "HashMap",
                "<",
                K::type_name(),
                ", ",
                V::type_name(),
                ", ",
                S::type_name(),
                ">",
            ])
        })
    }
    const IDENT: &str = "HashMap";
    const CRATE: Option<&str> = Some("std");
    const MODULE: Option<&str> = Some("std::collections");
}

// ----------------------------------------------------------------------------
// HashSet — Typed
// ----------------------------------------------------------------------------

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
                GenericInfo::Type(TypeParamInfo::new::<S>("S").with_default::<RandomState>()),
            ])))
        })
    }
}

// ----------------------------------------------------------------------------
// HashSet — Set
// ----------------------------------------------------------------------------

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

// ----------------------------------------------------------------------------
// HashSet — Reflect
// ----------------------------------------------------------------------------

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
        let mut value = match value.downcast::<Self>() {
            Ok(ret) => return Ok(ret),
            Err(e) => e,
        };
        if let Some(db) = value.type_db() {
            match db.convert(value, TypeId::of::<Self>()) {
                Ok(ret) => return Ok(ret.downcast::<Self>().expect(CONVERT_TYPE_ERROR)),
                Err(e) => value = e,
            }
        }
        if value.reflect_kind() != crate::info::ReflectKind::Set {
            return Err(value);
        }
        let mut set_v = value.reflect_owned().into_set().unwrap();
        if !set_v
            .iter_values()
            .all(|i| is_convertable(i, TypeId::of::<T>()))
        {
            return Err(set_v);
        }
        let items: Vec<Box<dyn Reflect>> = set_v.drain_all();
        let mut set = Self::with_capacity_and_hasher(items.len(), S::default());
        for item in items {
            set.insert(*T::from_reflect(item).expect(COMPATIBLE_ERROR));
        }
        Ok(Box::new(set))
    }
}

// ----------------------------------------------------------------------------
// HashSet — TypeDatabase
// ----------------------------------------------------------------------------

impl<T: TypeDatabase + Eq + Hash, S: TypePath + BuildHasher + Default + Send + Sync> TypeDatabase
    for HashSet<T, S>
{
    fn on_register(db: &'static TypeDB) {
        db.insert_defaultor(Self::default);
    }
    fn register_dependencies() {
        TypeDB::register::<T>();
    }
}

// ----------------------------------------------------------------------------
// HashMap — Typed
// ----------------------------------------------------------------------------

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
                GenericInfo::Type(TypeParamInfo::new::<S>("S").with_default::<RandomState>()),
            ])))
        })
    }
}

// ----------------------------------------------------------------------------
// HashMap — Map
// ----------------------------------------------------------------------------

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

// ----------------------------------------------------------------------------
// HashMap — Reflect
// ----------------------------------------------------------------------------

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
        let mut value = match value.downcast::<Self>() {
            Ok(ret) => return Ok(ret),
            Err(e) => e,
        };
        if let Some(db) = value.type_db() {
            match db.convert(value, TypeId::of::<Self>()) {
                Ok(ret) => return Ok(ret.downcast::<Self>().expect(CONVERT_TYPE_ERROR)),
                Err(e) => value = e,
            }
        }
        if value.reflect_kind() != crate::info::ReflectKind::Map {
            return Err(value);
        }
        let mut map_v = value.reflect_owned().into_map().unwrap();
        {
            let key_type_id = TypeId::of::<K>();
            let val_type_id = TypeId::of::<V>();
            let mut ok = true;
            for (k, v) in map_v.iter_entries() {
                if !is_convertable(k, key_type_id) || !is_convertable(v, val_type_id) {
                    ok = false;
                    break;
                }
            }
            if !ok {
                return Err(map_v);
            }
        }
        let entries: Vec<(Box<dyn Reflect>, Box<dyn Reflect>)> = map_v.drain_all();
        let mut map = Self::with_capacity_and_hasher(entries.len(), S::default());
        for (k, v) in entries {
            let key = *K::from_reflect(k).expect(COMPATIBLE_ERROR);
            let val = *V::from_reflect(v).expect(COMPATIBLE_ERROR);
            map.insert(key, val);
        }
        Ok(Box::new(map))
    }
}

// ----------------------------------------------------------------------------
// HashMap — TypeDatabase
// ----------------------------------------------------------------------------

impl<K, V, S> TypeDatabase for HashMap<K, V, S>
where
    K: TypeDatabase + Eq + Hash,
    V: TypeDatabase,
    S: TypePath + BuildHasher + Default + Send + Sync,
{
    fn on_register(db: &'static TypeDB) {
        db.insert_defaultor(Self::default);
    }
    fn register_dependencies() {
        TypeDB::register::<K>();
        TypeDB::register::<V>();
    }
}
