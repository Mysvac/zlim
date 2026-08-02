use core::any::TypeId;

use crate::db::{TypeDB, TypeDatabase};
use crate::impls::{CLONE_TYPE_ERROR, COMPATIBLE_ERROR, CONVERT_TYPE_ERROR, is_convertable};
use crate::info::{GenericInfo, Generics, InfoCell, ListInfo, TypeInfo, TypeParamInfo, Typed};
use crate::ops::{ApplyError, CloneError, List, ListItemIter, ReflectRef};
use crate::path::{PathCell, concat};
use crate::{Reflect, TypePath};

impl<T: TypePath> TypePath for Vec<T> {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["alloc::vec::Vec", "<", <T>::type_path(), ">"]))
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["Vec", "<", <T>::type_name(), ">"]))
    }

    const IDENT: &str = "Vec";
    const CRATE: Option<&str> = Some("alloc");
    const MODULE: Option<&str> = Some("alloc::vec");
}

impl<T: Reflect + Typed> Typed for Vec<T> {
    fn type_info() -> &'static TypeInfo {
        static CELL: InfoCell = InfoCell::new();
        CELL.get_or_init::<Self>(|| {
            TypeInfo::List(ListInfo::new::<Self, T>().with_generics(Generics::new(&[
                GenericInfo::Type(TypeParamInfo::new::<T>("T")),
            ])))
        })
    }
}

impl<T: Reflect + Typed> List for Vec<T> {
    fn item(&self, index: usize) -> Option<&dyn Reflect> {
        self.get(index).map(|x| x as &dyn Reflect)
    }

    fn item_mut(&mut self, index: usize) -> Option<&mut dyn Reflect> {
        self.get_mut(index).map(|x| x as &mut dyn Reflect)
    }

    fn item_len(&self) -> usize {
        self.len()
    }

    fn iter_items(&self) -> ListItemIter<'_> {
        ListItemIter::new(self)
    }

    fn push_back(&mut self, value: Box<dyn Reflect>) -> Result<(), Box<dyn Reflect>> {
        let value = T::from_reflect(value)?;
        self.push(*value);
        Ok(())
    }

    fn push_front(&mut self, value: Box<dyn Reflect>) -> Result<(), Box<dyn Reflect>> {
        let value = T::from_reflect(value)?;
        self.insert(0, *value);
        Ok(())
    }

    fn pop_back(&mut self) -> Option<Box<dyn Reflect>> {
        self.pop().map(|x| Box::new(x) as Box<dyn Reflect>)
    }

    fn pop_front(&mut self) -> Option<Box<dyn Reflect>> {
        if self.is_empty() {
            None
        } else {
            Some(Box::new(self.remove(0)))
        }
    }

    fn drain_all(&mut self) -> Vec<Box<dyn Reflect>> {
        self.drain(..)
            .map(|x| Box::new(x) as Box<dyn Reflect>)
            .collect()
    }
}

impl<T: Reflect + Typed> Reflect for Vec<T> {
    crate::impls::impl_reflect_kind!(List);

    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        let mut buf: Vec<T> = Vec::with_capacity(self.len());
        for item in self.iter() {
            let it = item.reflect_clone()?;
            buf.push(it.take::<T>().expect(CLONE_TYPE_ERROR));
        }
        Ok(Box::new(buf))
    }

    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        crate::impls::list_apply(self, value)
    }

    fn reflect_debug(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        crate::impls::list_debug(self, f)
    }

    fn reflect_eq(&self, other: &dyn Reflect) -> bool {
        crate::impls::list_eq(self, other)
    }

    fn reflect_hash(&self) -> u64 {
        crate::impls::list_hash(self)
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

        let ReflectRef::List(v) = value.reflect_ref() else {
            return Err(value);
        };

        if !v
            .iter_items()
            .all(|item| is_convertable(item, TypeId::of::<T>()))
        {
            return Err(value);
        }

        let mut value = value.reflect_owned().into_list().unwrap();
        let items: Vec<Box<dyn Reflect>> = value.drain_all();

        let mut buf = Self::with_capacity(items.len());
        for item in items {
            buf.push(*T::from_reflect(item).expect(COMPATIBLE_ERROR));
        }

        Ok(Box::new(buf))
    }
}

impl<T: TypeDatabase> TypeDatabase for Vec<T> {
    fn on_register(db: &'static TypeDB) {
        db.insert_defaultor(Self::default);
    }

    fn register_dependencies() {
        TypeDB::register::<T>();
    }
}
