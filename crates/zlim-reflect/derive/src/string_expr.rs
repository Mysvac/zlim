use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::LitStr;
use syn::spanned::Spanned;

/// An enum representing different types of string expressions
#[derive(Clone)]
pub(crate) enum StringExpr {
    /// A string that is valid at compile time.
    ///
    /// In most cases, this is a string lit, such as: `"mystring"`.
    ///
    /// But sometimes, this also includes macros, such as: `module_path!(xxx)`
    Literal(TokenStream),
    /// A [string slice](str) that is borrowed for a `'static` lifetime.
    ///
    /// For example: `a`, a is a `&'static str`.
    Static(TokenStream),
    /// An [owned string](String).
    ///
    /// For example: `a`, a is a [`String`].
    Owned(TokenStream),
}

impl Default for StringExpr {
    fn default() -> Self {
        Self::Literal("".to_token_stream())
    }
}

impl StringExpr {
    /// Creates a [literal] [`StringExpr`] from a [str].
    ///
    /// [literal]: StringExpr::Literal
    pub fn from_str(string: &str) -> Self {
        // ↓ Generate tokens with string literal.
        Self::Literal(string.to_token_stream())
    }

    /// Creates a [literal] [`StringExpr`] from a value.
    ///
    /// [literal]: StringExpr::Literal
    pub fn from_val<T: ToString + Spanned>(value: T) -> Self {
        let span = value.span();
        let value = value.to_string();
        Self::Literal(LitStr::new(&value, span).to_token_stream())
    }

    /// Get inner TokenStream if self is const string expr.
    ///
    /// # Panic
    /// - self is not const string expr.
    pub fn into_literal(self) -> TokenStream {
        match self {
            StringExpr::Literal(token_stream) => token_stream,
            _ => unreachable!(), // See: `StringExpr::from_iter`
        }
    }

    /// Returns tokens for a statically borrowed [string slice](str).
    pub fn into_borrowed(self) -> TokenStream {
        match self {
            Self::Literal(t) | Self::Static(t) => t,
            Self::Owned(owned) => quote!(&#owned as &str),
        }
    }

    /// Returns tokens for an [owned string](String).
    pub fn into_owned(self) -> TokenStream {
        match self {
            Self::Literal(t) | Self::Static(t) => {
                quote! { ::std::borrow::ToOwned::to_owned(#t) }
            }
            Self::Owned(owned) => owned,
        }
    }

    /// concat string from iterator
    ///
    /// If all expressions are [`StringExpr::Literal`] this will use [`concat`] to merge them.
    pub fn from_iter(iter: impl IntoIterator<Item = StringExpr>, zlim_reflect: &syn::Path) -> Self {
        let exprs: Vec<StringExpr> = iter.into_iter().collect();

        if exprs.is_empty() {
            return Self::default();
        }

        if exprs.iter().all(|s| matches!(s, Self::Literal(_))) {
            let inner = exprs.into_iter().map(StringExpr::into_literal); // `exprs` will not be empty here.

            Self::Literal(quote!(::core::concat!( #(#inner),* )))
        } else {
            let concat_fn = crate::path::concat_fn(zlim_reflect);
            let inner = exprs.into_iter().map(StringExpr::into_borrowed);

            Self::Owned(quote!(#concat_fn(&[ #(#inner),* ])))
        }
    }
}
