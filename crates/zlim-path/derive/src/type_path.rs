//! `#[derive(TypePath)]` — standalone derive macro for the [`TypePath`] trait.
//!
//! This module is intentionally independent of the [`Reflect`] derive machinery
//! so that users who only need stable type-path identifiers don't pay the
//! compile-time cost of the full reflection derive.
//!
//! [`TypePath`]: zlim_path::TypePath

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::spanned::Spanned;
use syn::{Attribute, DeriveInput, Expr, ExprLit, ImplGenerics, Path, TypeGenerics};
use syn::{GenericParam, Generics, Ident, Lit, Meta};

use crate::string_expr::StringExpr;

// -----------------------------------------------------------------------------
// Attribute parsing — `#[type_path = "..."]`
// -----------------------------------------------------------------------------

/// Parsed `#[type_path = "..."]` attribute.
struct CustomPath {
    /// Custom full type path, e.g. `my_crate::foo::Bar`.
    path: Option<Path>,
}

impl CustomPath {
    /// Extract `#[type_path = "..."]` from the given attribute slice.
    fn parse(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut result = Self { path: None };

        for attr in attrs {
            if let Meta::NameValue(pair) = &attr.meta
                && pair.path.is_ident("type_path")
            {
                result.parse_custom_path(pair)?;
            }
        }

        Ok(result)
    }

    fn parse_custom_path(&mut self, pair: &syn::MetaNameValue) -> syn::Result<()> {
        if let Expr::Lit(ExprLit { lit, .. }) = &pair.value
            && let Lit::Str(lit) = lit
        {
            let path: Path = syn::parse_str(&lit.value())?;

            if path.segments.is_empty() {
                const MSG: &str = "`type_path` attribute must not be empty.";
                return Err(syn::Error::new(lit.span(), MSG));
            }

            if path.leading_colon.is_some() {
                const MSG: &str = "`type_path` should not have a leading `::`.";
                return Err(syn::Error::new(lit.span(), MSG));
            }

            self.path = Some(path);
            Ok(())
        } else {
            const MSG: &str = "`#[type_path = \"...\"]` expects a string literal.";
            Err(syn::Error::new(pair.value.span(), MSG))
        }
    }
}

// -----------------------------------------------------------------------------
// Entry point
// -----------------------------------------------------------------------------

/// Generate the `TypePath` impl from the parsed derive input.
pub(crate) fn expand_type_path(input: &DeriveInput, zlim_path: &Path) -> TokenStream {
    let custom = match CustomPath::parse(&input.attrs) {
        Ok(c) => c,
        Err(e) => return e.into_compile_error(),
    };

    let ident = &input.ident;
    let generics = &input.generics;

    let has_generics = generics
        .params
        .iter()
        .any(|p| !matches!(p, GenericParam::Lifetime(_)));

    let Bodies {
        type_path,
        type_name,
        ident_,
        module,
        crate_name,
    } = if let Some(custom_path) = &custom.path {
        expand_with_custom_path(custom_path, has_generics, zlim_path, generics)
    } else {
        expand_default(ident, has_generics, zlim_path, generics)
    };

    let (impl_gen, ty_gen, where_clause) = split_generics_for_type_path(generics, zlim_path);

    let type_path_trait = quote! { #zlim_path::TypePath };
    let optional_inline = (!has_generics).then(|| quote! { #[inline] });

    quote! {
        const _:() = {
            #[automatically_derived]
            impl #impl_gen #type_path_trait for #ident #ty_gen #where_clause {
                #optional_inline
                fn type_path() -> &'static str {
                    #type_path
                }

                #optional_inline
                fn type_name() -> &'static str {
                    #type_name
                }

                const IDENT: &str = #ident_;
                const MODULE: ::core::option::Option<&str> = #module;
                const CRATE: ::core::option::Option<&str> = #crate_name;
            }
        };
    }
}

// -----------------------------------------------------------------------------
// Bodies — the five generated expressions
// -----------------------------------------------------------------------------

struct Bodies {
    type_path: TokenStream,
    type_name: TokenStream,
    ident_: TokenStream,
    module: TokenStream,
    crate_name: TokenStream,
}

// -----------------------------------------------------------------------------
// Custom path expansion
// -----------------------------------------------------------------------------

fn expand_with_custom_path(
    custom_path: &Path,
    has_generics: bool,
    zlim_path: &Path,
    generics: &Generics,
) -> Bodies {
    let custom_ident = &custom_path.segments.last().unwrap().ident;
    let custom_ident_lit = custom_ident.to_string().to_token_stream();

    // module_path = everything except the last segment
    let module_prefix_len = custom_path.segments.len().saturating_sub(1);
    let module_prefix: String = custom_path
        .segments
        .iter()
        .take(module_prefix_len)
        .map(|s| s.ident.to_string())
        .reduce(|a, b| a + "::" + &b)
        .unwrap_or_default();

    let (module_body, crate_name_body) = if module_prefix.is_empty() {
        (
            quote! { ::core::option::Option::None },
            quote! { ::core::option::Option::None },
        )
    } else {
        let module_lit: &str = &module_prefix;
        let crate_lit: String = custom_path.segments[0].ident.to_string();
        (
            quote! { ::core::option::Option::Some(#module_lit) },
            quote! { ::core::option::Option::Some(#crate_lit) },
        )
    };

    if !has_generics {
        let full_path = custom_path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .reduce(|a, b| a + "::" + &b)
            .expect("path must have at least one segment");

        return Bodies {
            type_path: full_path.to_token_stream(),
            type_name: custom_ident_lit.clone(),
            ident_: custom_ident_lit,
            module: module_body,
            crate_name: crate_name_body,
        };
    }

    // --------------------------------------
    // has_generics

    let path_cell = quote! { #zlim_path::PathCell };

    // type_path prefix:  "module::Ident<"
    let tp_prefix: Vec<StringExpr> = if module_prefix.is_empty() {
        vec![
            StringExpr::Literal(custom_ident_lit.clone()),
            StringExpr::from_str("<"),
        ]
    } else {
        vec![
            StringExpr::from_str(&module_prefix),
            StringExpr::from_str("::"),
            StringExpr::Literal(custom_ident_lit.clone()),
            StringExpr::from_str("<"),
        ]
    };

    // type_name prefix:  "Ident<"
    let tn_prefix: Vec<StringExpr> = vec![
        StringExpr::from_val(custom_ident.clone()),
        StringExpr::from_str("<"),
    ];

    Bodies {
        type_path: build_generic_concat(tp_prefix, generics, zlim_path, "type_path", &path_cell),
        type_name: build_generic_concat(tn_prefix, generics, zlim_path, "type_name", &path_cell),
        ident_: custom_ident_lit,
        module: module_body,
        crate_name: crate_name_body,
    }
}

// -----------------------------------------------------------------------------
// Default-path expansion
// -----------------------------------------------------------------------------

fn expand_default(
    ident: &Ident,
    has_generics: bool,
    zlim_path: &Path,
    generics: &Generics,
) -> Bodies {
    let ident_lit = ident.to_string().to_token_stream();

    let module_body = quote! { ::core::option::Option::Some(::core::module_path!()) };

    let crate_name_body = quote! {
        ::core::option::Option::Some(::core::env!("CARGO_CRATE_NAME"))
    };

    if !has_generics {
        return Bodies {
            type_path: quote! { ::core::concat!(::core::module_path!(), "::", #ident_lit) },
            type_name: ident_lit.clone(),
            ident_: ident_lit,
            module: module_body,
            crate_name: crate_name_body,
        };
    }

    let path_cell = quote! { #zlim_path::PathCell };

    // type_path prefix:  "module_path!()::Ident<"
    let tp_prefix: Vec<StringExpr> = vec![
        StringExpr::Literal(quote! { ::core::module_path!() }),
        StringExpr::from_str("::"),
        StringExpr::from_val(ident.clone()),
        StringExpr::from_str("<"),
    ];

    // type_name prefix:  "Ident<"
    let tn_prefix: Vec<StringExpr> = vec![
        StringExpr::from_val(ident.clone()),
        StringExpr::from_str("<"),
    ];

    Bodies {
        type_path: build_generic_concat(tp_prefix, generics, zlim_path, "type_path", &path_cell),
        type_name: build_generic_concat(tn_prefix, generics, zlim_path, "type_name", &path_cell),
        ident_: ident_lit,
        module: module_body,
        crate_name: crate_name_body,
    }
}

// -----------------------------------------------------------------------------
// Generic concat builder
// -----------------------------------------------------------------------------

/// Build a `PathCell`-backed expression for a generic type's `type_path()` or
/// `type_name()` using [`StringExpr::from_iter`] to handle the composition.
fn build_generic_concat(
    mut prefix: Vec<StringExpr>,
    generics: &Generics,
    zlim_path: &Path,
    method: &str,
    cell: &TokenStream,
) -> TokenStream {
    let type_path_trait = quote! { #zlim_path::TypePath };

    let method = Ident::new(method, proc_macro2::Span::call_site());

    let mut is_first = true;

    for param in generics.params.iter() {
        match param {
            GenericParam::Type(tp) => {
                if !is_first {
                    prefix.push(StringExpr::from_str(", "));
                }
                let t = &tp.ident;
                let s = quote! { <#t as #type_path_trait>::#method() };
                prefix.push(StringExpr::Static(s));
                is_first = false;
            }
            GenericParam::Const(cp) => {
                if !is_first {
                    prefix.push(StringExpr::from_str(", "));
                }
                let c = &cp.ident;
                let s = quote! { ::std::string::ToString::to_string(&#c) };
                prefix.push(StringExpr::Owned(s));
                is_first = false;
            }
            _ => { /* Lifetime - do nothing */ }
        }
    }

    prefix.push(StringExpr::from_str(">"));

    let body = StringExpr::from_iter(prefix, zlim_path).into_owned();

    quote! {
        static CELL: #cell = #cell::new();
        CELL.get_or_init::<Self>(|| #body)
    }
}

// -----------------------------------------------------------------------------
// Generic splitting for the impl header
// -----------------------------------------------------------------------------

/// Produces `(impl_generics, ty_generics, where_clause)` with a `TypePath`
/// bound added for every type parameter.
fn split_generics_for_type_path<'a>(
    generics: &'a Generics,
    zlim_path: &Path,
) -> (ImplGenerics<'a>, TypeGenerics<'a>, TokenStream) {
    let (impl_ge, ty_gen, z) = generics.split_for_impl();

    if generics.type_params().next().is_none() {
        let wc = z.map(|w| w.to_token_stream()).unwrap_or_default();
        return (impl_ge, ty_gen, wc);
    }

    let trailing_punct = z.map(|w| w.predicates.trailing_punct()).unwrap_or(false);
    let mut where_clause = z.map(|w| w.to_token_stream()).unwrap_or_default();

    let type_path_trait = quote! { #zlim_path::TypePath };

    let predicates = generics.type_params().map(|tp| {
        let t = &tp.ident;
        quote! { #t: #type_path_trait }
    });

    if where_clause.is_empty() {
        where_clause = quote! { where #(#predicates),* };
    } else if trailing_punct {
        where_clause = quote! { #where_clause #(#predicates),* };
    } else {
        where_clause = quote! { #where_clause, #(#predicates),* };
    }

    (impl_ge, ty_gen, where_clause)
}

// -----------------------------------------------------------------------------
