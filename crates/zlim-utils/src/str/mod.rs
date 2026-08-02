//! Small-string optimization, pre-hashed strings, and global string interning.
//!
//! This module provides:
//!
//! - [`SmolStr`] — a small-buffer-optimized, immutable string type that wraps
//!   the [`smol_str`] crate. Strings up to 23 bytes are stack-allocated;
//!   longer strings are heap-allocated. `Clone` is `O(1)`.
//!
//! - [`HashStr`] — a [`SmolStr`] with a pre-computed hash, enabling `O(1)`
//!   equality checks and fast hashing. Ideal for use as keys in hash-based
//!   collections.
//!
//! - [`intern_str`] — interns a `&str` into a `&'static str` via a global
//!   read-optimised pool.
//!
//! - [`format_smol!`] — a macro for creating a [`SmolStr`] via `format_args!`.

// ----------------------------------------------------------------------------
// Modules

mod hash;
mod pool;
mod smol;

// ----------------------------------------------------------------------------
// Exports

pub use hash::HashStr;
pub use pool::intern_str;
pub use smol::SmolStr;

#[macro_export]
macro_rules! format_smol {
    ($($tt:tt)*) => {{
        let mut w = $crate::str::__private::__SmolStrBuilder::new();
        ::core::fmt::Write::write_fmt(&mut w, ::core::format_args!($($tt)*))
            .expect("a formatting trait implementation returned an error");
        $crate::str::SmolStr::from(w.finish())
    }};
}

#[doc(hidden)]
pub mod __private {
    pub use smol_str::SmolStrBuilder as __SmolStrBuilder;
}

// ----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    #[test]
    fn format_macro() {
        let s = format_smol!("1 + 2 = {}", 1 + 2);
        assert_eq!(s.as_str(), "1 + 2 = 3");
    }
}
