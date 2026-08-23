//! Implementation for the `job_fn` attribute macro and the `job!`
//! function-like macro.
//!
//! Both macros generate a marker type implementing `JobLabel` plus a
//! `JobDB` descriptor:
//!
//! - The `job_fn` attribute macro marks a function; the marker type's
//!   generics are taken from the `type = Name<GENERICS>` declaration when
//!   present, and from the function itself otherwise.
//! - The `job!` macro marks an arbitrary system expression
//!   (typically built with `IntoSystem::pipe`); the marker type's generics
//!   are declared in the `type: ...` argument.
//!
//! For non-generic markers, the database registers itself through
//! `zlim_reg::submit!` at program startup.  Generic markers cannot be
//! auto-registered and must be registered manually with
//! `JobDB::register`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{GenericParam, Generics, Ident, ItemFn, LitStr, Token, parse_quote};

// -----------------------------------------------------------------------------
// Inputs

/// Parsed `#[job(type = ..., name = ..., strict = ...)]` attribute arguments.
pub(crate) struct JobAttr {
    pub type_ident: Ident,
    /// Generics declared in `type = Name<GENERICS>`.  Empty when the
    /// attribute does not declare generics (the function's own generics are
    /// used instead).
    pub generics: Generics,
    pub name: LitStr,
    /// Whether the generated job registers its access strictly.  Defaults
    /// to `true`.
    pub strict: bool,
}

/// Parses the `type` argument (`Name` or `Name<GENERICS>`) of both macros.
pub(crate) fn parse_type_spec(input: ParseStream) -> syn::Result<(Ident, Generics)> {
    let ident: Ident = input.parse()?;

    let mut params = Punctuated::<GenericParam, Token![,]>::new();
    let mut lt_token = None;
    let mut gt_token = None;

    if input.peek(Token![<]) {
        lt_token = Some(input.parse::<Token![<]>()?);

        while !input.peek(Token![>]) {
            params.push(input.parse()?);

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        gt_token = Some(input.parse::<Token![>]>()?);
    }

    Ok((
        ident,
        Generics {
            lt_token,
            params,
            gt_token,
            where_clause: None,
        },
    ))
}

impl Parse for JobAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut type_spec = None;
        let mut name = None;
        let mut strict = true;

        while !input.is_empty() {
            let key = input.call(Ident::parse_any)?;
            input.parse::<Token![=]>()?;

            match key.to_string().as_str() {
                "type" => type_spec = Some(parse_type_spec(input)?),
                "name" => name = Some(input.parse()?),
                "strict" => strict = input.parse::<syn::LitBool>()?.value(),
                _ => {
                    return Err(syn::Error::new_spanned(
                        key,
                        "expected `type`, `name`, or `strict`",
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        let (type_ident, generics) =
            type_spec.ok_or_else(|| syn::Error::new(input.span(), "missing `type` argument"))?;
        let name = name.ok_or_else(|| syn::Error::new(input.span(), "missing `name` argument"))?;

        Ok(Self {
            type_ident,
            generics,
            name,
            strict,
        })
    }
}

/// Parsed `job! { type: ..., name: ..., system: ..., strict: ... }` input.
pub(crate) struct DefineJobInput {
    pub type_ident: Ident,
    pub generics: Generics,
    pub name: LitStr,
    pub system: syn::Expr,
    /// Whether the generated job registers its access strictly.  Defaults
    /// to `true`.
    pub strict: bool,
}

impl Parse for DefineJobInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut type_spec = None;
        let mut name = None;
        let mut system = None;
        let mut strict = true;

        while !input.is_empty() {
            let key = input.call(Ident::parse_any)?;
            input.parse::<Token![:]>()?;

            match key.to_string().as_str() {
                "type" => type_spec = Some(parse_type_spec(input)?),
                "name" => name = Some(input.parse()?),
                "system" => system = Some(input.parse()?),
                "strict" => strict = input.parse::<syn::LitBool>()?.value(),
                _ => {
                    return Err(syn::Error::new_spanned(
                        key,
                        "expected `type`, `name`, `system`, or `strict`",
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        let (type_ident, generics) =
            type_spec.ok_or_else(|| syn::Error::new(input.span(), "missing `type` argument"))?;
        let name = name.ok_or_else(|| syn::Error::new(input.span(), "missing `name` argument"))?;
        let system =
            system.ok_or_else(|| syn::Error::new(input.span(), "missing `system` argument"))?;

        Ok(Self {
            type_ident,
            generics,
            name,
            system,
            strict,
        })
    }
}

// -----------------------------------------------------------------------------
// Expansion

/// Common inputs for both macros.
struct JobInput {
    type_ident: Ident,
    generics: Generics,
    name: LitStr,
    /// Whether the generated job registers its access strictly.
    strict: bool,
    /// The expression passed to `IntoJob::into_job` as the system.
    ctor: TokenStream,
}

/// Generates the marker struct and its `JobLabel` implementation.
fn expand_common(input: JobInput) -> TokenStream {
    let zlim_core = crate::path::zlim_core_path();
    let into_job_ = crate::path::into_job_(&zlim_core);
    let job_trait_ = crate::path::job_trait_(&zlim_core);
    let job_db_ = crate::path::job_db_(&zlim_core);
    let job_label_ = crate::path::job_label_(&zlim_core);
    let debug_location_ = crate::path::debug_location_(&zlim_core);
    let type_path_trait_ = crate::path::type_path_trait_(&zlim_core);
    let type_path_derive_ = crate::path::type_path_derive_(&zlim_core);
    let submit_ = crate::path::submit_(&zlim_core);

    let JobInput {
        type_ident,
        mut generics,
        name,
        strict,
        ctor,
    } = input;

    // The `STRICT` const generic of `IntoJob::into_job` selects between
    // `StrictJobSystem` and the non-strict `JobSystem` wrapper.
    let strict_arg = if strict {
        quote! { true }
    } else {
        quote! { false }
    };

    let type_params: Vec<Ident> = generics
        .type_params()
        .map(|param| param.ident.clone())
        .collect();
    let generic = !generics.params.is_empty();

    // Generic markers need `T: TypePath` so the job name can be derived
    // from the marker type itself.
    if generic {
        let where_clause = generics.make_where_clause();
        for param in &type_params {
            where_clause
                .predicates
                .push(parse_quote!(#param: #type_path_trait_));
        }
    }

    let (impl_g, ty_g, where_g) = generics.split_for_impl();

    let phantom = if type_params.is_empty() {
        quote! { ::core::marker::PhantomData<()> }
    } else {
        quote! { ::core::marker::PhantomData<( #(#type_params,)* )> }
    };

    let struct_item = if generic {
        quote! {
            #[derive(#type_path_derive_)]
            #[type_path = #name]
            pub struct #type_ident #ty_g (#phantom) #where_g;
        }
    } else {
        quote! { pub struct #type_ident (#phantom); }
    };

    let name_expr = if generic {
        quote! { <Self as #type_path_trait_>::type_path() }
    } else {
        quote! { #name }
    };

    let database_fn = if generic {
        quote! {
            fn database() -> #job_db_ {
                #job_db_ {
                    name: #name_expr,
                    ctor: |group: &'static str| -> ::std::boxed::Box<dyn #job_trait_> {
                        #into_job_::into_job::<#strict_arg>(#ctor, #name_expr, group)
                    },
                    location: #debug_location_::caller(),
                }
            }
        }
    } else {
        quote! {
            fn database() -> #job_db_ {
                const DB: #job_db_ = #job_db_ {
                    name: #name_expr,
                    ctor: |group: &'static str| -> ::std::boxed::Box<dyn #job_trait_> {
                        #into_job_::into_job::<#strict_arg>(#ctor, #name_expr, group)
                    },
                    location: #debug_location_::caller(),
                };

                #submit_!(DB => #job_db_);

                DB
            }
        }
    };

    quote! {
        #struct_item

        const _: () = {
            impl #impl_g #job_label_ for #type_ident #ty_g #where_g {
                fn name() -> &'static str {
                    #name_expr
                }

                #database_fn
            }
        };
    }
}

// -----------------------------------------------------------------------------
// Attribute macro

/// Expands `#[job(type = ..., name = ..., strict = ...)]` on a function.
pub(crate) fn expand_attr(attr: JobAttr, mut item: ItemFn) -> syn::Result<TokenStream> {
    let JobAttr {
        type_ident,
        generics: declared,
        name,
        strict,
    } = attr;

    // Use the generics declared in `type = Name<GENERICS>` when present;
    // otherwise fall back to the function's own generics.
    let generics = if declared.params.is_empty() {
        item.sig.generics.clone()
    } else {
        declared
    };

    if let Some(lifetime) = generics.lifetimes().next() {
        return Err(syn::Error::new_spanned(
            lifetime,
            "`job_fn` does not support lifetime parameters",
        ));
    }

    let fn_ident = &item.sig.ident;

    let ctor = if generics.params.is_empty() {
        quote! { #fn_ident }
    } else {
        let args: Vec<TokenStream> = generics
            .params
            .iter()
            .map(|param| match param {
                GenericParam::Type(param) => {
                    let ident = &param.ident;
                    quote! { #ident }
                }
                GenericParam::Const(param) => {
                    let ident = &param.ident;
                    quote! { #ident }
                }
                GenericParam::Lifetime(_) => unreachable!(),
            })
            .collect();

        quote! { #fn_ident::< #(#args),* > }
    };

    // Remove the `#[job(...)]` attribute before re-emitting the function,
    // otherwise the macro would be expanded again on its own output.
    item.attrs.retain(|attr| !attr.path().is_ident("job_fn"));

    let expanded = expand_common(JobInput {
        type_ident,
        generics,
        name,
        strict,
        ctor,
    });

    Ok(quote! {
        #expanded

        #item
    })
}

// -----------------------------------------------------------------------------
// job! macro

/// Expands `job! { type: ..., name: ..., system: ..., strict: ... }`.
pub(crate) fn expand_define(input: DefineJobInput) -> TokenStream {
    let DefineJobInput {
        type_ident,
        generics,
        name,
        system,
        strict,
    } = input;

    expand_common(JobInput {
        type_ident,
        generics,
        name,
        strict,
        ctor: quote! { #system },
    })
}
