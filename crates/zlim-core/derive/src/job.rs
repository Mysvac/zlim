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
//! The marker type always derives `TypePath`.  The job name — returned by
//! `JobLabel::name()` and stored in the `JobDB` — is the marker's type path:
//! the `name = "..."` argument overrides it via `#[type_path = "..."]`; when
//! omitted it is `<Self as TypePath>::type_path()` (i.e.
//! `module_path!()::TypeName`).
//!
//! Optional `run_if` expressions gate the job: each is a system returning
//! `bool` / `Result<bool, E>` and becomes a condition job named
//! `"{job}#run_if<{ordinal}>#{expression}"` (0-based ordinal; the expression
//! string is truncated to 15 characters) and interned for `'static`.  The
//! condition constructors — each taking the job's group name — are stored in
//! the `JobDB::run_if` slice.
//!
//! For non-generic markers, the database registers itself through
//! `zlim_reg::submit!` at program startup.  Generic markers cannot be
//! auto-registered and must be registered manually with
//! `JobDB::register`.

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Attribute, GenericParam, Generics, Ident, ItemFn, LitStr, Token, parse_quote};

// -----------------------------------------------------------------------------
// Inputs

/// Parses the `run_if` argument: a single expression or a bracketed list of
/// expressions.
fn parse_run_if(input: ParseStream) -> syn::Result<Vec<syn::Expr>> {
    if input.peek(syn::token::Bracket) {
        let content;
        syn::bracketed!(content in input);
        let exprs = content.parse_terminated(syn::Expr::parse, Token![,])?;
        Ok(exprs.into_iter().collect())
    } else {
        Ok(vec![input.parse()?])
    }
}

/// Parsed `#[job(type = ..., name = ..., run_if = ..., strict = ...)]`
/// attribute arguments.
pub(crate) struct JobAttr {
    pub type_ident: Ident,
    /// Generics declared in `type = Name<GENERICS>`.  Empty when the
    /// attribute does not declare generics (the function's own generics are
    /// used instead).
    pub generics: Generics,
    /// Custom job name.  When `None`, the job name is the marker type's
    /// `TypePath` (`module_path!()::TypeName`).
    pub name: Option<LitStr>,
    /// Run conditions: systems returning `bool` / `Result<bool, E>` that
    /// gate this job.  Empty when `run_if` is omitted.
    pub run_if: Vec<syn::Expr>,
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
        let mut run_if = Vec::new();
        let mut strict = true;

        while !input.is_empty() {
            let key = input.call(Ident::parse_any)?;
            input.parse::<Token![=]>()?;

            match key.to_string().as_str() {
                "type" => type_spec = Some(parse_type_spec(input)?),
                "name" => name = Some(input.parse()?),
                "run_if" => run_if = parse_run_if(input)?,
                "strict" => strict = input.parse::<syn::LitBool>()?.value(),
                _ => {
                    return Err(syn::Error::new_spanned(
                        key,
                        "expected `type`, `name`, `run_if`, or `strict`",
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        let (type_ident, generics) =
            type_spec.ok_or_else(|| syn::Error::new(input.span(), "missing `type` argument"))?;

        Ok(Self {
            type_ident,
            generics,
            name,
            run_if,
            strict,
        })
    }
}

/// Parsed `job! { type: ..., name: ..., run_if: ..., system: ..., strict: ... }` input.
pub(crate) struct DefineJobInput {
    pub type_ident: Ident,
    pub generics: Generics,
    /// Custom job name.  When `None`, the job name is the marker type's
    /// `TypePath` (`module_path!()::TypeName`).
    pub name: Option<LitStr>,
    /// Run conditions: systems returning `bool` / `Result<bool, E>` that
    /// gate this job.  Empty when `run_if` is omitted.
    pub run_if: Vec<syn::Expr>,
    pub system: syn::Expr,
    /// Whether the generated job registers its access strictly.  Defaults
    /// to `true`.
    pub strict: bool,
}

impl Parse for DefineJobInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut type_spec = None;
        let mut name = None;
        let mut run_if = Vec::new();
        let mut system = None;
        let mut strict = true;

        while !input.is_empty() {
            let key = input.call(Ident::parse_any)?;
            input.parse::<Token![:]>()?;

            match key.to_string().as_str() {
                "type" => type_spec = Some(parse_type_spec(input)?),
                "name" => name = Some(input.parse()?),
                "run_if" => run_if = parse_run_if(input)?,
                "system" => system = Some(input.parse()?),
                "strict" => strict = input.parse::<syn::LitBool>()?.value(),
                _ => {
                    return Err(syn::Error::new_spanned(
                        key,
                        "expected `type`, `name`, `run_if`, `system`, or `strict`",
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        let (type_ident, generics) =
            type_spec.ok_or_else(|| syn::Error::new(input.span(), "missing `type` argument"))?;
        let system =
            system.ok_or_else(|| syn::Error::new(input.span(), "missing `system` argument"))?;

        Ok(Self {
            type_ident,
            generics,
            name,
            run_if,
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
    /// Custom job name; `None` falls back to the marker's `TypePath`.
    name: Option<LitStr>,
    /// Run conditions gating this job; empty when `run_if` is omitted.
    run_if: Vec<syn::Expr>,
    /// Whether the generated job registers its access strictly.
    strict: bool,
    /// The expression passed to `IntoJob::into_job` as the system.
    ctor: TokenStream,
    /// `#[doc]` attributes forwarded from the annotated function to the
    /// generated marker type (empty for `job!`, which has no source).
    doc_attrs: Vec<Attribute>,
}

/// Generates the marker struct and its `JobLabel` implementation.
fn expand_common(input: JobInput) -> TokenStream {
    let zlim_core = crate::path::zlim_core_path();
    let into_job_ = crate::path::into_job_(&zlim_core);
    let job_trait_ = crate::path::job_trait_(&zlim_core);
    let job_db_ = crate::path::job_db_(&zlim_core);
    let job_label_ = crate::path::job_label_(&zlim_core);
    let job_reg_ = crate::path::job_reg_(&zlim_core);
    let debug_location_ = crate::path::debug_location_(&zlim_core);
    let type_path_trait_ = crate::path::type_path_trait_(&zlim_core);
    let type_path_derive_ = crate::path::type_path_derive_(&zlim_core);
    let intern_str_ = crate::path::intern_str_(&zlim_core);
    let slice_pool_ = crate::path::slice_pool_(&zlim_core);
    let submit_ = crate::path::submit_(&zlim_core);

    let JobInput {
        type_ident,
        mut generics,
        name,
        run_if,
        strict,
        ctor,
        doc_attrs,
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

    // The marker type always derives `TypePath`; an explicit `name` becomes
    // its custom path, otherwise the path defaults to
    // `module_path!()::TypeName`.
    let type_path_attr = name.as_ref().map(|name| quote! { #[type_path = #name] });

    let struct_item = quote! {
        #(#doc_attrs)*
        #[derive(#type_path_derive_)]
        #type_path_attr
        pub struct #type_ident #ty_g (#phantom) #where_g;
    };

    // The job name is always the marker's `TypePath`.
    let name_expr: TokenStream = quote! { <Self as #type_path_trait_>::type_path() };
    // The database defers to `name()`, so it can never drift from the label.
    let db_name_expr: TokenStream = quote! { <Self as #job_label_>::name() };

    fn debug_expr(expr: &syn::Expr) -> String {
        let expr_str = expr.to_token_stream().to_string();
        if expr_str.len() <= 15 {
            return expr_str;
        }
        let mut expr_name = String::with_capacity(15);
        for c in expr_str.chars() {
            if expr_name.len() + c.len_utf8() <= 13 {
                expr_name.push(c);
            } else {
                break;
            };
        }
        expr_name.push_str("..");
        expr_name
    }

    // One condition constructor per `run_if` expression.  Each condition is
    // a job named `"{job}#run_if<{ordinal}>#{expression}"` (interned for
    // `'static`) and built on demand through the same `IntoJob` bridge as
    // the job itself — the caller passes the job's group name explicitly.
    // The ordinal is the 0-based position within the `run_if` list; the
    // expression string is truncated to 15 characters.
    let run_if_ctors: Vec<TokenStream> = run_if
        .iter()
        .enumerate()
        .map(|(index, expr)| {
            let expr_str = debug_expr(expr);

            quote! {
                |group: &'static str| -> ::std::boxed::Box<dyn #job_trait_> {
                    #into_job_::into_job::<#strict_arg>(
                        #expr,
                        #intern_str_(&::std::format!(
                            "{}#run_if<{}>#{}",
                            <Self as #job_label_>::name(),
                            #index,
                            #expr_str,
                        )),
                        group,
                    )
                }
            }
        })
        .collect();

    let run_if_field: TokenStream = if run_if_ctors.is_empty() {
        quote! { &[] }
    } else {
        quote! { #slice_pool_::run_if(&[ #(#run_if_ctors),* ]) }
    };

    let database_fn: TokenStream = quote! {
        fn database() -> #job_db_ {
            #job_db_ {
                name: #db_name_expr,
                ctor: |group: &'static str| -> ::std::boxed::Box<dyn #job_trait_> {
                    #into_job_::into_job::<#strict_arg>(#ctor, #db_name_expr, group)
                },
                run_if: #run_if_field,
                location: #debug_location_::caller(),
            }
        }
    };

    let registration: Option<TokenStream> = (!generic).then(|| {
        quote! {
            #submit_!(#job_reg_::of::<#type_ident>() => #job_reg_);
        }
    });

    quote! {
        #struct_item

        const _: () = {
            #registration

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
        run_if,
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

    // Forward the function's doc comments (`/// ...` / `#[doc = ...]`) to
    // the generated marker type, so `MyJob` is documented like the
    // function.  Other attributes are left on the function itself.
    let doc_attrs: Vec<Attribute> = item
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .cloned()
        .collect();

    let expanded = expand_common(JobInput {
        type_ident,
        generics,
        name,
        run_if,
        strict,
        ctor,
        doc_attrs,
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
        run_if,
        system,
        strict,
    } = input;

    expand_common(JobInput {
        type_ident,
        generics,
        name,
        run_if,
        strict,
        ctor: quote! { #system },
        doc_attrs: Vec::new(),
    })
}
