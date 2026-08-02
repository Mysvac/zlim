use core::any::TypeId;
use core::fmt::{Debug, Formatter};

use crate::Reflect;
use crate::db::{TypeDB, TypeDatabase};
use crate::impls::{CLONE_TYPE_ERROR, COMPATIBLE_ERROR, CONVERT_TYPE_ERROR, UNPACK_ERROR};
use crate::info::{InfoCell, ReflectKind, TupleInfo, TypeInfo, Typed, UnnamedField};
use crate::ops::{ApplyError, CloneError, Tuple, TupleFieldIter};
use crate::path::{PathCell, TypePath, concat};

// ----------------------------------------------------------------------------
// Unit Tuple
// ----------------------------------------------------------------------------

impl TypePath for () {
    #[inline]
    fn type_path() -> &'static str {
        "()"
    }

    #[inline]
    fn type_name() -> &'static str {
        "()"
    }

    const IDENT: &str = "()";
    const CRATE: Option<&str> = None;
    const MODULE: Option<&str> = None;
}

impl Typed for () {
    #[inline]
    fn type_info() -> &'static TypeInfo {
        static INFO: TypeInfo = TypeInfo::Tuple(TupleInfo::UNIT);
        &INFO
    }
}

impl Tuple for () {
    #[inline]
    fn field(&self, _: usize) -> Option<&dyn Reflect> {
        None
    }

    #[inline]
    fn field_mut(&mut self, _: usize) -> Option<&mut dyn Reflect> {
        None
    }

    #[inline]
    fn field_len(&self) -> usize {
        0
    }

    #[inline]
    fn iter_fields(&self) -> TupleFieldIter<'_> {
        TupleFieldIter::new(self)
    }

    #[inline]
    fn unpack(self: Box<Self>) -> Vec<Box<dyn Reflect>> {
        Vec::new()
    }
}

impl Reflect for () {
    crate::impls::impl_reflect_kind!(Tuple);

    #[inline]
    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        Ok(Box::new(()))
    }

    #[inline]
    fn reflect_hash(&self) -> u64 {
        0
    }

    #[inline]
    fn reflect_eq(&self, other: &dyn Reflect) -> bool {
        other.type_id() == TypeId::of::<Self>()
    }

    #[inline]
    fn reflect_debug(&self, f: &mut Formatter) -> core::fmt::Result {
        Debug::fmt(&(), f)
    }

    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        if value.type_id() == TypeId::of::<Self>() {
            return Ok(());
        }

        if let Some(db) = TypeDB::get_by_type(value.type_id())
            && db.contains_convertor(TypeId::of::<Self>())
            && let Ok(cloned) = value.reflect_clone()
            // The convertor may have side effects and needs to be executed.
            && let Ok(_) = db.convert(cloned, TypeId::of::<Self>())
        {
            return Ok(());
        }

        // Phase 3: cast `other` to `&dyn Tuple`.
        let other: &dyn Tuple = value.reflect_ref().as_tuple().map_err(|e| {
            ::core::hint::cold_path();
            let apply = value.reflect_type_path();
            ApplyError::mismatched_kind("()", apply, e.expected, e.received)
        })?;

        if other.field_len() != 0 {
            ::core::hint::cold_path();
            let apply = other.reflect_type_path();
            return Err(ApplyError::mismatched_size(
                "()",
                apply,
                0,
                other.field_len(),
            ));
        }

        Ok(())
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>> {
        if value.type_id() == TypeId::of::<Self>() {
            return Ok(Box::new(()));
        }

        let mut value = value;

        if let Some(db) = TypeDB::get_by_type(value.type_id()) {
            match db.convert(value, TypeId::of::<Self>()) {
                Ok(_x) => return Ok(Box::new(())),
                Err(e) => value = e,
            }
        }

        if value.reflect_kind() != ReflectKind::Tuple {
            return Err(value);
        }

        let value = value.reflect_owned().into_tuple().unwrap();

        if value.field_len() == 0 {
            Ok(Box::new(()))
        } else {
            Err(value)
        }
    }
}

impl TypeDatabase for () {
    fn on_register(db: &'static TypeDB) {
        db.insert_defaultor(Self::default);
        db.insert_serializer::<Self>();
        db.insert_deserializer::<Self>();
    }

    fn register_dependencies() {}
}

crate::register!(()); // Register TypeDB

// ----------------------------------------------------------------------------
// Tuple - 1~12, TypePath
// ----------------------------------------------------------------------------

macro_rules! to_erased_type {
    ($_:ident) => {
        ", _"
    };
}

macro_rules! impl_tuple_type_path {
    (0: []) => {};
    (1: [ $index:tt : $name:ident ]) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 items long.")]
        impl<$name: TypePath> TypePath for ($name,) {
            fn type_path() -> &'static str {
                static CELL: PathCell = PathCell::new();
                CELL.get_or_init::<Self>(|| concat(&["(" , <$name>::type_path() , ",)"]))
            }

            fn type_name() -> &'static str {
                static CELL: PathCell = PathCell::new();
                CELL.get_or_init::<Self>(|| concat(&["(" , <$name>::type_name() , ",)"]))
            }

            const IDENT: &str = "(_,)";
            const CRATE: Option<&str> = None;
            const MODULE: Option<&str> = None;
        }
    };
    ($_:literal: [$zero_index:tt : $zero_name:ident , $($index:tt : $name:ident),*]) => {
        #[cfg_attr(docsrs, doc(hidden))]
        impl<$zero_name: TypePath, $($name: TypePath),*> TypePath for ($zero_name, $($name),*) {
            fn type_path() -> &'static str {
                static CELL: PathCell = PathCell::new();
                CELL.get_or_init::<Self>(|| {
                    concat(&["(", <$zero_name>::type_path() $(, ", ", <$name>::type_path())* , ")"])
                })
            }

            fn type_name() -> &'static str {
                static CELL: PathCell = PathCell::new();
                CELL.get_or_init::<Self>(|| {
                    concat(&["(", <$zero_name>::type_name() $(, ", ", <$name>::type_name())* , ")"])
                })
            }

            const IDENT: &str = concat!( "(_", $( to_erased_type!{ $name } , )* ")" );
            const CRATE: Option<&str> = None;
            const MODULE: Option<&str> = None;
        }
    };
}

zlim_utils::range_invoke!(impl_tuple_type_path, 12);

// ----------------------------------------------------------------------------
// Tuple - 1~12, Typed & Tuple & Reflect
// ----------------------------------------------------------------------------

macro_rules! fake_or_hidden {
    (1 => @ $t:item @) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(
            docsrs,
            doc = "This trait is implemented for tuples up to 12 items long."
        )]
        $t
    };
    ($_:literal => @ $t:item @) => {
        #[cfg_attr(docsrs, doc(hidden))]
        $t
    };
}

macro_rules! impl_tuple_reflect {
    (0: []) => {};
    ($len:literal: [$($index:tt : $name:ident),*]) => {
        fake_or_hidden!{ $len => @
            impl<$( $name: Reflect + Typed ),*> Typed for ( $( $name , )* ) {
                fn type_info() -> &'static TypeInfo {
                    static CELL: InfoCell = InfoCell::new();
                    CELL.get_or_init::<Self>(|| {
                        TypeInfo::Tuple(TupleInfo::new::<Self>(&[
                            $( UnnamedField::new::<$name>($index), )*
                        ]))
                    })
                }
            }
        @ }


        fake_or_hidden!{ $len => @
            impl<$( $name: Reflect + Typed ),*> Tuple for ( $( $name , )* ) {
                fn field(&self, index: usize) -> Option<&dyn Reflect> {
                    match index {
                        $( $index => Some(&self.$index), )*
                        _ => None
                    }
                }

                fn field_mut(&mut self, index: usize) -> Option<&mut dyn Reflect> {
                    match index {
                        $( $index => Some(&mut self.$index), )*
                        _ => None
                    }
                }

                #[inline]
                fn field_len(&self) -> usize {
                    $len
                }

                #[inline]
                fn iter_fields(&self) -> TupleFieldIter<'_> {
                    TupleFieldIter::new(self)
                }

                fn unpack(self: Box<Self>) -> Vec<Box<dyn Reflect>> {
                    vec![ $(  Box::new(self.$index) ,)* ]
                }
            }
        @ }

        fake_or_hidden!{ $len => @
            impl<$( $name: Reflect + Typed ),*> Reflect for ( $( $name , )* ) {
                crate::impls::impl_reflect_kind!(Tuple);

                fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
                    Ok(Box::new((
                        $( self.$index.reflect_clone()?.take::<$name>().expect(CLONE_TYPE_ERROR), )*
                    )))
                }

                #[inline]
                fn reflect_hash(&self) -> u64 {
                    crate::impls::tuple_hash(self)
                }

                #[inline]
                fn reflect_eq(&self, value: &dyn Reflect) -> bool {
                    crate::impls::tuple_eq(self, value)
                }

                #[inline]
                fn reflect_debug(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
                    crate::impls::tuple_debug(self, f)
                }

                #[inline]
                fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
                    crate::impls::tuple_apply(self, value)
                }

                fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>>
                where
                    Self: Sized
                {
                    let mut value = match value.downcast::<Self>() {
                        Ok(ret) => return Ok(ret),
                        Err(e) => e,
                    };

                    if let Some(db) = TypeDB::get_by_type((&*value).type_id()) {
                        match db.convert(value, TypeId::of::<Self>()) {
                            Ok(ret) => {
                                let r = ret.downcast::<Self>().expect(CONVERT_TYPE_ERROR);
                                return Ok(r);
                            },
                            Err(e) => value = e,
                        }
                    }

                    if value.reflect_kind() != ReflectKind::Tuple {
                        return Err(value);
                    }

                    let value: Box<dyn Tuple> = value.reflect_owned().into_tuple().unwrap();
                    if value.field_len() != $len {
                        return Err(value);
                    }

                    $({
                        let field =  value.field($index).expect("valid index");
                        if !crate::impls::is_convertable(field, TypeId::of::<$name>()) {
                            return Err(value);
                        }
                    })*

                    let items: Vec<Box<dyn Reflect>> = value.unpack();

                    #[expect(clippy::allow_attributes, reason = "simplify implementation")]
                    #[allow(non_snake_case, reason = "macro generated")]
                    let [
                        $( $name, )*
                    ]: [Box<dyn Reflect>; $len] = items.try_into().expect(UNPACK_ERROR);

                    Ok(Box::new((
                        $( *<$name>::from_reflect( $name ).expect(COMPATIBLE_ERROR), )*
                    )))
                }
            }
        @ }

        fake_or_hidden!{ $len => @
            impl<$( $name: TypeDatabase ),*> TypeDatabase for ( $( $name , )* ) {
                fn on_register(_: &'static TypeDB) {}

                fn register_dependencies() {
                    $( TypeDB::register::<$name>(); )*
                }
            }
        @ }
    };
}

zlim_utils::range_invoke!(impl_tuple_reflect, 12);

// ----------------------------------------------------------------------------
// Tests
// ----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::db::TypeDB;
    use crate::info::Typed;
    use crate::path::TypePath;
    use core::any::TypeId;

    macro_rules! assert_path {
        (
            $t:ty,
            $a:expr,
            $b:expr,
            $c:expr,
            $d:expr,
            $e:expr,
        ) => {
            assert_eq!(<$t>::type_path(), $a);
            assert_eq!(<$t>::type_name(), $b);
            assert_eq!(<$t>::IDENT, $c);
            assert_eq!(<$t>::CRATE, $d);
            assert_eq!(<$t>::MODULE, $e);
        };
    }

    #[test]
    fn tuple_path() {
        assert_path! {
            (), "()", "()", "()", None, None,
        }

        assert_path! {
            (u8,),
            "(u8,)",
            "(u8,)",
            "(_,)",
            None,
            None,
        }

        assert_path! {
            (u8, (u8,), i32),
            "(u8, (u8,), i32)",
            "(u8, (u8,), i32)",
            "(_, _, _)",
            None,
            None,
        }
    }

    #[test]
    fn tuple_info() {
        let info = <()>::type_info().as_tuple().unwrap();
        assert_eq!(info.field_len(), 0);
        assert_eq!(info.fields().len(), 0);
        assert_eq!(info.type_path(), <()>::type_path());

        let info = <(i32,)>::type_info().as_tuple().unwrap();
        assert_eq!(info.field_len(), 1);
        assert_eq!(info.fields().len(), 1);
        assert_eq!(info.type_path(), <(i32,)>::type_path());
        assert_eq!(info.field(0).unwrap().type_id(), TypeId::of::<i32>());
        assert_eq!(
            info.field(0).unwrap().type_info().type_path(),
            <i32>::type_path()
        );

        let info = <(i32, (i32, u8), f32)>::type_info().as_tuple().unwrap();
        assert_eq!(info.field_len(), 3);
        assert_eq!(info.fields().len(), 3);
        assert_eq!(info.type_path(), <(i32, (i32, u8), f32)>::type_path());
        assert_eq!(info.field(1).unwrap().type_id(), TypeId::of::<(i32, u8)>());
        assert_eq!(
            info.field(2).unwrap().type_info().type_name(),
            <f32>::type_name()
        );
    }

    #[test]
    fn registered() {
        TypeDB::collect();
        assert!(TypeDB::get_by_type(TypeId::of::<()>()).is_some());
        assert!(TypeDB::get_by_path("()").is_some());
    }
}
