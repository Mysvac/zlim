use zlim_reflect::Reflect;
use zlim_reflect::info::ReflectKind;
use zlim_reflect::ops::{Struct, Tuple};

// ----------------------------------------------------------------------------
// Test types — plain (no Hash/Eq since f32 doesn't impl them)
// ----------------------------------------------------------------------------

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct Named {
    x: i32,
    y: f32,
}

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct TupleStruct(i32, f32);

#[derive(Reflect, Debug, PartialEq)]
struct Unit;

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct WithIgnore {
    x: i32,
    #[reflect(ignore)]
    _cache: String,
    y: f32,
}

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct WithIgnoreDefault {
    x: i32,
    #[reflect(ignore, default)]
    _cache: u64,
    y: f32,
}

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct WithDefault {
    x: i32,
    #[reflect(default)]
    y: f32,
}

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct AllDefault {
    #[reflect(default)]
    x: i32,
    #[reflect(default)]
    y: f32,
}

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct WithCloneField {
    #[reflect(clone)]
    x: i32,
    y: f32,
}

// No type-level `#[reflect(Clone)]` — tests field-level `#[reflect(clone)]`
// in field-by-field `reflect_clone`.
#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Debug, Default)]
struct FieldCloneOnly {
    #[reflect(clone)]
    a: i32,
    #[reflect(clone)]
    b: String,
}

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct Nested {
    inner: Named,
    label: &'static str,
}

// ----------------------------------------------------------------------------
// Tuples with ignore
// ----------------------------------------------------------------------------

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct TupleWithIgnore(#[reflect(ignore)] String, i32);

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct TupleWithIgnoreDefault(#[reflect(ignore, default)] u64, i32);

// ----------------------------------------------------------------------------
// Enum types (no Default derive — enums need #[default] on a variant)
// ----------------------------------------------------------------------------

#[derive(Reflect, Debug, PartialEq, Clone)]
#[reflect(Clone, Debug)]
enum SimpleEnum {
    A,
    B(i32),
    C { x: f32 },
}

#[derive(Reflect, Debug, PartialEq, Clone)]
#[reflect(Clone, Debug)]
enum EnumWithIgnore {
    A,
    B(#[reflect(ignore)] u8, i32),
    C {
        x: f32,
        #[reflect(ignore, default)]
        _cache: u64,
    },
}

// ----------------------------------------------------------------------------
// reflect_kind tests
// ----------------------------------------------------------------------------

#[test]
fn reflect_kind_struct() {
    assert_eq!(Named { x: 1, y: 2.0 }.reflect_kind(), ReflectKind::Struct);
    assert_eq!(Unit.reflect_kind(), ReflectKind::Opaque);
}

#[test]
fn reflect_kind_tuple() {
    assert_eq!(TupleStruct(1, 2.0).reflect_kind(), ReflectKind::Tuple);
}

#[test]
fn reflect_kind_enum() {
    assert_eq!(SimpleEnum::A.reflect_kind(), ReflectKind::Enum);
    assert_eq!(SimpleEnum::B(1).reflect_kind(), ReflectKind::Enum);
    assert_eq!(SimpleEnum::C { x: 1.0 }.reflect_kind(), ReflectKind::Enum);
}

// ----------------------------------------------------------------------------
// from_reflect: same type (Phase 1 downcast)
// ----------------------------------------------------------------------------

#[test]
fn from_reflect_same_type_named() {
    let val = Named { x: 42, y: 3.14 };
    let boxed: Box<dyn Reflect> = Box::new(val);
    let result = Named::from_reflect(boxed).unwrap();
    assert_eq!(*result, Named { x: 42, y: 3.14 });
}

#[test]
fn from_reflect_same_type_tuple() {
    let val = TupleStruct(7, 2.71);
    let boxed: Box<dyn Reflect> = Box::new(val);
    let result = TupleStruct::from_reflect(boxed).unwrap();
    assert_eq!(*result, TupleStruct(7, 2.71));
}

#[test]
fn from_reflect_same_type_unit() {
    let boxed: Box<dyn Reflect> = Box::new(Unit);
    let result = Unit::from_reflect(boxed).unwrap();
    assert_eq!(*result, Unit);
}

#[test]
fn from_reflect_same_type_enum() {
    let val = SimpleEnum::B(99);
    let boxed: Box<dyn Reflect> = Box::new(val);
    let result: Box<SimpleEnum> = SimpleEnum::from_reflect(boxed).unwrap();
    assert_eq!(*result, SimpleEnum::B(99));
}

// ----------------------------------------------------------------------------
// from_reflect: struct field matching
// ----------------------------------------------------------------------------

#[test]
fn from_reflect_struct_extra_fields_allowed() {
    let src = Named { x: 10, y: 20.0 };
    let boxed: Box<dyn Reflect> = Box::new(src);
    let result = WithDefault::from_reflect(boxed).unwrap();
    assert_eq!(*result, WithDefault { x: 10, y: 20.0 });
}

#[test]
fn from_reflect_struct_default_field_fills_missing() {
    let src = Named { x: 10, y: 0.0 };
    let boxed: Box<dyn Reflect> = Box::new(src);
    let result = AllDefault::from_reflect(boxed).unwrap();
    assert_eq!(*result, AllDefault { x: 10, y: 0.0 });
}

#[test]
fn from_reflect_struct_with_ignore_uses_default() {
    let src = Named { x: 5, y: 1.0 };
    let boxed: Box<dyn Reflect> = Box::new(src);
    let result = WithIgnoreDefault::from_reflect(boxed).unwrap();
    assert_eq!(result.x, 5);
    assert_eq!(result.y, 1.0);
    assert_eq!(result._cache, 0);
}

#[test]
fn from_reflect_struct_ignore_with_type_default() {
    // WithIgnore has `#[reflect(Default)]` at the type level,
    // so `from_reflect` constructs via `Default::default()` first,
    // then assigns the active fields. The ignored field `_cache`
    // silently falls back to its Default value.
    let src = Named { x: 1, y: 2.0 };
    let boxed: Box<dyn Reflect> = Box::new(src);
    let result = WithIgnore::from_reflect(boxed).unwrap();
    assert_eq!(result.x, 1);
    assert_eq!(result.y, 2.0);
    assert_eq!(result._cache, "");
}

// ----------------------------------------------------------------------------
// from_reflect: tuple
// ----------------------------------------------------------------------------

#[test]
fn from_reflect_tuple_with_ignore_default() {
    let val = TupleStruct(3, 4.0);
    let boxed: Box<dyn Reflect> = Box::new(val);
    let result = TupleWithIgnoreDefault::from_reflect(boxed).unwrap();
    assert_eq!(result.1, 3);
    assert_eq!(result.0, 0);
}

#[test]
fn from_reflect_tuple_with_ignore_rejected() {
    let val = TupleStruct(3, 4.0);
    let boxed: Box<dyn Reflect> = Box::new(val);
    let result = TupleWithIgnore::from_reflect(boxed);
    assert!(result.is_err(), "ignore without default should fail");
}

#[test]
fn from_reflect_tuple_wrong_kind_rejected() {
    let val = Named { x: 1, y: 2.0 };
    let boxed: Box<dyn Reflect> = Box::new(val);
    let result = TupleStruct::from_reflect(boxed);
    assert!(result.is_err(), "wrong ReflectKind should fail");
}

// ----------------------------------------------------------------------------
// from_reflect: enum
// ----------------------------------------------------------------------------

#[test]
fn from_reflect_enum_unit_variant() {
    let val = SimpleEnum::A;
    let boxed: Box<dyn Reflect> = Box::new(val);
    let result: Box<SimpleEnum> = SimpleEnum::from_reflect(boxed).unwrap();
    assert_eq!(*result, SimpleEnum::A);
}

#[test]
fn from_reflect_enum_tuple_variant() {
    let val = SimpleEnum::B(42);
    let boxed: Box<dyn Reflect> = Box::new(val);
    let result: Box<SimpleEnum> = SimpleEnum::from_reflect(boxed).unwrap();
    assert_eq!(*result, SimpleEnum::B(42));
}

#[test]
fn from_reflect_enum_struct_variant() {
    let val = SimpleEnum::C { x: 9.9 };
    let boxed: Box<dyn Reflect> = Box::new(val);
    let result: Box<SimpleEnum> = SimpleEnum::from_reflect(boxed).unwrap();
    match *result {
        SimpleEnum::C { x } => assert_eq!(x, 9.9),
        _ => panic!("wrong variant"),
    }
}

#[test]
fn from_reflect_enum_wrong_kind_rejected() {
    let src = TupleStruct(1, 2.0);
    let boxed: Box<dyn Reflect> = Box::new(src);
    let result: Result<Box<SimpleEnum>, _> = SimpleEnum::from_reflect(boxed);
    assert!(result.is_err(), "wrong ReflectKind should fail");
}

// ----------------------------------------------------------------------------
// Struct trait: field access
// ----------------------------------------------------------------------------

#[test]
fn struct_field_access() {
    let s = Named { x: 10, y: 20.0 };
    let s: &dyn Struct = &s;
    assert_eq!(s.field_len(), 2);
    assert_eq!(s.name_at(0), Some("x"));
    assert_eq!(s.name_at(1), Some("y"));
    assert!(s.field("x").is_some());
    assert!(s.field("z").is_none());
}

#[test]
fn struct_ignore_excludes_field() {
    let s = WithIgnore {
        x: 1,
        _cache: "hello".into(),
        y: 2.0,
    };
    let s: &dyn Struct = &s;
    assert_eq!(s.field_len(), 2);
    assert!(s.field("_cache").is_none());
    assert!(s.field("x").is_some());
    assert!(s.field("y").is_some());
}

#[test]
fn struct_unpack_respects_ignore() {
    use std::borrow::Cow;
    let s = WithIgnore {
        x: 5,
        _cache: "ignored".into(),
        y: 3.0,
    };
    let b: Box<dyn Struct> = Box::new(s);
    let items: Vec<(Cow<'static, str>, Box<dyn Reflect>)> = b.unpack();
    assert_eq!(items.len(), 2);
    let names: Vec<&str> = items.iter().map(|(n, _)| n.as_ref()).collect();
    assert_eq!(names, vec!["x", "y"]);
}

// ----------------------------------------------------------------------------
// Tuple trait: field access
// ----------------------------------------------------------------------------

#[test]
fn tuple_field_access() {
    let t = TupleStruct(7, 8.0);
    let t: &dyn Tuple = &t;
    assert_eq!(t.field_len(), 2);
    assert!(t.field(0).is_some());
    assert!(t.field(1).is_some());
    assert!(t.field(2).is_none());
}

#[test]
fn tuple_unpack_respects_ignore() {
    let t = TupleWithIgnore("ignored".into(), 99);
    let b: Box<dyn Tuple> = Box::new(t);
    let items: Vec<Box<dyn Reflect>> = b.unpack();
    assert_eq!(items.len(), 1);
    let val: &i32 = items[0].downcast_ref::<i32>().unwrap();
    assert_eq!(*val, 99);
}

// ----------------------------------------------------------------------------
// Edge cases: empty struct / tuple
// ----------------------------------------------------------------------------

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct EmptyStruct {}

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct EmptyTuple();

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct AllIgnored {
    #[reflect(ignore, default)]
    _a: i32,
    #[reflect(ignore, default)]
    _b: String,
}

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct AllIgnoredTuple(
    #[reflect(ignore, default)] i32,
    #[reflect(ignore, default)] String,
);

#[test]
fn empty_struct_reflect_kind() {
    assert_eq!(EmptyStruct {}.reflect_kind(), ReflectKind::Struct);
}

#[test]
fn empty_struct_field_len() {
    let s = EmptyStruct {};
    let s: &dyn Struct = &s;
    assert_eq!(s.field_len(), 0);
}

#[test]
fn empty_struct_from_reflect() {
    let val = EmptyStruct {};
    let boxed: Box<dyn Reflect> = Box::new(val);
    let result = EmptyStruct::from_reflect(boxed).unwrap();
    assert_eq!(*result, EmptyStruct {});
}

#[test]
fn empty_struct_unpack() {
    let b: Box<dyn Struct> = Box::new(EmptyStruct {});
    let items = b.unpack();
    assert!(items.is_empty());
}

#[test]
fn empty_tuple_reflect_kind() {
    assert_eq!(EmptyTuple().reflect_kind(), ReflectKind::Tuple);
}

#[test]
fn empty_tuple_field_len() {
    let t = EmptyTuple();
    let t: &dyn Tuple = &t;
    assert_eq!(t.field_len(), 0);
}

#[test]
fn empty_tuple_from_reflect() {
    let val = EmptyTuple();
    let boxed: Box<dyn Reflect> = Box::new(val);
    let result = EmptyTuple::from_reflect(boxed).unwrap();
    assert_eq!(*result, EmptyTuple());
}

#[test]
fn empty_tuple_unpack() {
    let b: Box<dyn Tuple> = Box::new(EmptyTuple());
    let items = b.unpack();
    assert!(items.is_empty());
}

#[test]
fn all_ignored_struct_is_empty_struct() {
    // Struct with all fields ignored → 0 active fields, still Struct.
    let s = AllIgnored {
        _a: 1,
        _b: "x".into(),
    };
    assert_eq!(s.reflect_kind(), ReflectKind::Struct);
    let s: &dyn Struct = &s;
    assert_eq!(s.field_len(), 0);
}

#[test]
fn all_ignored_struct_from_reflect_cross_type() {
    // Use different-type source to hit Phase 4–5 (Phase 1 downcast preserves originals).
    // AllIgnored has 0 active fields; all ignore+default fields get Default.
    let src = Named { x: 1, y: 2.0 };
    let boxed: Box<dyn Reflect> = Box::new(src);
    let result = AllIgnored::from_reflect(boxed).unwrap();
    assert_eq!(result._a, 0);
    assert_eq!(result._b, "");
}

#[test]
fn all_ignored_tuple_is_empty_tuple() {
    let t = AllIgnoredTuple(1, "x".into());
    assert_eq!(t.reflect_kind(), ReflectKind::Tuple);
    let t: &dyn Tuple = &t;
    assert_eq!(t.field_len(), 0);
}

#[test]
fn all_ignored_tuple_from_reflect_cross_type() {
    // EmptyTuple has 0 active fields; AllIgnoredTuple also has 0 (all ignore+default).
    let src = EmptyTuple();
    let boxed: Box<dyn Reflect> = Box::new(src);
    let result = AllIgnoredTuple::from_reflect(boxed).unwrap();
    assert_eq!(result.0, 0);
    assert_eq!(result.1, "");
}

#[test]
fn all_ignored_tuple_from_reflect_same_type() {
    // Same-type from_reflect preserves original values (Phase 1 downcast).
    let val = AllIgnoredTuple(42, "hello".into());
    let boxed: Box<dyn Reflect> = Box::new(val);
    let result = AllIgnoredTuple::from_reflect(boxed).unwrap();
    assert_eq!(result.0, 42);
    assert_eq!(result.1, "hello");
}

#[test]
fn field_clone_reflect_clone_works() {
    // `FieldCloneOnly` has no type-level `#[reflect(Clone)]`, so
    // `reflect_clone` is field-by-field with `#[reflect(clone)]` fast paths.
    let val = FieldCloneOnly {
        a: 10,
        b: "hi".into(),
    };
    let cloned: Box<FieldCloneOnly> = val
        .reflect_clone()
        .unwrap()
        .downcast::<FieldCloneOnly>()
        .unwrap();
    assert_eq!(cloned.a, 10);
    assert_eq!(cloned.b, "hi");
}

#[test]
fn cross_struct_extra_fields_in_source() {
    // Named (x:i32, y:f32) → WithDefault (x:i32, y:f32 with #[reflect(default)])
    // The source has y, so it should be used.
    let src = Named { x: 100, y: 200.0 };
    let boxed: Box<dyn Reflect> = Box::new(src);
    let result = WithDefault::from_reflect(boxed).unwrap();
    assert_eq!(result.x, 100);
    assert_eq!(result.y, 200.0);
}

#[test]
fn cross_tuple_same_shape() {
    // TupleStruct(3, 4.0) → different tuple type, same field types
    let src = TupleStruct(42, 3.14);
    let boxed: Box<dyn Reflect> = Box::new(src);
    let result = TupleStruct::from_reflect(boxed).unwrap();
    assert_eq!(*result, TupleStruct(42, 3.14));
}

#[test]
fn with_clone_field_from_reflect() {
    let val = WithCloneField { x: 7, y: 8.0 };
    let boxed: Box<dyn Reflect> = Box::new(val);
    let result = WithCloneField::from_reflect(boxed).unwrap();
    assert_eq!(result.x, 7);
    assert_eq!(result.y, 8.0);
}

#[test]
fn enum_ignore_tuple_variant_from_reflect() {
    // `B(#[reflect(ignore)] u8, i32)` — first field is ignored, second is active.
    let val = EnumWithIgnore::B(99, 42);
    let boxed: Box<dyn Reflect> = Box::new(val);
    let result: Box<EnumWithIgnore> = EnumWithIgnore::from_reflect(boxed).unwrap();
    match *result {
        EnumWithIgnore::B(_, y) => assert_eq!(y, 42),
        _ => panic!("wrong variant"),
    }
}

#[test]
fn enum_variant_with_ignore_default_constructs() {
    let val = EnumWithIgnore::C { x: 1.0, _cache: 0 };
    let boxed: Box<dyn Reflect> = Box::new(val);
    let result: Box<EnumWithIgnore> = EnumWithIgnore::from_reflect(boxed).unwrap();
    match *result {
        EnumWithIgnore::C { x, _cache } => {
            assert_eq!(x, 1.0);
            assert_eq!(_cache, 0);
        }
        _ => panic!("wrong variant"),
    }
}
