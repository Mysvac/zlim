//! Compile control macros
//!
//! This crate provides macros to manage conditional compilation in
//! a more flexible and readable way, similar to [`cfg`] attributes
//! but with macro-based syntax.
//!
//! # Note
//!
//! Starting from **Rust 1.95**, the standard library provides [`cfg_select!`]
//! macro which offers similar functionality.
//!
//! # Examples
//!
//! ```
//! # #![expect(unexpected_cfgs, reason = "doc-test")]
//! use zlim_cfg;
//!
//! mod cfg {
//!     pub use zlim_cfg::switch;
//!     // Define aliases for compilation options.
//!     zlim_cfg::define_alias!{
//!         #[cfg(feature = "std")] => std,
//!     }
//! }
//!
//! // As a boolean literal, with empty content
//! fn std_is_enabled() -> bool {
//!     cfg::std!()
//! }
//!
//! // Control whether the internal code is active
//! cfg::std!{
//!     extern crate std;
//! }
//!
//! // Conditional branching
//! cfg::std!{
//!     if {
//!         mod std_impls{ /* ... */}
//!     } else {
//!         mod no_std_impls{ /* ... */}
//!     }
//! }
//!
//! // Switch-like pattern matching
//! cfg::switch!{
//!     cfg::std => {
//!         mod std_impl{ /* .. */ }
//!     }
//!     #[cfg(debug_assertions)] => {
//!         mod debug_impl{ /* .. */ }
//!     }
//!     _ => {
//!         /* ... */
//!     }
//! }
//! ```
//!
//! See more information in:
//! - [`disabled`]: Represents a disabled compilation option.
//! - [`enabled`]: Represents an enabled compilation option.
//! - [`switch`]: A conditional compilation macro similar to a `switch` statement.
//! - [`define_alias`]: Define aliases for compilation options.
//!
//! [`disabled`]: crate::disabled
//! [`enabled`]: crate::enabled
//! [`switch`]: crate::switch
//! [`define_alias`]: crate::define_alias
#![no_std]

// ----------------------------------------------------------------------------
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
        $crate::switch! {
            #[cfg($meta)] => {
                #[doc = concat!("An alias for `#[cfg(", stringify!($meta), ")]` .\n")]
                #[doc = "See [`zlim_cfg::define_alias`] for details."]
                $(#[$id_meta])*
                pub use $crate::enabled as $id;
            }
            _ => {
                #[doc = concat!("An alias for `#[cfg(", stringify!($meta), ")]` .\n")]
                #[doc = "See [`zlim_cfg::define_alias`] for details."]
                $(#[$id_meta])*
                pub use $crate::disabled as $id;
            }
        }
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
