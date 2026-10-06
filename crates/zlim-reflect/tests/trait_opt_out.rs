//! `#[reflect(Trait = false)]` opts out of a generated implementation. The
//! macro then emits nothing for that trait, so the user is expected to write
//! their own — these tests provide one and check the macro left the slot free.

use core::any::TypeId;
use std::borrow::Cow;

use zlim_reflect::db::TypeDB;
use zlim_reflect::info::{OpaqueInfo, ReflectKind, TypeInfo, Typed, VariantKind};
use zlim_reflect::ops::{ApplyError, CloneError, Enum, Opaque, ReflectMut};
use zlim_reflect::ops::{ReflectOwned, ReflectRef, Struct, Tuple};
use zlim_reflect::{Reflect, TypePath};

// -----------------------------------------------------------------------------
// Struct = false

#[derive(Reflect, TypePath, Debug)]
#[reflect(Reflect = false, Struct = false)]
struct SkippedStruct {
    x: i32,
}

impl Struct for SkippedStruct {
    fn field(&self, name: &str) -> Option<&dyn Reflect> {
        if name == "x" { Some(&self.x) } else { None }
    }

    fn field_mut(&mut self, name: &str) -> Option<&mut dyn Reflect> {
        if name == "x" { Some(&mut self.x) } else { None }
    }

    fn field_at(&self, index: usize) -> Option<&dyn Reflect> {
        if index == 0 { Some(&self.x) } else { None }
    }

    fn field_at_mut(&mut self, index: usize) -> Option<&mut dyn Reflect> {
        if index == 0 { Some(&mut self.x) } else { None }
    }

    fn name_at(&self, index: usize) -> Option<&str> {
        (index == 0).then_some("x")
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        (name == "x").then_some(0)
    }

    fn field_len(&self) -> usize {
        1
    }

    fn iter_fields(&self) -> zlim_reflect::ops::StructFieldIter<'_> {
        zlim_reflect::ops::StructFieldIter::new(self)
    }

    fn unpack(self: Box<Self>) -> Vec<(Cow<'static, str>, Box<dyn Reflect>)> {
        vec![(Cow::Borrowed("x"), Box::new(self.x))]
    }
}

impl Reflect for SkippedStruct {
    fn reflect_kind(&self) -> ReflectKind {
        ReflectKind::Struct
    }

    fn reflect_ref(&self) -> ReflectRef<'_> {
        ReflectRef::Struct(self)
    }

    fn reflect_mut(&mut self) -> ReflectMut<'_> {
        ReflectMut::Struct(self)
    }

    fn reflect_owned(self: Box<Self>) -> ReflectOwned {
        ReflectOwned::Struct(self)
    }

    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        Ok(Box::new(SkippedStruct { x: self.x }))
    }

    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        zlim_reflect::impls::struct_apply(self, value)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>> {
        match value.downcast::<Self>() {
            Ok(v) => Ok(v),
            Err(_) => Err(Box::new(SkippedStruct { x: 0 })),
        }
    }
}

#[test]
fn struct_opt_out_leaves_the_impl_to_the_user() {
    let value = SkippedStruct { x: 7 };
    let as_struct: &dyn Struct = &value;

    assert_eq!(as_struct.field_len(), 1);
    assert!(as_struct.field("x").is_some());
    // Both impls are the hand-written ones, and they agree on the kind.
    assert!(matches!(
        value.reflect_ref().as_struct(),
        Ok(s) if s.field_len() == 1
    ));
}

// -----------------------------------------------------------------------------
// Opaque is not an opt-out
//
// `#[reflect(Opaque)]` is the type-level "treat this as opaque" flag, and it
// already leaves the `Opaque` impl to the user. A `= false` spelling of the same
// name would be a second way to say it, so there is none.

#[derive(Reflect, TypePath, Debug)]
#[reflect(Opaque)]
struct ManualOpaque(String);

impl Opaque for ManualOpaque {
    fn apply_str(&mut self, v: &str) -> Result<(), String> {
        self.0 = v.to_owned();
        Ok(())
    }

    fn stringify(&self) -> String {
        self.0.clone()
    }
}

#[test]
fn opaque_impl_is_the_users() {
    let mut value = ManualOpaque(String::from("before"));

    assert_eq!(Opaque::stringify(&value), "before");
    assert!(value.reflect_ref().as_opaque().is_ok());

    value.apply_str("after").unwrap();
    assert_eq!(value.0, "after");
}

// -----------------------------------------------------------------------------
// Tuple = false

#[derive(Reflect, TypePath, Debug)]
#[reflect(Reflect = false, Tuple = false)]
struct SkippedTuple(i32);

impl Tuple for SkippedTuple {
    fn field(&self, index: usize) -> Option<&dyn Reflect> {
        if index == 0 { Some(&self.0) } else { None }
    }

    fn field_mut(&mut self, index: usize) -> Option<&mut dyn Reflect> {
        if index == 0 { Some(&mut self.0) } else { None }
    }

    fn field_len(&self) -> usize {
        1
    }

    fn iter_fields(&self) -> zlim_reflect::ops::TupleFieldIter<'_> {
        zlim_reflect::ops::TupleFieldIter::new(self)
    }

    fn unpack(self: Box<Self>) -> Vec<Box<dyn Reflect>> {
        vec![Box::new(self.0)]
    }
}

#[test]
fn tuple_opt_out_leaves_the_impl_to_the_user() {
    let value = SkippedTuple(3);

    assert!(matches!(
        value.reflect_ref().as_tuple(),
        Ok(t) if t.field_len() == 1
    ));
}

impl Reflect for SkippedTuple {
    fn reflect_kind(&self) -> ReflectKind {
        ReflectKind::Tuple
    }

    fn reflect_ref(&self) -> ReflectRef<'_> {
        ReflectRef::Tuple(self)
    }

    fn reflect_mut(&mut self) -> ReflectMut<'_> {
        ReflectMut::Tuple(self)
    }

    fn reflect_owned(self: Box<Self>) -> ReflectOwned {
        ReflectOwned::Tuple(self)
    }

    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        Ok(Box::new(SkippedTuple(self.0)))
    }

    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        zlim_reflect::impls::tuple_apply(self, value)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>> {
        match value.downcast::<Self>() {
            Ok(v) => Ok(v),
            Err(_) => Err(Box::new(SkippedTuple(0))),
        }
    }
}

// -----------------------------------------------------------------------------
// Enum = false

#[derive(Reflect, TypePath, Debug)]
#[reflect(Reflect = false, Enum = false)]
enum SkippedEnum {
    A,
}

impl Enum for SkippedEnum {
    fn field(&self, _: &str) -> Option<&dyn Reflect> {
        None
    }

    fn field_at(&self, _: usize) -> Option<&dyn Reflect> {
        None
    }

    fn field_mut(&mut self, _: &str) -> Option<&mut dyn Reflect> {
        None
    }

    fn field_at_mut(&mut self, _: usize) -> Option<&mut dyn Reflect> {
        None
    }

    fn field_name_at(&self, _: usize) -> Option<&str> {
        None
    }

    fn field_index_of(&self, _: &str) -> Option<usize> {
        None
    }

    fn field_len(&self) -> usize {
        0
    }

    fn iter_fields(&self) -> zlim_reflect::ops::VariantFieldIter<'_> {
        zlim_reflect::ops::VariantFieldIter::new(self)
    }

    fn variant_kind(&self) -> VariantKind {
        VariantKind::Unit
    }

    fn variant_index(&self) -> usize {
        0
    }

    fn variant_name(&self) -> &str {
        "A"
    }

    fn unpack(self: Box<Self>) -> Vec<(Option<Cow<'static, str>>, Box<dyn Reflect>)> {
        Vec::new()
    }
}

#[test]
fn enum_opt_out_leaves_the_impl_to_the_user() {
    let value = SkippedEnum::A;

    assert_eq!(value.reflect_ref().as_enum().unwrap().variant_name(), "A");
}

impl Reflect for SkippedEnum {
    fn reflect_kind(&self) -> ReflectKind {
        ReflectKind::Enum
    }

    fn reflect_ref(&self) -> ReflectRef<'_> {
        ReflectRef::Enum(self)
    }

    fn reflect_mut(&mut self) -> ReflectMut<'_> {
        ReflectMut::Enum(self)
    }

    fn reflect_owned(self: Box<Self>) -> ReflectOwned {
        ReflectOwned::Enum(self)
    }

    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        Ok(Box::new(SkippedEnum::A))
    }

    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        // A single unit variant never switches, so a variant-name mismatch is
        // the only thing `enum_try_apply` can hand back here.
        match zlim_reflect::impls::enum_try_apply(self, value)? {
            Ok(()) => Ok(()),
            Err(_) => Err(ApplyError {
                src: <Self as TypePath>::type_path(),
                apply: value.reflect_type_path(),
                error: String::from("`SkippedEnum` has a single variant"),
            }),
        }
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>> {
        match value.downcast::<Self>() {
            Ok(v) => Ok(v),
            Err(_) => Err(Box::new(SkippedEnum::A)),
        }
    }
}

// -----------------------------------------------------------------------------
// Reflect = false
//
// A hand-written `Reflect` has to dispatch to some kind trait; here that is the
// generated `Struct`, since a named struct already gets one.

#[derive(Reflect, TypePath, Debug, Default)]
#[reflect(Reflect = false)]
struct WrittenByHand {
    x: i32,
}

impl Reflect for WrittenByHand {
    fn reflect_kind(&self) -> ReflectKind {
        ReflectKind::Struct
    }

    fn reflect_ref(&self) -> ReflectRef<'_> {
        ReflectRef::Struct(self)
    }

    fn reflect_mut(&mut self) -> ReflectMut<'_> {
        ReflectMut::Struct(self)
    }

    fn reflect_owned(self: Box<Self>) -> ReflectOwned {
        ReflectOwned::Struct(self)
    }

    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, CloneError> {
        Ok(Box::new(WrittenByHand { x: self.x }))
    }

    fn reflect_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
        zlim_reflect::impls::struct_apply(self, value)
    }

    fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<Self>, Box<dyn Reflect>> {
        match value.downcast::<Self>() {
            Ok(v) => Ok(v),
            Err(_) => Err(Box::new(WrittenByHand::default())),
        }
    }
}

#[test]
fn reflect_opt_out_leaves_the_impl_to_the_user() {
    let mut value = WrittenByHand { x: 1 };

    // These all run the hand-written implementation, which delegates to the
    // generated `Struct`.
    assert_eq!(value.reflect_kind(), ReflectKind::Struct);
    assert_eq!(value.reflect_ref().as_struct().unwrap().field_len(), 1);
    assert!(
        value
            .reflect_mut()
            .as_struct()
            .unwrap()
            .field("x")
            .is_some()
    );
    assert!(value.reflect_clone().unwrap().is::<WrittenByHand>());
    value.reflect_apply(&WrittenByHand { x: 9 }).unwrap();
    assert_eq!(value.x, 9);
}

// -----------------------------------------------------------------------------
// Typed = false

#[derive(Reflect, TypePath, Debug)]
#[reflect(Typed = false)]
struct SkippedTyped;

impl Typed for SkippedTyped {
    fn type_info() -> &'static TypeInfo {
        static INFO: TypeInfo = TypeInfo::Opaque(OpaqueInfo::new::<SkippedTyped>());
        &INFO
    }
}

#[test]
fn typed_opt_out_leaves_the_impl_to_the_user() {
    // The macro emitted no `Typed`, so this is the hand-written one.
    assert_eq!(
        SkippedTyped::type_info().type_path(),
        "trait_opt_out::SkippedTyped"
    );

    TypeDB::collect();
    assert!(TypeDB::get_by_type(TypeId::of::<SkippedTyped>()).is_some());
}

// -----------------------------------------------------------------------------
// TypeDatabase = false

#[derive(Reflect, TypePath, Debug)]
#[reflect(TypeDatabase = false)]
struct SkippedTypeDatabase;

/// The type still reflects; only the database entry is gone.
#[derive(Reflect, TypePath, Debug)]
#[reflect(TypeDatabase = false)]
struct SkippedTypeDatabaseHolder {
    value: i32,
}

#[test]
fn type_database_opt_out_leaves_the_registration_out() {
    TypeDB::collect();

    // The opt-out drops the `register_reflect!` submission as well, so the type is not in the
    // linker section `collect` walks. Its absence is therefore structural: no number of further
    // collections can register it, which is why this is safe to assert under parallel tests.
    let id = TypeId::of::<SkippedTypeDatabase>();
    assert!(TypeDB::get_by_type(id).is_none());
    assert!(TypeDB::get_by_path(<SkippedTypeDatabase as TypePath>::type_path()).is_none());

    // The reflection impls are untouched, so the value still works.
    let holder = SkippedTypeDatabaseHolder { value: 5 };
    assert_eq!(holder.reflect_kind(), ReflectKind::Struct);
    assert!(TypeDB::get_by_type(TypeId::of::<SkippedTypeDatabaseHolder>()).is_none());
}
