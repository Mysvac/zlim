use core::any::TypeId;
use std::collections::{BTreeMap, BTreeSet};

use crate::Reflect;
use crate::TypePath;
use crate::db::{TypeDB, TypeDatabase};
use crate::impls::{CLONE_TYPE_ERROR, COMPATIBLE_ERROR, CONVERT_TYPE_ERROR, is_convertable};
use crate::info::{
    GenericInfo, Generics, InfoCell, MapInfo, SetInfo, TypeInfo, TypeParamInfo, Typed,
};
use crate::ops::{ApplyError, CloneError, Map, Set};
use crate::path::{PathCell, concat};

// ----------------------------------------------------------------------------
// BTreeSet<T>
// ----------------------------------------------------------------------------

impl<T: TypePath> TypePath for BTreeSet<T> {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&[
                "alloc::collections::btree::set::BTreeSet",
                "<",
                <T>::type_path(),
                ">",
            ])
        })
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["BTreeSet", "<", <T>::type_name(), ">"]))
    }

    const IDENT: &str = "BTreeSet";
    const CRATE: Option<&str> = Some("alloc");
    const MODULE: Option<&str> = Some("alloc::collections::btree::set");
}

impl<T: Reflect + Typed + Ord> Typed for BTreeSet<T> {
    fn type_info() -> &'static TypeInfo {
        static CELL: InfoCell = InfoCell::new();
        CELL.get_or_init::<Self>(|| {
            TypeInfo::Set(SetInfo::new::<Self, T>().with_generics(Generics::new(&[
                GenericInfo::Type(TypeParamInfo::new::<T>("T")),
            ])))
        })
    }
}

impl<T: Reflect + Typed + Ord> Set for BTreeSet<T> {
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

impl<T: Reflect + Typed + Ord> Reflect for BTreeSet<T> {
    crate::impls::impl_reflect_kind!(Set);

    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        let mut set: BTreeSet<T> = BTreeSet::new();
        for item in self.iter() {
            let it = item.reflect_clone()?;
            set.insert(it.take::<T>().expect(CLONE_TYPE_ERROR));
        }
        Ok(Box::new(set))
    }

    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        crate::impls::set_apply(self, value)
    }

    fn reflect_debug(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        crate::impls::set_debug(self, f)
    }

    fn reflect_eq(&self, other: &dyn Reflect) -> bool {
        crate::impls::set_eq(self, other)
    }

    fn reflect_hash(&self) -> u64 {
        crate::impls::set_hash(self)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>> {
        let mut value = match value.downcast::<Self>() {
            Ok(ret) => return Ok(ret),
            Err(e) => e,
        };

        if let Some(db) = value.type_db() {
            match db.convert(value, TypeId::of::<Self>()) {
                Ok(ret) => {
                    let r = ret.downcast::<Self>().expect(CONVERT_TYPE_ERROR);
                    return Ok(r);
                }
                Err(e) => value = e,
            }
        }

        if value.reflect_kind() != crate::info::ReflectKind::Set {
            return Err(value);
        }

        let mut set_v = value.reflect_owned().into_set().unwrap();

        if !set_v
            .iter_values()
            .all(|item| is_convertable(item, TypeId::of::<T>()))
        {
            return Err(set_v);
        }

        let items: Vec<Box<dyn Reflect>> = set_v.drain_all();

        let mut set = Self::new();
        for item in items {
            set.insert(*T::from_reflect(item).expect(COMPATIBLE_ERROR));
        }

        Ok(Box::new(set))
    }
}

impl<T: TypeDatabase + Ord> TypeDatabase for BTreeSet<T> {
    fn on_register(db: &'static TypeDB) {
        db.insert_defaultor(Self::default);
    }

    fn register_dependencies() {
        TypeDB::register::<T>();
    }
}

// ----------------------------------------------------------------------------
// BTreeMap<K, V>
// ----------------------------------------------------------------------------

impl<K: TypePath, V: TypePath> TypePath for BTreeMap<K, V> {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&[
                "alloc::collections::btree::map::BTreeMap",
                "<",
                <K>::type_path(),
                ", ",
                <V>::type_path(),
                ">",
            ])
        })
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&[
                "BTreeMap",
                "<",
                <K>::type_name(),
                ", ",
                <V>::type_name(),
                ">",
            ])
        })
    }

    const IDENT: &str = "BTreeMap";
    const CRATE: Option<&str> = Some("alloc");
    const MODULE: Option<&str> = Some("alloc::collections::btree::map");
}

impl<K: Reflect + Typed + Ord, V: Reflect + Typed> Typed for BTreeMap<K, V> {
    fn type_info() -> &'static TypeInfo {
        static CELL: InfoCell = InfoCell::new();
        CELL.get_or_init::<Self>(|| {
            TypeInfo::Map(MapInfo::new::<Self, K, V>().with_generics(Generics::new(&[
                GenericInfo::Type(TypeParamInfo::new::<K>("K")),
                GenericInfo::Type(TypeParamInfo::new::<V>("V")),
            ])))
        })
    }
}

impl<K: Reflect + Typed + Ord, V: Reflect + Typed> Map for BTreeMap<K, V> {
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
        let key = match K::from_reflect(key) {
            Ok(k) => k,
            Err(e) => return Err((e, value)),
        };
        let value = match V::from_reflect(value) {
            Ok(v) => v,
            Err(e) => return Err((Box::new(*key), e)),
        };
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

impl<K: Reflect + Typed + Ord, V: Reflect + Typed> Reflect for BTreeMap<K, V> {
    crate::impls::impl_reflect_kind!(Map);

    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        let mut map: BTreeMap<K, V> = BTreeMap::new();
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

    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        crate::impls::map_apply(self, value)
    }

    fn reflect_debug(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        crate::impls::map_debug(self, f)
    }

    fn reflect_eq(&self, other: &dyn Reflect) -> bool {
        crate::impls::map_eq(self, other)
    }

    fn reflect_hash(&self) -> u64 {
        crate::impls::map_hash(self)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>> {
        let mut value = match value.downcast::<Self>() {
            Ok(ret) => return Ok(ret),
            Err(e) => e,
        };

        if let Some(db) = value.type_db() {
            match db.convert(value, TypeId::of::<Self>()) {
                Ok(ret) => {
                    let r = ret.downcast::<Self>().expect(CONVERT_TYPE_ERROR);
                    return Ok(r);
                }
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

        let mut map = Self::new();
        for (k, v) in entries {
            let key = *K::from_reflect(k).expect(COMPATIBLE_ERROR);
            let val = *V::from_reflect(v).expect(COMPATIBLE_ERROR);
            map.insert(key, val);
        }

        Ok(Box::new(map))
    }
}

impl<K: TypeDatabase + Ord, V: TypeDatabase> TypeDatabase for BTreeMap<K, V> {
    fn on_register(db: &'static TypeDB) {
        db.insert_defaultor(Self::default);
    }

    fn register_dependencies() {
        TypeDB::register::<K>();
        TypeDB::register::<V>();
    }
}
