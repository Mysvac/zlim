use core::any::TypeId;

use zlim_utils::format_smol;

use crate::Reflect;
use crate::db::{TypeDB, TypeDatabase};
use crate::impls::{CLONE_TYPE_ERROR, COMPATIBLE_ERROR};
use crate::impls::{CONVERT_TYPE_ERROR, UNPACK_ERROR, is_convertable};
use crate::info::{ArrayInfo, InfoCell, ReflectKind, TypeInfo, Typed};
use crate::ops::{Array, CloneError};
use crate::path::{PathCell, TypePath, concat};

// ----------------------------------------------------------------------------
// TypePath for `[T]`
// ----------------------------------------------------------------------------

impl<T: TypePath> TypePath for [T] {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["[", <T>::type_path(), "]"]))
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["[", <T>::type_name(), "]"]))
    }

    const IDENT: &str = "[_]";
    const CRATE: Option<&str> = None;
    const MODULE: Option<&str> = None;
}

// ----------------------------------------------------------------------------
// TypePath for `[T; N]`
// ----------------------------------------------------------------------------

impl<T: TypePath, const N: usize> TypePath for [T; N] {
    fn type_path() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["[", T::type_path(), "; ", &format_smol!("{N}"), "]"]))
    }

    fn type_name() -> &'static str {
        static CELL: PathCell = PathCell::new();
        CELL.get_or_init::<Self>(|| concat(&["[", T::type_name(), "; ", &format_smol!("{N}"), "]"]))
    }

    const IDENT: &str = "[_; _]";
    const CRATE: Option<&str> = None;
    const MODULE: Option<&str> = None;
}

// ----------------------------------------------------------------------------
// Typed & Reflect & Array
// ----------------------------------------------------------------------------

impl<T: Reflect + Typed, const N: usize> Typed for [T; N] {
    fn type_info() -> &'static TypeInfo {
        static CELL: InfoCell = InfoCell::new();
        CELL.get_or_init::<Self>(|| TypeInfo::Array(ArrayInfo::new::<Self, T>(N)))
    }
}

impl<T: Reflect + Typed, const N: usize> Array for [T; N] {
    fn item(&self, index: usize) -> Option<&dyn Reflect> {
        self.get(index).map(|x| x as &dyn Reflect)
    }

    fn item_mut(&mut self, index: usize) -> Option<&mut dyn Reflect> {
        self.get_mut(index).map(|x| x as &mut dyn Reflect)
    }

    fn item_len(&self) -> usize {
        N
    }

    fn iter_items(&self) -> crate::ops::ArrayItemIter<'_> {
        crate::ops::ArrayItemIter::new(self)
    }

    fn unpack(self: Box<Self>) -> Vec<Box<dyn Reflect>> {
        let mut buf: Vec<Box<dyn Reflect>> = Vec::with_capacity(N);
        for item in self as Box<[T]> {
            buf.push(Box::new(item));
        }
        buf
    }
}

impl<T: Reflect + Typed, const N: usize> Reflect for [T; N] {
    crate::impls::impl_reflect_kind!(Array);

    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        let mut buf: Vec<T> = Vec::with_capacity(N);
        for item in self {
            let x = item.reflect_clone()?;
            buf.push(x.take::<T>().expect(CLONE_TYPE_ERROR));
        }

        match TryInto::<Box<Self>>::try_into(buf) {
            Ok(res) => Ok(res),
            Err(_) => unreachable!(), // buf.len() == N
        }
    }

    #[inline]
    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), crate::ops::ApplyError> {
        crate::impls::array_apply(self, value)
    }

    #[inline]
    fn reflect_hash(&self) -> u64 {
        crate::impls::array_hash(self)
    }

    #[inline]
    fn reflect_eq(&self, value: &dyn Reflect) -> bool {
        crate::impls::array_eq(self, value)
    }

    #[inline]
    fn reflect_debug(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        crate::impls::array_debug(self, f)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>>
    where
        Self: Sized,
    {
        let mut value = match value.downcast::<Self>() {
            Ok(ret) => return Ok(ret),
            Err(e) => e,
        };

        if let Some(db) = TypeDB::get_by_type((*value).type_id()) {
            match db.convert(value, TypeId::of::<Self>()) {
                Ok(ret) => {
                    let r = ret.downcast::<Self>().expect(CONVERT_TYPE_ERROR);
                    return Ok(r);
                }
                Err(e) => value = e,
            }
        }

        if value.reflect_kind() != ReflectKind::Array {
            return Err(value);
        }

        let value: Box<dyn Array> = value.reflect_owned().into_array().unwrap();

        if value.item_len() != N {
            return Err(value);
        }

        for item in value.iter_items() {
            if !is_convertable(item, TypeId::of::<T>()) {
                return Err(value);
            }
        }

        let items: Vec<Box<dyn Reflect>> = value.unpack();
        assert_eq!(items.len(), N, "{}", UNPACK_ERROR);

        let mut values: Vec<T> = Vec::with_capacity(N);
        for item in items {
            values.push(*T::from_reflect(item).expect(COMPATIBLE_ERROR));
        }

        match TryInto::<Box<Self>>::try_into(values) {
            Ok(res) => Ok(res),
            Err(_) => unreachable!(), // buf.len() == N
        }
    }
}

// ----------------------------------------------------------------------------
// TypeDB
// ----------------------------------------------------------------------------

impl<T: TypeDatabase, const N: usize> TypeDatabase for [T; N] {
    fn on_register(_: &'static TypeDB) {}

    fn register_dependencies() {
        TypeDB::register::<T>();
    }
}

// Generic type cannot be registered by `register!` macro.
