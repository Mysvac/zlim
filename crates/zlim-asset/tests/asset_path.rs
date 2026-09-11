use std::path::Path;
use zlim_asset::path::{AssetPath, ParseAssetPathError};

#[test]
fn parse_asset_path() {
    macro_rules! assert_path {
        ($result:ident, $expect:expr) => {
            assert_eq!(
                $result
                    .as_ref()
                    .map(|path| { (path.source(), path.path(), path.label(),) }),
                $expect,
            );
        };
    }

    let result = AssetPath::try_parse_static("a/b.test");
    assert_path!(result, Ok((None, Path::new("a/b.test"), None)));

    let result = AssetPath::try_parse_static("http://a/b.test");
    assert_path!(result, Ok((Some("http"), Path::new("a/b.test"), None)));

    let result = AssetPath::try_parse_static("http://a/b.test#Foo");
    assert_path!(
        result,
        Ok((Some("http"), Path::new("a/b.test"), Some("Foo")))
    );

    let result = AssetPath::try_parse_static("localhost:80/b.test");
    assert_path!(result, Ok((None, Path::new("localhost:80/b.test"), None)));

    let result = AssetPath::try_parse_static("http://localhost:80/b.test");
    assert_path!(
        result,
        Ok((Some("http"), Path::new("localhost:80/b.test"), None))
    );

    let result = AssetPath::try_parse_static("http://localhost:80/b.test#Foo");
    assert_path!(
        result,
        Ok((Some("http"), Path::new("localhost:80/b.test"), Some("Foo")))
    );

    let result = AssetPath::try_parse_static("http://");
    assert_path!(result, Ok((Some("http"), Path::new(""), None)));

    let result = AssetPath::try_parse_static("://x");
    assert_path!(result, Err(&ParseAssetPathError::MissingSource));

    let result = AssetPath::try_parse_static("a/b.test#");
    assert_path!(result, Err(&ParseAssetPathError::MissingLabel));
}

#[test]
fn test_serialize() {
    assert!(ron::de::from_str::<AssetPath>("\"a/b.test\"").is_ok());
    assert!(ron::de::from_str::<AssetPath>("\"a/b.test#\"").is_err());

    macro_rules! test_serialize {
        ($path:literal) => {{
            let path = ron::de::from_str::<AssetPath>($path).unwrap();
            let ser = ron::ser::to_string(&path).unwrap();
            assert_eq!($path, ser.as_str());
        }};
    }

    test_serialize!(r###""a/b.test""###);
    test_serialize!(r###""http://a/b.test#Foo""###);
    test_serialize!(r###""localhost:80/b.test""###);
    test_serialize!(r###""http://localhost:80/b.test#Foo""###);
}

#[test]
fn test_parent() {
    // Parent consumes path segments, returns None when insufficient
    let result = AssetPath::from("a/b.test");
    assert_eq!(result.parent(), Some(AssetPath::from("a")));
    assert_eq!(result.parent().unwrap().parent(), Some(AssetPath::from("")));
    assert_eq!(result.parent().unwrap().parent().unwrap().parent(), None);

    // Parent cannot consume asset source
    let result = AssetPath::from("http://a");
    assert_eq!(result.parent(), Some(AssetPath::from("http://")));
    assert_eq!(result.parent().unwrap().parent(), None);

    // Parent consumes labels
    let result = AssetPath::from("http://a#Foo");
    assert_eq!(result.parent(), Some(AssetPath::from("http://")));
}

#[test]
fn test_with_source() {
    let result = AssetPath::from("http://a#Foo");
    assert_eq!(result.with_source("ftp"), AssetPath::from("ftp://a#Foo"));
}

#[test]
fn test_without_label() {
    let result = AssetPath::from("http://a#Foo");
    assert_eq!(result.without_label(), AssetPath::from("http://a"));
}

#[test]
fn test_resolve_full() {
    // A "full" path should ignore the base path.
    let base = AssetPath::from("alice/bob#carol");
    assert_eq!(
        base.resolve_str("/joe/next").unwrap(),
        AssetPath::from("joe/next")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("/joe/next")),
        AssetPath::from("joe/next")
    );
    assert_eq!(
        base.resolve_embed_str("/joe/next").unwrap(),
        AssetPath::from("joe/next")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("/joe/next")),
        AssetPath::from("joe/next")
    );
    assert_eq!(
        base.resolve_str("/joe/next#dave").unwrap(),
        AssetPath::from("joe/next#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("/joe/next#dave")),
        AssetPath::from("joe/next#dave")
    );
    assert_eq!(
        base.resolve_embed_str("/joe/next#dave").unwrap(),
        AssetPath::from("joe/next#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("/joe/next#dave")),
        AssetPath::from("joe/next#dave")
    );
}

#[test]
fn test_resolve_implicit_relative() {
    // A path with no initial directory separator should be considered relative.
    let base = AssetPath::from("alice/bob#carol");
    assert_eq!(
        base.resolve_str("joe/next").unwrap(),
        AssetPath::from("alice/bob/joe/next")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("joe/next")),
        AssetPath::from("alice/bob/joe/next")
    );
    assert_eq!(
        base.resolve_embed_str("joe/next").unwrap(),
        AssetPath::from("alice/joe/next")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("joe/next")),
        AssetPath::from("alice/joe/next")
    );
    assert_eq!(
        base.resolve_str("joe/next#dave").unwrap(),
        AssetPath::from("alice/bob/joe/next#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("joe/next#dave")),
        AssetPath::from("alice/bob/joe/next#dave")
    );
    assert_eq!(
        base.resolve_embed_str("joe/next#dave").unwrap(),
        AssetPath::from("alice/joe/next#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("joe/next#dave")),
        AssetPath::from("alice/joe/next#dave")
    );
}

#[test]
fn test_resolve_explicit_relative() {
    // A path which begins with "./" or "../" is treated as relative
    let base = AssetPath::from("alice/bob#carol");
    assert_eq!(
        base.resolve_str("./martin#dave").unwrap(),
        AssetPath::from("alice/bob/martin#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("./martin#dave")),
        AssetPath::from("alice/bob/martin#dave")
    );
    assert_eq!(
        base.resolve_embed_str("./martin#dave").unwrap(),
        AssetPath::from("alice/martin#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("./martin#dave")),
        AssetPath::from("alice/martin#dave")
    );
    assert_eq!(
        base.resolve_str("../martin#dave").unwrap(),
        AssetPath::from("alice/martin#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("../martin#dave")),
        AssetPath::from("alice/martin#dave")
    );
    assert_eq!(
        base.resolve_embed_str("../martin#dave").unwrap(),
        AssetPath::from("martin#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("../martin#dave")),
        AssetPath::from("martin#dave")
    );
}

#[test]
fn test_resolve_trailing_slash() {
    // A path which begins with "./" or "../" is treated as relative
    let base = AssetPath::from("alice/bob/");
    assert_eq!(
        base.resolve_str("./martin#dave").unwrap(),
        AssetPath::from("alice/bob/martin#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("./martin#dave")),
        AssetPath::from("alice/bob/martin#dave")
    );
    assert_eq!(
        base.resolve_embed_str("./martin#dave").unwrap(),
        AssetPath::from("alice/bob/martin#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("./martin#dave")),
        AssetPath::from("alice/bob/martin#dave")
    );
    assert_eq!(
        base.resolve_str("../martin#dave").unwrap(),
        AssetPath::from("alice/martin#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("../martin#dave")),
        AssetPath::from("alice/martin#dave")
    );
    assert_eq!(
        base.resolve_embed_str("../martin#dave").unwrap(),
        AssetPath::from("alice/martin#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("../martin#dave")),
        AssetPath::from("alice/martin#dave")
    );
}

#[test]
fn test_resolve_canonicalize() {
    // Test that ".." and "." are removed after concatenation.
    let base = AssetPath::from("alice/bob#carol");
    assert_eq!(
        base.resolve_str("./martin/stephan/..#dave").unwrap(),
        AssetPath::from("alice/bob/martin#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("./martin/stephan/..#dave")),
        AssetPath::from("alice/bob/martin#dave")
    );
    assert_eq!(
        base.resolve_embed_str("./martin/stephan/..#dave").unwrap(),
        AssetPath::from("alice/martin#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("./martin/stephan/..#dave")),
        AssetPath::from("alice/martin#dave")
    );
    assert_eq!(
        base.resolve_str("../martin/.#dave").unwrap(),
        AssetPath::from("alice/martin#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("../martin/.#dave")),
        AssetPath::from("alice/martin#dave")
    );
    assert_eq!(
        base.resolve_embed_str("../martin/.#dave").unwrap(),
        AssetPath::from("martin#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("../martin/.#dave")),
        AssetPath::from("martin#dave")
    );
    assert_eq!(
        base.resolve_str("/martin/stephan/..#dave").unwrap(),
        AssetPath::from("martin#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("/martin/stephan/..#dave")),
        AssetPath::from("martin#dave")
    );
    assert_eq!(
        base.resolve_embed_str("/martin/stephan/..#dave").unwrap(),
        AssetPath::from("martin#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("/martin/stephan/..#dave")),
        AssetPath::from("martin#dave")
    );
}

#[test]
fn test_resolve_canonicalize_base() {
    // Test that ".." and "." are removed after concatenation even from the base path.
    let base = AssetPath::from("alice/../bob#carol");
    assert_eq!(
        base.resolve_str("./martin/stephan/..#dave").unwrap(),
        AssetPath::from("bob/martin#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("./martin/stephan/..#dave")),
        AssetPath::from("bob/martin#dave")
    );
    assert_eq!(
        base.resolve_embed_str("./martin/stephan/..#dave").unwrap(),
        AssetPath::from("martin#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("./martin/stephan/..#dave")),
        AssetPath::from("martin#dave")
    );
    assert_eq!(
        base.resolve_str("../martin/.#dave").unwrap(),
        AssetPath::from("martin#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("../martin/.#dave")),
        AssetPath::from("martin#dave")
    );
    assert_eq!(
        base.resolve_embed_str("../martin/.#dave").unwrap(),
        AssetPath::from("../martin#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("../martin/.#dave")),
        AssetPath::from("../martin#dave")
    );
    assert_eq!(
        base.resolve_str("/martin/stephan/..#dave").unwrap(),
        AssetPath::from("martin#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("/martin/stephan/..#dave")),
        AssetPath::from("martin#dave")
    );
    assert_eq!(
        base.resolve_embed_str("/martin/stephan/..#dave").unwrap(),
        AssetPath::from("martin#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("/martin/stephan/..#dave")),
        AssetPath::from("martin#dave")
    );
}

#[test]
fn test_resolve_canonicalize_with_source() {
    // Test that ".." and "." are removed after concatenation.
    let base = AssetPath::from("source://alice/bob#carol");
    assert_eq!(
        base.resolve_str("./martin/stephan/..#dave").unwrap(),
        AssetPath::from("source://alice/bob/martin#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("./martin/stephan/..#dave")),
        AssetPath::from("source://alice/bob/martin#dave")
    );
    assert_eq!(
        base.resolve_embed_str("./martin/stephan/..#dave").unwrap(),
        AssetPath::from("source://alice/martin#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("./martin/stephan/..#dave")),
        AssetPath::from("source://alice/martin#dave")
    );
    assert_eq!(
        base.resolve_str("../martin/.#dave").unwrap(),
        AssetPath::from("source://alice/martin#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("../martin/.#dave")),
        AssetPath::from("source://alice/martin#dave")
    );
    assert_eq!(
        base.resolve_embed_str("../martin/.#dave").unwrap(),
        AssetPath::from("source://martin#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("../martin/.#dave")),
        AssetPath::from("source://martin#dave")
    );
    assert_eq!(
        base.resolve_str("/martin/stephan/..#dave").unwrap(),
        AssetPath::from("source://martin#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("/martin/stephan/..#dave")),
        AssetPath::from("source://martin#dave")
    );
    assert_eq!(
        base.resolve_embed_str("/martin/stephan/..#dave").unwrap(),
        AssetPath::from("source://martin#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("/martin/stephan/..#dave")),
        AssetPath::from("source://martin#dave")
    );
}

#[test]
fn test_resolve_absolute() {
    // Paths beginning with '/' replace the base path
    let base = AssetPath::from("alice/bob#carol");
    assert_eq!(
        base.resolve_str("/martin/stephan").unwrap(),
        AssetPath::from("martin/stephan")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("/martin/stephan")),
        AssetPath::from("martin/stephan")
    );
    assert_eq!(
        base.resolve_embed_str("/martin/stephan").unwrap(),
        AssetPath::from("martin/stephan")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("/martin/stephan")),
        AssetPath::from("martin/stephan")
    );
    assert_eq!(
        base.resolve_str("/martin/stephan#dave").unwrap(),
        AssetPath::from("martin/stephan/#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("/martin/stephan#dave")),
        AssetPath::from("martin/stephan/#dave")
    );
    assert_eq!(
        base.resolve_embed_str("/martin/stephan#dave").unwrap(),
        AssetPath::from("martin/stephan/#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("/martin/stephan#dave")),
        AssetPath::from("martin/stephan/#dave")
    );
}

#[test]
fn test_resolve_asset_source() {
    // Paths beginning with 'source://' replace the base path
    let base = AssetPath::from("alice/bob#carol");
    assert_eq!(
        base.resolve_str("source://martin/stephan").unwrap(),
        AssetPath::from("source://martin/stephan")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("source://martin/stephan")),
        AssetPath::from("source://martin/stephan")
    );
    assert_eq!(
        base.resolve_embed_str("source://martin/stephan").unwrap(),
        AssetPath::from("source://martin/stephan")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("source://martin/stephan")),
        AssetPath::from("source://martin/stephan")
    );
    assert_eq!(
        base.resolve_str("source://martin/stephan#dave").unwrap(),
        AssetPath::from("source://martin/stephan/#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("source://martin/stephan#dave")),
        AssetPath::from("source://martin/stephan/#dave")
    );
    assert_eq!(
        base.resolve_embed_str("source://martin/stephan#dave")
            .unwrap(),
        AssetPath::from("source://martin/stephan/#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("source://martin/stephan#dave")),
        AssetPath::from("source://martin/stephan/#dave")
    );
}

#[test]
fn test_resolve_label() {
    // A relative path with only a label should replace the label portion
    let base = AssetPath::from("alice/bob#carol");
    assert_eq!(
        base.resolve_str("#dave").unwrap(),
        AssetPath::from("alice/bob#dave")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("#dave")),
        AssetPath::from("alice/bob#dave")
    );
    assert_eq!(
        base.resolve_embed_str("#dave").unwrap(),
        AssetPath::from("alice/bob#dave")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("#dave")),
        AssetPath::from("alice/bob#dave")
    );
}

#[test]
fn test_resolve_insufficient_elements() {
    // Ensure that ".." segments are preserved if there are insufficient elements to remove them.
    let base = AssetPath::from("alice/bob#carol");
    assert_eq!(
        base.resolve_str("../../joe/next").unwrap(),
        AssetPath::from("joe/next")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("../../joe/next")),
        AssetPath::from("joe/next")
    );
    assert_eq!(
        base.resolve_embed_str("../../joe/next").unwrap(),
        AssetPath::from("../joe/next")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("../../joe/next")),
        AssetPath::from("../joe/next")
    );
}

#[test]
fn resolve_embed_relative_to_external_path() {
    let base = AssetPath::from("../../a/b.gltf");
    assert_eq!(
        base.resolve_embed_str("c.bin").unwrap(),
        AssetPath::from("../../a/c.bin")
    );
    assert_eq!(
        base.resolve_embed(&AssetPath::parse("c.bin")),
        AssetPath::from("../../a/c.bin")
    );
}

#[test]
fn resolve_relative_to_external_path() {
    let base = AssetPath::from("../../a/b.gltf");
    assert_eq!(
        base.resolve_str("c.bin").unwrap(),
        AssetPath::from("../../a/b.gltf/c.bin")
    );
    assert_eq!(
        base.resolve(&AssetPath::parse("c.bin")),
        AssetPath::from("../../a/b.gltf/c.bin")
    );
}

#[test]
fn test_full_extension() {
    let result = AssetPath::from("http://a.tar.gz#Foo");
    assert_eq!(result.full_extension(), Some("tar.gz"));

    let result = AssetPath::from("http://a#Foo");
    assert_eq!(result.full_extension(), None);

    let result = AssetPath::from("http://a.tar.bz2?foo=bar#Baz");
    assert_eq!(result.full_extension(), Some("tar.bz2"));

    let result = AssetPath::from("asset.Custom");
    assert_eq!(result.full_extension(), Some("Custom"));
}

#[test]
fn test_extension() {
    let result = AssetPath::from("http://a.tar.gz#Foo");
    assert_eq!(result.extension(), Some("gz"));

    let result = AssetPath::from("http://a#Foo");
    assert_eq!(result.extension(), None);

    let result = AssetPath::from("http://a.tar.bz2?foo=bar#Baz");
    assert_eq!(result.extension(), Some("bz2"));

    let result = AssetPath::from("asset.Custom");
    assert_eq!(result.extension(), Some("Custom"));
}
