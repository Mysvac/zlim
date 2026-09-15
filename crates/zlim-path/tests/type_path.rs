use zlim_path::TypePath;

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

#[derive(TypePath)]
#[type_path = "my_crate::boo::MyArray"]
struct TestMyArray<T, const N: usize>([T; N]);

/// A type with a lifetime parameter: it only implements `TypePath` for `'static`
/// instantiations, which is what the derive's `where Self: 'static` clause expresses.
#[derive(TypePath)]
struct TestLifetime<'a> {
    _marker: &'a (),
}

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

/// An explicit `#[type_path = "..."]` is adopted verbatim and the module is the
/// path with its last segment removed, so a two-segment path leaves crate and
/// module identical.
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

/// A path made of a single segment has no crate or module to report, so those
/// two associated constants are `None` while the name fields still hold the path.
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

/// Generic arguments are recursed into, and each one contributes its full path
/// to `type_path` but only its short name to `type_name`.
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

/// The same recursion applies under a custom `#[type_path]`, so the outer type
/// takes the written path while the generic argument keeps its own.
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

/// A const generic argument is rendered with its value rather than erased, so
/// the same type at another length would be a distinct path.
#[test]
fn with_const_generic() {
    assert_path! {
        TestMyArray<TestLoc, 5>,
        "my_crate::boo::MyArray<my_game::Location, 5>",
        "MyArray<Location, 5>",
        "MyArray",
        Some("my_crate"),
        Some("my_crate::boo"),
    }
}

/// Lifetimes never show up in the generated strings, so a type that is only
/// usable at `'static` still reads as a plain path with no parameter.
#[test]
fn with_lifetime() {
    assert_path! {
        TestLifetime<'static>,
        "type_path::TestLifetime",
        "TestLifetime",
        "TestLifetime",
        Some("type_path"),
        Some("type_path"),
    }
}
