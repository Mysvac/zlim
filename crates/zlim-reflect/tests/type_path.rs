use zlim_reflect::TypePath;

#[derive(TypePath)]
struct TestUnit;

#[derive(TypePath)]
#[type_path = "my_game::components::Position"]
struct TestPos;

#[derive(TypePath)]
#[type_path = "my_game::Location"]
struct TestLoc;

#[derive(TypePath)]
#[type_path = "JustType"]
struct TestJustType;

#[derive(TypePath)]
struct TestContainer<T>(T);

#[derive(TypePath)]
#[type_path = "my_crate::boo::MyVec"]
struct TestMyVec<T>(T);

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

// -----------------------------------------------------------------------------

#[test]
fn non_generic_default_path() {
    assert_path! {
        TestUnit,
        "type_path::TestUnit",
        "TestUnit",
        "TestUnit",
        Some("type_path"),
        Some("type_path"),
    }
}

#[test]
fn non_generic_custom_path() {
    assert_path! {
        TestPos,
        "my_game::components::Position",
        "Position",
        "Position",
        Some("my_game"),
        Some("my_game::components"),
    }

    assert_path! {
        TestLoc,
        "my_game::Location",
        "Location",
        "Location",
        Some("my_game"),
        Some("my_game"),
    }
}

#[test]
fn non_generic_single_segment() {
    assert_path! {
        TestJustType,
        "JustType",
        "JustType",
        "JustType",
        None,
        None,
    }
}

#[test]
fn generic_type_path() {
    assert_path! {
        TestContainer<TestPos>,
        "type_path::TestContainer<my_game::components::Position>",
        "TestContainer<Position>",
        "TestContainer",
        Some("type_path"),
        Some("type_path"),
    }

    assert_path! {
        TestContainer<TestJustType>,
        "type_path::TestContainer<JustType>",
        "TestContainer<JustType>",
        "TestContainer",
        Some("type_path"),
        Some("type_path"),
    }
}

#[test]
fn generic_custom_path() {
    assert_path! {
        TestMyVec<TestLoc>,
        "my_crate::boo::MyVec<my_game::Location>",
        "MyVec<Location>",
        "MyVec",
        Some("my_crate"),
        Some("my_crate::boo"),
    }

    assert_path! {
        TestMyVec<TestJustType>,
        "my_crate::boo::MyVec<JustType>",
        "MyVec<JustType>",
        "MyVec",
        Some("my_crate"),
        Some("my_crate::boo"),
    }
}
