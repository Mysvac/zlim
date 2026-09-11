use core::panic::Location;

use crate::path::TypePath;

// -----------------------------------------------------------------------------
// &'static Location<'static>
// -----------------------------------------------------------------------------

/// `Location` is always used behind a `&'static` reference, so the reference
/// itself is transparent here: the path describes the `Location` type.
impl TypePath for &'static Location<'static> {
    #[inline]
    fn type_path() -> &'static str {
        "core::panic::Location"
    }

    #[inline]
    fn type_name() -> &'static str {
        "Location"
    }

    const IDENT: &str = "Location";
    const CRATE: Option<&str> = Some("core");
    const MODULE: Option<&str> = Some("core::panic");
}

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use core::panic::Location;

    #[test]
    #[rustfmt::skip]
    fn location() {
        assert_eq!(<&'static Location<'static>>::type_path(), "core::panic::Location");
        assert_eq!(<&'static Location<'static>>::type_name(), "Location");
        assert_eq!(<&'static Location<'static>>::IDENT, "Location");
        assert_eq!(<&'static Location<'static>>::CRATE, Some("core"));
        assert_eq!(<&'static Location<'static>>::MODULE, Some("core::panic"));
    }
}
