use std::path::Path;
use zlim_asset::path::{AssetPath, ParseAssetPathError};

/// Covers the shapes a static path string may take: a bare relative path, an
/// explicit `scheme://` source, a host whose port must not be mistaken for a
/// source, and an optional `#label` on top of either. The last two cases pin the
/// errors for a missing source and a missing label.
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

/// A path that parses must survive a round trip through RON unchanged, since that
/// is how a path is written out and read back. A string with a bare trailing `#`
/// does not parse in the first place, so it never gets that far.
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

/// A path that stands on its own replaces the base instead of extending it: the
/// absolute `/joe/next` ignores `alice/bob` entirely. Each case runs through both
/// the string and the parsed-path entry points and through the `_embed` variants,
/// which have to agree here because nothing of the base is left to resolve
/// against.
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

/// A path with no leading separator is relative. The plain resolution appends it
/// to the whole base path, so it ends up inside `alice/bob`, while the `_embed`
/// variants resolve it in the base's own directory `alice` — which is what a
/// sub-asset named from inside the file needs.
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

/// An explicit `./` or `../` marks the path as relative just as clearly as a bare
/// name does, and the two differ only in how many segments they walk up: `../`
/// drops one more than `./`. The `_embed` variants drop the base's own file name
/// as well, so each of their answers is one segment shorter.
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

/// A base path that already ends in `/` names a directory, so the `_embed`
/// variants must not drop its last segment the way they drop a file name.
/// Resolving `./martin` or `../martin` under `alice/bob/` therefore walks up from
/// the same place for every entry point.
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

/// Once base and relative path are joined the result is normalized: `.` segments
/// disappear and each `..` pops the segment in front of it. All four entry points
/// are checked so that they agree on the same normalized answer.
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

/// Normalization is not confined to the joined part: a `..` already present in the
/// base is resolved too, so `alice/../bob#carol` behaves like `bob` everywhere.
/// For the `_embed` variants one case is left with a leading `..`, which has to be
/// kept because there is nothing left above it to pop.
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

/// The same normalization with an explicit source on the base: the `.` and `..`
/// segments are resolved inside the path component only, and every entry point
/// carries the source through untouched.
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

/// An absolute path replaces the base path rather than extending it, so
/// `/martin/stephan` resolves to `martin/stephan` instead of landing inside
/// `alice/bob`. The label rules are the ones that were already in force: the
/// requested label simply replaces the base's.
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

/// A path that names its own source replaces both the base path and the base's
/// source. Unlike the absolute case this needs no `_embed` distinction: the
/// source pins the root, so there is no base directory left to resolve against.
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

/// A `..` with nothing left to pop is preserved rather than clamped away. How
/// much there is to pop depends on the entry point: the plain resolution counts
/// the base's two segments, while the `_embed` variants start from the base's
/// directory and so have to leave one `..` in the result.
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

/// A base path that already climbs out of the root keeps those segments when a
/// file beside it is embedded: `c.bin` resolves under `../../a`, next to the file
/// it was named from, instead of being pulled back towards the root.
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

/// A plain relative resolution treats the base path as a directory, so `c.bin`
/// named from `../../a/b.gltf` lands inside that file's own path. The leading `..`
/// segments survive, because nothing in the joined path pops them.
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

/// The full extension is everything after the first dot of the file name, so
/// `a.tar.gz` reports `tar.gz` where the plain extension reports only `gz`. A
/// query string is dropped, and the spelling is taken from the file name itself.
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

/// Only what follows the last dot of the full extension counts, so `a.tar.gz`
/// reports `gz`. As in the full-extension case a query string is dropped and the
/// file name's own spelling is kept.
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
