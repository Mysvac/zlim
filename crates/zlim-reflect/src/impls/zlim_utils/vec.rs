//! The `zlim-utils` vectors: their type paths, and the reflection of [`SmallVec`].
//!
//! `SmallVec` is a growable list, so it is reflected the way `Vec` is: as a [`List`] whose elements
//! are the reflected values it holds. `ArrayVec` is not reflected — it has a fixed capacity, so
//! pushing into it can fail in a way the `List` interface has no way to report — and is named only.

use zlim_utils::format_smol;
use zlim_utils::vec::{ArrayVec, SmallVec};

use crate::Reflect;
use crate::db::{TypeDB, TypeDatabase};
use crate::impls::{CLONE_TYPE_ERROR, COMPATIBLE_ERROR};
use crate::info::{ConstParam, ConstParamInfo, GenericInfo, Generics, InfoCell};
use crate::info::{ListInfo, TypeInfo, TypeParamInfo, Typed};
use crate::ops::{ApplyError, CloneError, List, ListItemIter, ReflectRef};
use crate::path::{PathCell, TypePath, concat};

// -----------------------------------------------------------------------------
// TypePath
// -----------------------------------------------------------------------------

impl<T: TypePath, const N: usize> TypePath for SmallVec<T, N> {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&[
                "zlim_utils::vec",
                "::",
                "SmallVec",
                "<",
                T::type_path(),
                ", ",
                &format_smol!("{N}"),
                ">",
            ])
        })
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&[
                "SmallVec",
                "<",
                T::type_name(),
                ", ",
                &format_smol!("{N}"),
                ">",
            ])
        })
    }

    const IDENT: &'static str = "SmallVec";
    const CRATE: Option<&'static str> = Some("zlim_utils");
    const MODULE: Option<&'static str> = Some("zlim_utils::vec");
}

impl<T: TypePath, const N: usize> TypePath for ArrayVec<T, N> {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&[
                "zlim_utils::vec",
                "::",
                "ArrayVec",
                "<",
                T::type_path(),
                ", ",
                &format_smol!("{N}"),
                ">",
            ])
        })
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| {
            concat(&[
                "ArrayVec",
                "<",
                T::type_name(),
                ", ",
                &format_smol!("{N}"),
                ">",
            ])
        })
    }

    const IDENT: &'static str = "ArrayVec";
    const CRATE: Option<&'static str> = Some("zlim_utils");
    const MODULE: Option<&'static str> = Some("zlim_utils::vec");
}

// -----------------------------------------------------------------------------
// SmallVec<T, N> — Typed
// -----------------------------------------------------------------------------

impl<T: Reflect + Typed, const N: usize> Typed for SmallVec<T, N> {
    fn type_info() -> &'static TypeInfo {
        static CELL: InfoCell = InfoCell::new();
        CELL.get_or_init::<Self>(|| {
            TypeInfo::List(ListInfo::new::<Self, T>().with_generics(Generics::new(&[
                GenericInfo::Type(TypeParamInfo::new::<T>("T")),
                GenericInfo::Const(
                    ConstParamInfo::new::<usize>("N").with_value(ConstParam::Usize(N)),
                ),
            ])))
        })
    }
}

// -----------------------------------------------------------------------------
// SmallVec<T, N> — List
// -----------------------------------------------------------------------------

impl<T: Reflect + Typed, const N: usize> List for SmallVec<T, N> {
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

// -----------------------------------------------------------------------------
// SmallVec<T, N> — Reflect
// -----------------------------------------------------------------------------

impl<T: Reflect + Typed, const N: usize> Reflect for SmallVec<T, N> {
    crate::impls::impl_reflect_kind!(List);

    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        let mut buf: SmallVec<T, N> = SmallVec::with_capacity(self.len());
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
        let value = match value.downcast::<Self>() {
            Ok(ret) => return Ok(ret),
            Err(e) => e,
        };

        let ReflectRef::List(v) = value.reflect_ref() else {
            return Err(value);
        };

        if v.iter_items().any(|item| !item.is::<T>()) {
            return Err(value);
        }

        let mut value = value.reflect_owned().into_list().unwrap();
        let items: Vec<Box<dyn Reflect>> = value.drain_all();

        let mut buf = SmallVec::with_capacity(items.len());
        for item in items {
            buf.push(item.take::<T>().expect(COMPATIBLE_ERROR));
        }

        Ok(Box::new(buf))
    }
}

// -----------------------------------------------------------------------------
// SmallVec<T, N> — TypeDatabase
// -----------------------------------------------------------------------------

impl<T: TypeDatabase, const N: usize> TypeDatabase for SmallVec<T, N> {
    fn on_register(db: &'static TypeDB) {
        db.insert_defaultor::<Self>();
    }

    fn register_dependencies() {
        TypeDB::register::<T>();
    }
}

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::info::Typed;
    use crate::ops::{List, Reflect};
    use crate::path::TypePath;
    use zlim_utils::vec::{ArrayVec, SmallVec};

    #[test]
    #[rustfmt::skip]
    fn small_vec() {
        assert_eq!(<SmallVec<u8, 4>>::type_path(), "zlim_utils::vec::SmallVec<u8, 4>");
        assert_eq!(<SmallVec<u8, 4>>::type_name(), "SmallVec<u8, 4>");
        assert_eq!(<SmallVec<u8, 4>>::IDENT, "SmallVec");
        assert_eq!(<SmallVec<u8, 4>>::CRATE, Some("zlim_utils"));
        assert_eq!(<SmallVec<u8, 4>>::MODULE, Some("zlim_utils::vec"));

        // Nested / differently sized instantiations are distinct paths.
        assert_eq!(<SmallVec<SmallVec<u8, 2>, 16>>::type_path(), "zlim_utils::vec::SmallVec<zlim_utils::vec::SmallVec<u8, 2>, 16>");
        assert_eq!(<SmallVec<SmallVec<u8, 2>, 16>>::type_name(), "SmallVec<SmallVec<u8, 2>, 16>");
    }

    #[test]
    #[rustfmt::skip]
    fn array_vec() {
        assert_eq!(<ArrayVec<u8, 8>>::type_path(), "zlim_utils::vec::ArrayVec<u8, 8>");
        assert_eq!(<ArrayVec<u8, 8>>::type_name(), "ArrayVec<u8, 8>");
        assert_eq!(<ArrayVec<u8, 8>>::IDENT, "ArrayVec");
        assert_eq!(<ArrayVec<u8, 8>>::CRATE, Some("zlim_utils"));
        assert_eq!(<ArrayVec<u8, 8>>::MODULE, Some("zlim_utils::vec"));
    }

    /// A `SmallVec` is a list: its length comes from the vector, and its items are the elements.
    #[test]
    fn small_vec_is_a_list() {
        let mut buf: SmallVec<i32, 4> = SmallVec::new();
        buf.push_back(Box::new(7i32)).unwrap();
        buf.push_back(Box::new(9i32)).unwrap();

        assert_eq!(buf.item_len(), 2);
        assert_eq!(buf.item(0).unwrap().downcast_ref::<i32>(), Some(&7));
        assert_eq!(buf.item(1).unwrap().downcast_ref::<i32>(), Some(&9));

        let popped = buf.pop_back().expect("the list is not empty");
        assert_eq!(popped.downcast_ref::<i32>(), Some(&9));
        assert_eq!(buf.item_len(), 1);
    }

    /// The reflected value round-trips through `from_reflect`, which is what makes it usable as a
    /// field of another reflected type.
    #[test]
    fn small_vec_reflects_itself() {
        let mut original: SmallVec<i32, 4> = SmallVec::new();
        original.push(1);
        original.push(2);

        let cloned = original.reflect_clone().expect("clone");
        let cloned = cloned
            .take::<SmallVec<i32, 4>>()
            .expect("the clone is the same type");
        assert!(original.reflect_eq(&cloned));

        let rebuilt = SmallVec::<i32, 4>::from_reflect(Box::new(original.clone()))
            .expect("a list of the same type converts");
        assert!(original.reflect_eq(&*rebuilt));
        assert_eq!(rebuilt.item_len(), 2);

        // The type itself is registered, with a default constructor.
        crate::db::TypeDB::collect();
        let info = <SmallVec<i32, 4> as Typed>::type_info();
        assert_eq!(info.type_path(), <SmallVec<i32, 4>>::type_path());
    }
}
