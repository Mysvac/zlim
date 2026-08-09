use core::fmt::{Debug, Display, Formatter};

const ANONYMOUS_NAME: &str = "_unknown_";

// -----------------------------------------------------------------------------
// DebugName

/// A wrapper type that provides debugging information for type name.
///
/// This type conditionally includes type name based on compilation settings:
/// - When `debug_assertions` are enabled or the `debug` feature is active,
///   it stores and displays the actual type name.
/// - Otherwise, it displays a placeholder string indicating debugging is disabled.
///
/// This is useful for debugging ECS-related issues where knowing the concrete type of components
/// or systems is valuable, while allowing the debugging overhead to be compiled out in release builds.
///
/// # Examples
///
/// ```ignore
/// // Create a debug name from a type
/// let name = DebugName::type_name::<String>();
/// assert!(!name.parse().is_empty());
///
/// // Create a debug name from a function pointer
/// let custom = DebugName::with(|| "custom_name");
/// assert_eq!(custom.parse(), "custom_name");
///
/// // Create an anonymous debug name
/// let anonymous = DebugName::anonymous();
/// assert_eq!(anonymous.parse(), "_unknown_");
/// ```
#[derive(Clone, Copy)]
pub(crate) struct DebugName {
    #[cfg(any(debug_assertions, feature = "debug"))]
    name: fn() -> &'static str,
}

impl DebugName {
    /// Creates a new `DebugName` that will display the type name of the specified type.
    ///
    /// This uses [`core::any::type_name`] internally to obtain the type's name at compile time.
    /// The type name is only stored when debugging is enabled; otherwise, this operation is a no-op.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// struct MyComponent;
    /// let name = DebugName::type_name::<MyComponent>();
    /// ```
    #[inline(always)]
    pub const fn type_name<T>() -> Self {
        cfg_select! {
            debug_assertions => Self { name: ::core::any::type_name::<T> },
            feature = "debug" => Self { name: ::core::any::type_name::<T> },
            _ => Self {},
        }
    }

    /// Creates a new anonymous `DebugName` that always displays `_unknown_`.
    ///
    /// This is useful as a fallback when a type name cannot be determined or when
    /// intentionally hiding the type information.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let anonymous = DebugName::anonymous();
    /// assert_eq!(anonymous.parse(), "_unknown_");
    /// ```
    #[inline(always)]
    pub const fn anonymous() -> Self {
        cfg_select! {
            debug_assertions => Self { name: || { ANONYMOUS_NAME } },
            feature = "debug" => Self { name: || { ANONYMOUS_NAME } },
            _ => Self {},
        }
    }
}

/// Formats a fully-qualified Rust type name into a
/// more readable form for debugging output.
#[inline(never)]
#[cfg(any(debug_assertions, feature = "debug"))]
fn debug_fmt(full_name: &str, f: &mut Formatter<'_>) -> core::fmt::Result {
    /// Collapses a fully-qualified type name segment to its most readable form.
    ///
    /// # Arguments
    /// * `name` - A segment of a type name (e.g., `core::option::Option`)
    ///
    /// # Returns
    /// The collapsed version of the type name segment
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
            f.write_str(collapse_type_name(&rest[0..index]))?;

            let special = &rest[index..=index];
            f.write_str(special)?;

            rest = &rest[(index + 1)..];
        } else {
            // If there are no special characters left, we're done!
            f.write_str(collapse_type_name(rest))?;
            return Ok(());
        }
    }

    Ok(())
}

impl Display for DebugName {
    /// Formats the debug name for display purposes.
    ///
    /// When debugging is enabled, this will show the collapsed type name.
    ///
    /// When debugging is disabled, it will show the anonymous placeholder (`_unknown_`).
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        cfg_select! {
            debug_assertions => debug_fmt((self.name)(), f),
            feature = "debug" => debug_fmt((self.name)(), f),
            _ => f.write_str(ANONYMOUS_NAME),
        }
    }
}

impl Debug for DebugName {
    /// Formats the debug name using the debug formatter.
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        cfg_select! {
            debug_assertions => debug_fmt((self.name)(), f),
            feature = "debug" => debug_fmt((self.name)(), f),
            _ => f.write_str(ANONYMOUS_NAME),
        }
    }
}

#[cfg(test)]
#[cfg(any(debug_assertions, feature = "debug"))]
mod tests {
    use super::DebugName;

    pub struct Foo;

    #[test]
    fn parse() {
        assert_eq!(DebugName::type_name::<u32>().to_string(), "u32");
        assert_eq!(DebugName::type_name::<bool>().to_string(), "bool");
        assert_eq!(DebugName::type_name::<char>().to_string(), "char");
        assert_eq!(DebugName::type_name::<f32>().to_string(), "f32");
        assert_eq!(DebugName::type_name::<usize>().to_string(), "usize");

        assert_eq!(DebugName::type_name::<&str>().to_string(), "&str");
        assert_eq!(DebugName::type_name::<&u32>().to_string(), "&u32");
        assert_eq!(DebugName::type_name::<&mut u32>().to_string(), "&mut u32");
        assert_eq!(DebugName::type_name::<&&u32>().to_string(), "&&u32");

        assert_eq!(
            DebugName::type_name::<*const u32>().to_string(),
            "*const u32"
        );
        assert_eq!(DebugName::type_name::<*mut u32>().to_string(), "*mut u32");

        assert_eq!(DebugName::type_name::<[u32; 5]>().to_string(), "[u32; 5]");
        assert_eq!(DebugName::type_name::<&[u32]>().to_string(), "&[u32]");
        assert_eq!(
            DebugName::type_name::<&mut [u32]>().to_string(),
            "&mut [u32]"
        );
        assert_eq!(DebugName::type_name::<[&u32; 3]>().to_string(), "[&u32; 3]");

        assert_eq!(DebugName::type_name::<()>().to_string(), "()");
        assert_eq!(DebugName::type_name::<(u32,)>().to_string(), "(u32,)");

        assert_eq! {
            DebugName::type_name::<(u32, Foo, &str)>().to_string(),
            "(u32, Foo, &str)",
        }
        assert_eq! {
            DebugName::type_name::<(&u32, &mut Foo)>().to_string(),
            "(&u32, &mut Foo)",
        }

        assert_eq! {
            DebugName::type_name::<Option<u32>>().to_string(),
            "Option<u32>",
        }
        assert_eq! {
            DebugName::type_name::<Option<&u32>>().to_string(),
            "Option<&u32>",
        }
        assert_eq! {
            DebugName::type_name::<Result<u32, ()>>().to_string(),
            "Result<u32, ()>",
        }
        assert_eq! {
            DebugName::type_name::<Result<&Foo, &str>>().to_string(),
            "Result<&Foo, &str>",
        }

        assert_eq! {
            DebugName::type_name::<Option<Option<u32>>>().to_string(),
            "Option<Option<u32>>",
        }
        assert_eq! {
            DebugName::type_name::<Result<Option<&u32>, ()>>().to_string(),
            "Result<Option<&u32>, ()>",
        }
        assert_eq! {
            DebugName::type_name::<Vec<Option<&Foo>>>().to_string(),
            "Vec<Option<&Foo>>",
        }
    }
}
