use zlim_utils::mem::Global;

#[inline(never)]
pub(crate) fn split_path(path: &'static str) -> (&'static str, &'static str, &'static str) {
    let type_path = path;

    let (prefix, generics) = path.split_once('<').unwrap_or((path, ""));

    let (mut module, mut type_name) = prefix.rsplit_once("::").unwrap_or(("", path));

    if module.starts_with('[') || module.starts_with('(') {
        ::core::hint::cold_path();
        module = "";
    }

    if !generics.is_empty() {
        ::core::hint::cold_path();
        let mut buffer = String::with_capacity(path.len());
        parse_type_name(path, &mut buffer);
        type_name = Global::alloc_str(&buffer)
    }

    (type_path, type_name, module)
}

// copy from `zlim_utils::debug::DebugName`
fn parse_type_name(full_name: &str, f: &mut String) {
    fn collapse_type_name(name: &str) -> &str {
        let mut segments = name.rsplit("::");
        let last = segments.next().unwrap();

        // Enums types are retained.
        // As heuristic, we assume the enum type to be uppercase.
        if let Some(second_last) = segments.next()
            && second_last.starts_with(char::is_uppercase)
        {
            let index = name.len() - last.len() - second_last.len() - 2;
            &name[index..]
        } else {
            last
        }
    }

    const SPECIAL_CHARS: [char; 11] = [' ', '<', '>', '(', ')', '[', ']', ',', ';', '&', '*'];
    let mut rest = full_name;

    while !rest.is_empty() {
        let index = rest.find(|c| SPECIAL_CHARS.contains(&c));

        if let Some(index) = index {
            f.push_str(collapse_type_name(&rest[0..index]));
            let special = &rest[index..=index];
            f.push_str(special);
            rest = &rest[(index + 1)..];
        } else {
            // If there are no special characters left, we're done!
            f.push_str(collapse_type_name(rest));
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_type_name, split_path};

    fn parse(full_name: &str) -> String {
        let mut buffer = String::new();
        parse_type_name(full_name, &mut buffer);
        buffer
    }

    #[test]
    fn parse_basic() {
        assert_eq!(parse("u32"), "u32");
        assert_eq!(parse("bool"), "bool");
        assert_eq!(parse("char"), "char");
        assert_eq!(parse("f32"), "f32");
        assert_eq!(parse("usize"), "usize");
    }

    #[test]
    fn parse_reference() {
        assert_eq!(parse("&str"), "&str");
        assert_eq!(parse("&u32"), "&u32");
        assert_eq!(parse("&mut u32"), "&mut u32");
        assert_eq!(parse("&&u32"), "&&u32");
    }

    #[test]
    fn parse_pointer() {
        assert_eq!(parse("*const u32"), "*const u32");
        assert_eq!(parse("*mut u32"), "*mut u32");
    }

    #[test]
    fn parse_array() {
        assert_eq!(parse("[u32; 5]"), "[u32; 5]");
        assert_eq!(parse("&[u32]"), "&[u32]");
        assert_eq!(parse("&mut [u32]"), "&mut [u32]");
        assert_eq!(parse("[&u32; 3]"), "[&u32; 3]");
    }

    #[test]
    fn parse_tuple() {
        assert_eq!(parse("()"), "()");
        assert_eq!(parse("(u32,)"), "(u32,)");
        assert_eq!(parse("(u32, Foo, &str)"), "(u32, Foo, &str)");
        assert_eq!(parse("(&u32, &mut Foo)"), "(&u32, &mut Foo)");
    }

    #[test]
    fn parse_generic() {
        assert_eq!(parse("Option<u32>"), "Option<u32>");
        assert_eq!(parse("Option<&u32>"), "Option<&u32>");
        assert_eq!(parse("Result<u32, ()>"), "Result<u32, ()>");
        assert_eq!(parse("Result<&Foo, &str>"), "Result<&Foo, &str>");
        assert_eq!(parse("Option<Option<u32>>"), "Option<Option<u32>>");
        assert_eq!(parse("Vec<Option<&Foo>>"), "Vec<Option<&Foo>>");
        assert_eq!(
            parse("Result<Option<&u32>, ()>"),
            "Result<Option<&u32>, ()>"
        );
    }

    #[test]
    fn parse_with_module() {
        assert_eq!(parse("core::option::Option<u32>"), "Option<u32>");
        assert_eq!(
            parse("core::result::Result<u32, core::convert::Infallible>"),
            "Result<u32, Infallible>"
        );
        assert_eq!(
            parse("alloc::vec::Vec<core::option::Option<&Foo>>"),
            "Vec<Option<&Foo>>"
        );
    }

    #[test]
    fn parse_enum() {
        assert_eq!(parse("core::option::Option::Some"), "Option::Some");
    }

    #[test]
    fn split() {
        assert_eq!(split_path("u32"), ("u32", "u32", ""));

        let (type_path, type_name, module) = split_path("core::option::Option<u32>");
        assert_eq!(type_path, "core::option::Option<u32>");
        assert_eq!(type_name, "Option<u32>");
        assert_eq!(module, "core::option");

        let (_, type_name, module) = split_path("alloc::vec::Vec<core::option::Option<&Foo>>");
        assert_eq!(type_name, "Vec<Option<&Foo>>");
        assert_eq!(module, "alloc::vec");

        let (_, type_name, module) = split_path("core::result::Result<u32, ()>");
        assert_eq!(type_name, "Result<u32, ()>");
        assert_eq!(module, "core::result");
    }
}
