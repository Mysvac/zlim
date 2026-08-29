#![doc = include_str!("../README.md")]
#![no_std]

// -----------------------------------------------------------------------------
// Macros

/// Represents a disabled conditional compilation block.
///
/// When used as a boolean expression, returns `false`. When used as a block,
/// the contents are ignored unless specified otherwise via `else` or `if-else`.
///
/// # Examples
///
/// ```
/// use zlim_cfg as cfg;
///
/// let mut x = 0;
///
/// // empty -> false
/// assert!( !cfg::disabled!() );
///
/// // A -> empty (do nothing)
/// cfg::disabled!{ x += 100; }
///
/// // if { A } else { B } -> B
/// cfg::disabled!{
///     if {
///         panic!();
///     } else {
///         x += 1;
///     }
/// }
///
/// assert_eq!(x, 1);
/// ```
#[macro_export]
macro_rules! disabled {
    () => { false };
    (if { $($p:tt)* } else { $($n:tt)* }) => { $($n)* };
    ($($p:tt)*) => {};
}

/// Represents an enabled conditional compilation block.
///
/// When used as a boolean expression, returns `true`. When used as a block,
/// the contents are executed normally unless overridden by `else` or `if-else`.
///
/// # Examples
///
/// ```
/// use zlim_cfg as cfg;
///
/// let mut x = 0;
///
/// // empty -> true
/// assert!( cfg::enabled!() );
///
/// // A -> A
/// cfg::enabled!{ x += 100; }
///
/// // if { A } else { B } -> A
/// cfg::enabled!{
///     if {
///         x += 1;
///     } else {
///         panic!();
///     }
/// }
///
/// assert_eq!(x, 101);
/// ```
#[macro_export]
macro_rules! enabled {
    () => { true };
    (if { $($p:tt)* } else { $($n:tt)* }) => { $($p)* };
    ($($p:tt)*) => { $($p)* };
}

/// Define aliases for compilation options.
///
/// The generated alias behaves like [`enabled`] or [`disabled`],
/// depending on whether the compilation condition is active.
///
/// # Examples
///
/// ```
/// use zlim_cfg as cfg;
///
/// cfg::define_alias!{
///     #[cfg(test)] => test,
/// };
///
/// // `test` is eq to 'cfg::enabled' in testing.
/// // Otherwise it is eq to 'cfg::disabled'.
/// let mut x = false;
/// test!{ x = true; };
///
/// // Docs test is not Unit Test.
/// // So `test!` is eq to 'cfg::disabled'.
/// assert!(x == false);
/// ```
#[macro_export]
macro_rules! define_alias {
    ( #[cfg($meta:meta)] => { $(#[$id_meta:meta])* $id:ident } $(,)? ) => {
        #[cfg($meta)]
        #[doc = concat!("An alias for `#[cfg(", stringify!($meta), ")]` .\n")]
        #[doc = "See [`zlim_cfg::define_alias`] for details."]
        $(#[$id_meta])*
        #[doc(inline)]
        pub use $crate::enabled as $id;

        #[cfg(not($meta))]
        #[doc = concat!("An alias for `#[cfg(", stringify!($meta), ")]` .\n")]
        #[doc = "See [`zlim_cfg::define_alias`] for details."]
        $(#[$id_meta])*
        #[doc(inline)]
        pub use $crate::disabled as $id;
    };
    ( #[cfg($meta:meta)] => $id:ident $(,)? ) => {
        $crate::define_alias! { #[cfg($meta)] => { $id } }
    };
    ( #[cfg($meta:meta)] => $id:ident , $( $rest:tt )+ ) => {
        $crate::define_alias! { #[cfg($meta)] => { $id } }
        $crate::define_alias! { $( $rest )+ }
    };
    ( #[cfg($meta:meta)] => { $(#[$id_meta:meta])* $id:ident } , $( $rest:tt )+ ) => {
        $crate::define_alias! { #[cfg($meta)] => { $(#[$id_meta])* $id } }
        $crate::define_alias! { $($rest)+ }
    };
    ( #[cfg($meta:meta)] => { $(#[$id_meta:meta])* $id:ident } $( $rest:tt )+ ) => {
        $crate::define_alias! { #[cfg($meta)] => { $(#[$id_meta])* $id } }
        $crate::define_alias! { $($rest)+ }
    };
}

/// A conditional compilation macro similar to a `switch` statement.
///
/// Allows matching against multiple compilation conditions,
/// executing the first matching branch. Supports `#[cfg(...)]`
/// attributes, path-based conditions, and a default `_` branch.
///
/// # Example
///
/// ```
/// use zlim_cfg as cfg;
///
/// let mut x = 0;
/// cfg::switch! {
///     #[cfg(test)] => {
///         x += 1;
///     }
///     cfg::enabled => {
///         x += 10;
///     }
///     _ => {
///         x += 100;
///     }
/// }
/// assert!(x == 1 || x == 10);
/// ```
#[macro_export]
macro_rules! switch {
    (_ => { $($output:tt)* } $(,)?) => {
        $($output)*
    };
    ($cond:path => { $($output:tt)* } $(,)?) => {
        $($output)*
    };
    (#[cfg($cfg:meta)] => { $($output:tt)* } $(,)?) => {
        #[cfg($cfg)] $crate::switch! { _ => { $($output)* } }
    };
    ( $cond:path => { $($output:tt)* } , $( $rest:tt )+ ) => {
        $cond! { if { $($output)* } else { $crate::switch! { $($rest)+ } } }
    };
    ( $cond:path => { $($output:tt)* } $( $rest:tt )+ ) => {
        $cond! { if { $($output)* } else { $crate::switch! { $($rest)+ } } }
    };
    ( #[cfg($cfg:meta)] => { $($output:tt)* } , $( $rest:tt )+ ) => {
        #[cfg($cfg)] $crate::switch! { _ => { $($output)* } }
        #[cfg(not($cfg))] $crate::switch! { $($rest)+ }
    };
    ( #[cfg($cfg:meta)] => { $($output:tt)* } $( $rest:tt )+ ) => {
        #[cfg($cfg)] $crate::switch! { _ => { $($output)* } }
        #[cfg(not($cfg))] $crate::switch! { $($rest)+ }
    };
}
