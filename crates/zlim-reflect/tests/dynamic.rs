use zlim_reflect::Reflect;
use zlim_reflect::dynamic::{
    DynamicArray, DynamicEnum, DynamicList, DynamicMap, DynamicSet, DynamicStruct, DynamicTuple,
    DynamicVariant,
};
use zlim_reflect::info::ReflectKind;
use zlim_reflect::ops::{Array, Enum, List, Map, Set, Struct, Tuple};

// ----------------------------------------------------------------------------
// Test types
// ----------------------------------------------------------------------------

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct Point {
    x: i32,
    y: f32,
}

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct Person {
    name: String,
    age: i32,
}

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct Vec3(i32, i32, i32);

#[derive(Reflect, Debug, PartialEq, Clone, Default)]
#[reflect(Clone, Debug, Default)]
struct Pair(i32, f32);

#[derive(Reflect, Debug, PartialEq, Clone)]
#[reflect(Clone, Debug)]
enum MyOption {
    None,
    Some(i32),
    Labeled { value: f32, tag: String },
}

// ----------------------------------------------------------------------------
// DynamicStruct
// ----------------------------------------------------------------------------

#[test]
fn dynamic_struct_new_empty() {
    let s = DynamicStruct::new();
    assert_eq!(s.field_len(), 0);
    assert!(s.field("anything").is_none());
}

#[test]
fn dynamic_struct_push_and_access() {
    let mut s = DynamicStruct::new();
    s.push("x".into(), Box::new(42i32));
    s.push("name".into(), Box::new("hello"));

    assert_eq!(s.field_len(), 2);
    assert_eq!(s.index_of("x"), Some(0));
    assert_eq!(s.index_of("name"), Some(1));

    let x: &i32 = s.field("x").unwrap().downcast_ref().unwrap();
    assert_eq!(*x, 42);

    let name: &str = s.field("name").unwrap().downcast_ref::<&str>().unwrap();
    assert_eq!(name, "hello");
}

#[test]
fn dynamic_struct_insert_replace() {
    let mut s = DynamicStruct::new();
    s.push("a".into(), Box::new(1i32));
    s.insert("a".into(), Box::new(99i32)); // replace

    assert_eq!(s.field_len(), 1);
    let a: &i32 = s.field("a").unwrap().downcast_ref().unwrap();
    assert_eq!(*a, 99);
}

#[test]
fn dynamic_struct_insert_new() {
    let mut s = DynamicStruct::new();
    s.insert("a".into(), Box::new(1i32));
    s.insert("b".into(), Box::new(2i32));

    assert_eq!(s.field_len(), 2);
}

#[test]
fn dynamic_struct_from_ref_point() {
    let pt = Point { x: 10, y: 3.14 };
    let pt_ref: &dyn Struct = &pt;
    let dyn_s = DynamicStruct::from_ref(pt_ref).unwrap();

    assert_eq!(dyn_s.field_len(), 2);
    assert_eq!(dyn_s.name_at(0), Some("x"));
    assert_eq!(dyn_s.name_at(1), Some("y"));

    let x: &i32 = dyn_s.field("x").unwrap().downcast_ref().unwrap();
    assert_eq!(*x, 10);
    let y: &f32 = dyn_s.field("y").unwrap().downcast_ref().unwrap();
    assert_eq!(*y, 3.14);
}

#[test]
fn dynamic_struct_from_reflect_to_point() {
    let mut dyn_s = DynamicStruct::new();
    dyn_s.push("x".into(), Box::new(7i32));
    dyn_s.push("y".into(), Box::new(2.71f32));

    let boxed: Box<dyn Reflect> = Box::new(dyn_s);
    let pt: Box<Point> = Point::from_reflect(boxed).unwrap();

    assert_eq!(*pt, Point { x: 7, y: 2.71 });
}

#[test]
fn dynamic_struct_roundtrip_via_dynamic() {
    // Concrete → DynamicStruct → concrete
    let original = Point { x: 5, y: 1.5 };
    let s: &dyn Struct = &original;
    let dyn_s = DynamicStruct::from_ref(s).unwrap();

    let boxed: Box<dyn Reflect> = Box::new(dyn_s);
    let restored: Box<Point> = Point::from_reflect(boxed).unwrap();

    assert_eq!(*restored, original);
}

#[test]
fn dynamic_struct_extra_field_in_dynamic_is_ok() {
    // DynamicStruct with extra field → Point (lenient conversion).
    let mut dyn_s = DynamicStruct::new();
    dyn_s.push("x".into(), Box::new(1i32));
    dyn_s.push("y".into(), Box::new(2.0f32));
    dyn_s.push("z".into(), Box::new(999i32)); // extra field — ignored

    let boxed: Box<dyn Reflect> = Box::new(dyn_s);
    let pt: Box<Point> = Point::from_reflect(boxed).unwrap();

    assert_eq!(pt.x, 1);
    assert_eq!(pt.y, 2.0);
}

#[test]
fn dynamic_struct_missing_non_default_field_fails() {
    // Missing required field 'y' → from_reflect should fail.
    let mut dyn_s = DynamicStruct::new();
    dyn_s.push("x".into(), Box::new(1i32));
    // 'y' is missing and has no #[reflect(default)]

    let boxed: Box<dyn Reflect> = Box::new(dyn_s);
    let result = Point::from_reflect(boxed);
    assert!(result.is_err(), "missing non-default field should fail");
}

#[test]
fn dynamic_struct_try_clone() {
    let mut s = DynamicStruct::new();
    s.push("a".into(), Box::new(100i32));
    s.push("b".into(), Box::new(200i32));

    let cloned = s.try_clone().unwrap();
    assert_eq!(cloned.field_len(), 2);

    let a: &i32 = cloned.field("a").unwrap().downcast_ref().unwrap();
    assert_eq!(*a, 100);
}

// ----------------------------------------------------------------------------
// DynamicTuple
// ----------------------------------------------------------------------------

#[test]
fn dynamic_tuple_new_empty() {
    let t = DynamicTuple::new();
    assert_eq!(t.field_len(), 0);
    assert!(t.field(0).is_none());
}

#[test]
fn dynamic_tuple_push_and_access() {
    let mut t = DynamicTuple::new();
    t.push(Box::new(10i32));
    t.push(Box::new(3.14f32));
    t.push(Box::new("text"));

    assert_eq!(t.field_len(), 3);

    let v0: &i32 = t.field(0).unwrap().downcast_ref().unwrap();
    assert_eq!(*v0, 10);
    let v1: &f32 = t.field(1).unwrap().downcast_ref().unwrap();
    assert_eq!(*v1, 3.14);
    let v2: &str = t.field(2).unwrap().downcast_ref::<&str>().unwrap();
    assert_eq!(v2, "text");
}

#[test]
fn dynamic_tuple_from_ref_pair() {
    let pair = Pair(7, 2.71);
    let pair_ref: &dyn Tuple = &pair;
    let dyn_t = DynamicTuple::from_ref(pair_ref).unwrap();

    assert_eq!(dyn_t.field_len(), 2);
    let v0: &i32 = dyn_t.field(0).unwrap().downcast_ref().unwrap();
    assert_eq!(*v0, 7);
    let v1: &f32 = dyn_t.field(1).unwrap().downcast_ref().unwrap();
    assert_eq!(*v1, 2.71);
}

#[test]
fn dynamic_tuple_from_reflect_to_pair() {
    let mut dyn_t = DynamicTuple::new();
    dyn_t.push(Box::new(42i32));
    dyn_t.push(Box::new(1.5f32));

    let boxed: Box<dyn Reflect> = Box::new(dyn_t);
    let pair: Box<Pair> = Pair::from_reflect(boxed).unwrap();

    assert_eq!(*pair, Pair(42, 1.5));
}

#[test]
fn dynamic_tuple_roundtrip_via_dynamic() {
    let original = Vec3(1, 2, 3);
    let t: &dyn Tuple = &original;
    let dyn_t = DynamicTuple::from_ref(t).unwrap();

    let boxed: Box<dyn Reflect> = Box::new(dyn_t);
    let restored: Box<Vec3> = Vec3::from_reflect(boxed).unwrap();

    assert_eq!(*restored, original);
}

#[test]
fn dynamic_tuple_wrong_length_fails() {
    // Vec3 expects 3 fields, DynamicTuple has only 2.
    let mut dyn_t = DynamicTuple::new();
    dyn_t.push(Box::new(1i32));
    dyn_t.push(Box::new(2i32));

    let boxed: Box<dyn Reflect> = Box::new(dyn_t);
    let result = Vec3::from_reflect(boxed);
    assert!(result.is_err(), "wrong length should fail (strict)");
}

#[test]
fn dynamic_tuple_try_clone() {
    let mut t = DynamicTuple::new();
    t.push(Box::new(1i32));
    t.push(Box::new(2i32));

    let cloned = t.try_clone().unwrap();
    assert_eq!(cloned.field_len(), 2);
}

// ----------------------------------------------------------------------------
// DynamicArray
// ----------------------------------------------------------------------------

#[test]
fn dynamic_array_push_and_access() {
    let mut a = DynamicArray::new();
    a.push(Box::new(10i32));
    a.push(Box::new(20i32));
    a.push(Box::new(30i32));

    assert_eq!(a.item_len(), 3);
    let v: &i32 = a.item(1).unwrap().downcast_ref().unwrap();
    assert_eq!(*v, 20);
}

#[test]
fn dynamic_array_from_reflect() {
    let mut a = DynamicArray::with_capacity(2);
    a.push(Box::new(100i32));
    a.push(Box::new(200i32));

    let boxed: Box<dyn Reflect> = Box::new(a);
    let result = DynamicArray::from_reflect(boxed).unwrap();

    assert_eq!(result.item_len(), 2);
}

#[test]
fn dynamic_array_wrong_kind_rejected() {
    let val = 42i32; // Opaque, not Array
    let boxed: Box<dyn Reflect> = Box::new(val);
    let result = DynamicArray::from_reflect(boxed);
    assert!(result.is_err());
}

// ----------------------------------------------------------------------------
// DynamicList
// ----------------------------------------------------------------------------

#[test]
fn dynamic_list_push_pop() {
    let mut list = DynamicList::new();
    list.push_back(Box::new(1i32)).unwrap();
    list.push_back(Box::new(2i32)).unwrap();
    list.push_back(Box::new(3i32)).unwrap();

    assert_eq!(list.item_len(), 3);

    let popped = list.pop_back().unwrap();
    let v: &i32 = popped.downcast_ref().unwrap();
    assert_eq!(*v, 3);
    assert_eq!(list.item_len(), 2);
}

#[test]
fn dynamic_list_push_front() {
    let mut list = DynamicList::new();
    list.push_back(Box::new(2i32)).unwrap();
    list.push_front(Box::new(1i32)).unwrap();

    let v: &i32 = list.item(0).unwrap().downcast_ref().unwrap();
    assert_eq!(*v, 1);
}

#[test]
fn dynamic_list_from_reflect() {
    let mut list = DynamicList::new();
    list.push_back(Box::new(10i32)).unwrap();
    list.push_back(Box::new(20i32)).unwrap();

    let boxed: Box<dyn Reflect> = Box::new(list);
    let result = DynamicList::from_reflect(boxed).unwrap();

    assert_eq!(result.item_len(), 2);
    let v: &i32 = result.item(0).unwrap().downcast_ref().unwrap();
    assert_eq!(*v, 10);
}

#[test]
fn dynamic_list_drain_all() {
    let mut list = DynamicList::new();
    list.push_back(Box::new(1i32)).unwrap();
    list.push_back(Box::new(2i32)).unwrap();

    let drained = list.drain_all();
    assert_eq!(drained.len(), 2);
    assert_eq!(list.item_len(), 0);
}

// ----------------------------------------------------------------------------
// DynamicMap
// ----------------------------------------------------------------------------

#[test]
fn dynamic_map_insert_and_lookup() {
    let mut map = DynamicMap::new();
    map.insert(Box::new("name"), Box::new("Alice"));
    map.insert(Box::new("age"), Box::new(30i32));

    assert_eq!(map.entry_len(), 2);

    let name: &str = map
        .value(&*Box::new("name"))
        .unwrap()
        .downcast_ref::<&str>()
        .unwrap();
    assert_eq!(name, "Alice");

    let age: &i32 = map
        .value(&*Box::new("age"))
        .unwrap()
        .downcast_ref()
        .unwrap();
    assert_eq!(*age, 30);
}

#[test]
fn dynamic_map_insert_replace() {
    let mut map = DynamicMap::new();
    map.insert(Box::new("key"), Box::new(1i32));
    let old = map.insert(Box::new("key"), Box::new(99i32));

    assert!(old.is_some());
    assert_eq!(map.entry_len(), 1);

    let v: &i32 = map
        .value(&*Box::new("key"))
        .unwrap()
        .downcast_ref()
        .unwrap();
    assert_eq!(*v, 99);
}

#[test]
fn dynamic_map_from_reflect() {
    let mut map = DynamicMap::new();
    map.insert(Box::new("a"), Box::new(1i32));
    map.insert(Box::new("b"), Box::new(2i32));

    let boxed: Box<dyn Reflect> = Box::new(map);
    let result = DynamicMap::from_reflect(boxed).unwrap();

    assert_eq!(result.entry_len(), 2);
}

#[test]
fn dynamic_map_try_clone() {
    let mut map = DynamicMap::new();
    map.insert(Box::new("x"), Box::new(42i32));

    let cloned = map.try_clone().unwrap();
    assert_eq!(cloned.entry_len(), 1);
    let v: &i32 = cloned
        .value(&*Box::new("x"))
        .unwrap()
        .downcast_ref()
        .unwrap();
    assert_eq!(*v, 42);
}

// ----------------------------------------------------------------------------
// DynamicSet
// ----------------------------------------------------------------------------

#[test]
fn dynamic_set_insert_unique() {
    let mut set = DynamicSet::new();
    assert!(set.insert(Box::new(1i32)));
    assert!(set.insert(Box::new(2i32)));
    assert!(set.insert(Box::new(3i32)));
    assert_eq!(set.value_len(), 3);
}

#[test]
fn dynamic_set_insert_duplicate() {
    let mut set = DynamicSet::new();
    assert!(set.insert(Box::new(1i32)));
    assert!(!set.insert(Box::new(1i32))); // duplicate
    assert_eq!(set.value_len(), 1);
}

#[test]
fn dynamic_set_lookup_and_remove() {
    let mut set = DynamicSet::new();
    set.insert(Box::new(10i32));
    set.insert(Box::new(20i32));

    assert!(set.value(&*Box::new(10i32)).is_some());
    assert!(set.value(&*Box::new(999i32)).is_none());

    assert!(set.remove_value(&*Box::new(10i32)));
    assert_eq!(set.value_len(), 1);
}

#[test]
fn dynamic_set_from_reflect() {
    let mut set = DynamicSet::new();
    set.insert(Box::new(1i32));
    set.insert(Box::new(2i32));

    let boxed: Box<dyn Reflect> = Box::new(set);
    let result = DynamicSet::from_reflect(boxed).unwrap();

    assert_eq!(result.value_len(), 2);
}

#[test]
fn dynamic_set_try_clone() {
    let mut set = DynamicSet::new();
    set.insert(Box::new(42i32));

    let cloned = set.try_clone().unwrap();
    assert_eq!(cloned.value_len(), 1);
    assert!(cloned.value(&*Box::new(42i32)).is_some());
}

// ----------------------------------------------------------------------------
// DynamicEnum
// ----------------------------------------------------------------------------

#[test]
fn dynamic_enum_unit_variant() {
    let e = DynamicEnum::new(0, "None", DynamicVariant::Unit);

    assert_eq!(e.variant_name(), "None");
    assert_eq!(e.variant_index(), 0);
    assert_eq!(e.field_len(), 0);
    assert!(matches!(e.variant(), DynamicVariant::Unit));
}

#[test]
fn dynamic_enum_tuple_variant() {
    let mut t = DynamicTuple::new();
    t.push(Box::new(42i32));

    let e = DynamicEnum::new(1, "Some", DynamicVariant::Tuple(t));

    assert_eq!(e.variant_name(), "Some");
    assert_eq!(e.field_len(), 1);

    let f: &i32 = e.field_at(0).unwrap().downcast_ref().unwrap();
    assert_eq!(*f, 42);
}

#[test]
fn dynamic_enum_struct_variant() {
    let mut s = DynamicStruct::new();
    s.push("value".into(), Box::new(3.14f32));
    s.push("tag".into(), Box::new("pi"));

    let e = DynamicEnum::new(0, "Labeled", DynamicVariant::Struct(s));

    assert_eq!(e.variant_name(), "Labeled");
    assert_eq!(e.field_len(), 2);
    assert!(e.field("value").is_some());
    assert!(e.field("tag").is_some());
    assert!(e.field("nonexistent").is_none());
}

#[test]
fn dynamic_enum_reset_variant() {
    let mut e = DynamicEnum::new(0, "A", DynamicVariant::Unit);
    assert_eq!(e.variant_name(), "A");

    let mut s = DynamicStruct::new();
    s.push("x".into(), Box::new(1i32));
    e.reset(1, "B", DynamicVariant::Struct(s));

    assert_eq!(e.variant_name(), "B");
    assert_eq!(e.variant_index(), 1);
    assert_eq!(e.field_len(), 1);
}

#[test]
fn dynamic_enum_from_ref_my_option_unit() {
    let val = MyOption::None;
    let e_ref: &dyn Enum = &val;
    let dyn_e = DynamicEnum::from_ref(e_ref).unwrap();

    assert_eq!(dyn_e.variant_name(), "None");
    assert_eq!(dyn_e.field_len(), 0);
}

#[test]
fn dynamic_enum_from_ref_my_option_tuple() {
    let val = MyOption::Some(99);
    let e_ref: &dyn Enum = &val;
    let dyn_e = DynamicEnum::from_ref(e_ref).unwrap();

    assert_eq!(dyn_e.variant_name(), "Some");
    assert_eq!(dyn_e.field_len(), 1);

    let f: &i32 = dyn_e.field_at(0).unwrap().downcast_ref().unwrap();
    assert_eq!(*f, 99);
}

#[test]
fn dynamic_enum_roundtrip_via_dynamic() {
    let original = MyOption::Some(42);
    let e: &dyn Enum = &original;
    let dyn_e = DynamicEnum::from_ref(e).unwrap();

    let boxed: Box<dyn Reflect> = Box::new(dyn_e);
    let restored: Box<MyOption> = MyOption::from_reflect(boxed).unwrap();

    assert_eq!(*restored, MyOption::Some(42));
}

#[test]
fn dynamic_enum_try_clone() {
    let mut s = DynamicStruct::new();
    s.push("value".into(), Box::new(1.0f32));
    s.push("tag".into(), Box::new("hello"));
    let e = DynamicEnum::new(0, "Labeled", DynamicVariant::Struct(s));

    let cloned = e.try_clone().unwrap();
    assert_eq!(cloned.variant_name(), "Labeled");
    assert_eq!(cloned.field_len(), 2);
}

// ----------------------------------------------------------------------------
// Cross-type conversion: Dynamic ↔ Custom types
// ----------------------------------------------------------------------------

#[test]
fn struct_to_dynamic_to_different_struct() {
    // Point { x: i32, y: f32 } → DynamicStruct → Person
    // Person expects (name: String, age: i32), which doesn't match.
    let pt = Point { x: 5, y: 1.0 };
    let s: &dyn Struct = &pt;
    let dyn_s = DynamicStruct::from_ref(s).unwrap();

    let boxed: Box<dyn Reflect> = Box::new(dyn_s);
    let result = Person::from_reflect(boxed);
    // Should fail: Person requires 'name' (String) and 'age' (i32),
    // but we only have 'x' (i32) and 'y' (f32).
    assert!(result.is_err());
}

#[test]
fn tuple_to_dynamic_to_different_tuple() {
    // Vec3(1, 2, 3) → DynamicTuple → Pair (expects exactly 2 fields).
    let v = Vec3(1, 2, 3);
    let t: &dyn Tuple = &v;
    let dyn_t = DynamicTuple::from_ref(t).unwrap();

    let boxed: Box<dyn Reflect> = Box::new(dyn_t);
    let result = Pair::from_reflect(boxed);
    // Should fail: Pair expects 2 fields, Vec3 has 3 (tuple is strict).
    assert!(result.is_err());
}

#[test]
fn dynamic_struct_with_compatible_fields_but_wrong_types_fails() {
    // Create a dynamic struct with field names matching Person but wrong types.
    let mut dyn_s = DynamicStruct::new();
    dyn_s.push("name".into(), Box::new(42i32)); // should be String
    dyn_s.push("age".into(), Box::new("not_an_int")); // should be i32

    let boxed: Box<dyn Reflect> = Box::new(dyn_s);
    let result = Person::from_reflect(boxed);
    assert!(result.is_err());
}

// ----------------------------------------------------------------------------
// Dynamic type identity
// ----------------------------------------------------------------------------

#[test]
fn dynamic_types_are_dynamic() {
    assert!(DynamicStruct::new().is_dynamic());
    assert!(DynamicTuple::new().is_dynamic());
    assert!(DynamicArray::new().is_dynamic());
    assert!(DynamicList::new().is_dynamic());
    assert!(DynamicMap::new().is_dynamic());
    assert!(DynamicSet::new().is_dynamic());
    assert!(DynamicEnum::new(0, "A", DynamicVariant::Unit).is_dynamic());
}

#[test]
fn dynamic_types_have_correct_kind() {
    assert_eq!(DynamicStruct::new().reflect_kind(), ReflectKind::Struct);
    assert_eq!(DynamicTuple::new().reflect_kind(), ReflectKind::Tuple);
    assert_eq!(DynamicArray::new().reflect_kind(), ReflectKind::Array);
    assert_eq!(DynamicList::new().reflect_kind(), ReflectKind::List);
    assert_eq!(DynamicMap::new().reflect_kind(), ReflectKind::Map);
    assert_eq!(DynamicSet::new().reflect_kind(), ReflectKind::Set);
    assert_eq!(
        DynamicEnum::new(0, "A", DynamicVariant::Unit).reflect_kind(),
        ReflectKind::Enum
    );
}

#[test]
fn dynamic_types_are_not_concrete() {
    let pt = Point { x: 1, y: 2.0 };
    assert!(!pt.is_dynamic());
}

// ----------------------------------------------------------------------------
// reflect_clone for dynamic types
// ----------------------------------------------------------------------------

#[test]
fn dynamic_struct_reflect_clone() {
    let mut s = DynamicStruct::new();
    s.push("a".into(), Box::new(1i32));

    let cloned = s.reflect_clone().unwrap();
    let cloned_s: &DynamicStruct = cloned.downcast_ref().unwrap();
    assert_eq!(cloned_s.field_len(), 1);
}

#[test]
fn dynamic_tuple_reflect_clone() {
    let mut t = DynamicTuple::new();
    t.push(Box::new(1i32));

    let cloned = t.reflect_clone().unwrap();
    let cloned_t: &DynamicTuple = cloned.downcast_ref().unwrap();
    assert_eq!(cloned_t.field_len(), 1);
}

#[test]
fn dynamic_list_reflect_clone() {
    let mut list = DynamicList::new();
    list.push_back(Box::new(1i32)).unwrap();

    let cloned = list.reflect_clone().unwrap();
    let cloned_list: &DynamicList = cloned.downcast_ref().unwrap();
    assert_eq!(cloned_list.item_len(), 1);
}

// ----------------------------------------------------------------------------
// from_reflect: dynamic type fast path (same type)
// ----------------------------------------------------------------------------

#[test]
fn dynamic_struct_from_reflect_same_type() {
    let mut s = DynamicStruct::new();
    s.push("x".into(), Box::new(42i32));

    let boxed: Box<dyn Reflect> = Box::new(s);
    let result = DynamicStruct::from_reflect(boxed).unwrap();
    assert_eq!(result.field_len(), 1);
}

#[test]
fn dynamic_enum_from_reflect_same_type() {
    let mut t = DynamicTuple::new();
    t.push(Box::new(10i32));
    let e = DynamicEnum::new(0, "V", DynamicVariant::Tuple(t));

    let boxed: Box<dyn Reflect> = Box::new(e);
    let result = DynamicEnum::from_reflect(boxed).unwrap();
    assert_eq!(result.variant_name(), "V");
    assert_eq!(result.field_len(), 1);
}

// ----------------------------------------------------------------------------
// Hash and Eq for dynamic types
// ----------------------------------------------------------------------------

#[test]
fn dynamic_struct_eq() {
    let mut a = DynamicStruct::new();
    a.push("x".into(), Box::new(1i32));

    let mut b = DynamicStruct::new();
    b.push("x".into(), Box::new(1i32));

    assert!(a.reflect_eq(&b));
}

#[test]
fn dynamic_struct_not_eq_different_values() {
    let mut a = DynamicStruct::new();
    a.push("x".into(), Box::new(1i32));

    let mut b = DynamicStruct::new();
    b.push("x".into(), Box::new(2i32));

    assert!(!a.reflect_eq(&b));
}

#[test]
fn dynamic_tuple_eq() {
    let mut a = DynamicTuple::new();
    a.push(Box::new(1i32));
    a.push(Box::new(2i32));

    let mut b = DynamicTuple::new();
    b.push(Box::new(1i32));
    b.push(Box::new(2i32));

    assert!(a.reflect_eq(&b));
}

#[test]
fn dynamic_tuple_not_eq_different_length() {
    let mut a = DynamicTuple::new();
    a.push(Box::new(1i32));

    let mut b = DynamicTuple::new();
    b.push(Box::new(1i32));
    b.push(Box::new(2i32));

    assert!(!a.reflect_eq(&b));
}

#[test]
fn dynamic_set_eq_order_independent() {
    let mut a = DynamicSet::new();
    a.insert(Box::new(1i32));
    a.insert(Box::new(2i32));
    a.insert(Box::new(3i32));

    let mut b = DynamicSet::new();
    b.insert(Box::new(3i32));
    b.insert(Box::new(1i32));
    b.insert(Box::new(2i32));

    assert!(a.reflect_eq(&b));
}
