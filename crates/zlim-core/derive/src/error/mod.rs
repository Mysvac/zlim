//! Implementation of the `#[derive(Error)]` proc-macro.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, Ident};

use crate::path;

// ----------------------------------------------------------------------------
// Internal expansion

pub fn expand(input: &DeriveInput) -> TokenStream {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Parse custom attributes (preserved via `attributes(error, zlim_error)`).
    let type_error = find_error_attr(&input.attrs);
    let type_zlim_error = match parse_zlim_error_attr(&input.attrs) {
        Ok(sev) => sev,
        Err(e) => return e.into_compile_error(),
    };

    // 1. Always: core::error::Error
    let error_impl = quote! {
        impl #impl_generics ::core::error::Error for #name #ty_generics #where_clause {}
    };

    // 2. Optional: Display
    let display_impl = match gen_display(input, &type_error) {
        Ok(ts) => ts,
        Err(e) => return e.into_compile_error(),
    };

    // 3. Optional: Into<ZlimError> via From impl
    let zlim_impls = match gen_zlim_into(input, name, &type_zlim_error) {
        Ok(ts) => ts,
        Err(e) => return e.into_compile_error(),
    };

    quote! {
        const _:() = {
            #error_impl
            #display_impl
            #zlim_impls
        };
    }
}

// ----------------------------------------------------------------------------
// Attribute parsing

/// Extract `#[error("format string")]` or `#[error("fmt", extra...)]`.
fn find_error_attr(attrs: &[syn::Attribute]) -> Option<TokenStream> {
    for attr in attrs {
        if attr.path().is_ident("error") {
            return attr.parse_args::<TokenStream>().ok();
        }
    }
    None
}

/// Parse `#[zlim_error(severity)]`.  Returns an error for invalid severity
/// values so the user gets a clear compile-time diagnostic.
fn parse_zlim_error_attr(attrs: &[syn::Attribute]) -> Result<Option<Ident>, syn::Error> {
    for attr in attrs {
        if attr.path().is_ident("zlim_error") {
            let severity: Ident = attr.parse_args().map_err(|_| {
                syn::Error::new_spanned(
                    attr,
                    "expected `#[zlim_error(info | warning | error | panic)]`",
                )
            })?;
            if severity != "info"
                && severity != "warning"
                && severity != "error"
                && severity != "panic"
            {
                let message = format!(
                    "invalid severity `{severity}`; expected one of `info`, `warning`, `error`, `panic`"
                );
                return Err(syn::Error::new_spanned(&severity, message));
            }
            return Ok(Some(severity));
        }
    }
    Ok(None)
}

// ----------------------------------------------------------------------------
// Display generation

/// Returns `Ok(empty)` when there is no `#[error]` at all (Display is
/// optional).
fn gen_display(
    input: &DeriveInput,
    type_error: &Option<TokenStream>,
) -> Result<TokenStream, syn::Error> {
    match &input.data {
        Data::Struct(data) => match type_error {
            Some(tokens) => Ok(gen_struct_display(input, data, tokens)),
            None => Ok(TokenStream::new()),
        },
        Data::Enum(data) => gen_enum_display(input, data, type_error),
        Data::Union(_) => Err(syn::Error::new_spanned(
            input,
            "Error derive does not support unions",
        )),
    }
}

fn gen_struct_display(
    input: &DeriveInput,
    data: &syn::DataStruct,
    tokens: &TokenStream,
) -> TokenStream {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let fields = &data.fields;

    let body = match fields {
        Fields::Named(fields) if fields.named.is_empty() => {
            quote! { __f__.write_str(#tokens) }
        }
        Fields::Named(fields) => {
            let idents = fields.named.iter().map(|f| f.ident.as_ref().unwrap());
            quote! {
                #[expect(clippy::allow_attributes, reason = "allow unused destructure bindings")]
                #[allow(unused, reason = "not all fields may appear in the format string")]
                let Self { #(#idents),* } = self;
                ::core::write!(__f__, #tokens)
            }
        }
        Fields::Unnamed(fields) => {
            let n = fields.unnamed.len();
            let pats = (0..n).map(|i| format_ident!("_{i}"));
            quote! {
                #[expect(clippy::allow_attributes, reason = "allow unused destructure bindings")]
                #[allow(unused, reason = "not all fields may appear in the format string")]
                let Self(#(#pats),*) = self;
                ::core::write!(__f__, #tokens)
            }
        }
        Fields::Unit => {
            quote! { __f__.write_str(#tokens) }
        }
    };

    quote! {
        impl #impl_generics ::core::fmt::Display for #name #ty_generics #where_clause {
            fn fmt(&self, __f__: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                #body
            }
        }
    }
}

fn gen_enum_display(
    input: &DeriveInput,
    data: &syn::DataEnum,
    default_tokens: &Option<TokenStream>,
) -> Result<TokenStream, syn::Error> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let mut specific_arms = Vec::new();
    let has_default = default_tokens.is_some();

    for v in &data.variants {
        let vname = &v.ident;
        let Some(tokens) = find_error_attr(&v.attrs) else {
            if has_default {
                continue;
            } else {
                let message = format!(
                    "variant `{vname}` is missing `#[error(\"...\")]` (no default #[error] on the enum)"
                );
                return Err(syn::Error::new_spanned(v, message));
            }
        };

        match &v.fields {
            Fields::Named(fields) if fields.named.is_empty() => {
                specific_arms.push(quote! { #name::#vname {} => ::core::write!(__f__, #tokens) });
            }
            Fields::Named(fields) => {
                let idents = fields.named.iter().map(|f| f.ident.as_ref().unwrap());
                specific_arms.push(quote! {
                    #[expect(clippy::allow_attributes, reason = "allow unused destructure bindings")]
                    #[allow(unused, reason = "not all fields may appear in the format string")]
                    #name::#vname { #(#idents),* } => ::core::write!(__f__, #tokens)
                });
            }
            Fields::Unnamed(fields) => {
                let n = fields.unnamed.len();
                let pats = (0..n).map(|i| format_ident!("_{i}"));
                specific_arms.push(quote! {
                    #[expect(clippy::allow_attributes, reason = "allow unused destructure bindings")]
                    #[allow(unused, reason = "not all fields may appear in the format string")]
                    #name::#vname(#(#pats),*) => ::core::write!(__f__, #tokens)
                });
            }
            Fields::Unit => {
                specific_arms.push(quote! { #name::#vname => __f__.write_str(#tokens) });
            }
        }
    }

    if !has_default && specific_arms.is_empty() {
        return Ok(TokenStream::new());
    }

    let fallback = match default_tokens {
        Some(tokens) => quote! { _ => __f__.write_str(#tokens) },
        None => TokenStream::new(),
    };

    Ok(quote! {
        impl #impl_generics ::core::fmt::Display for #name #ty_generics #where_clause {
            fn fmt(&self, __f__: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #(#specific_arms,)*
                    #fallback
                }
            }
        }
    })
}

// ----------------------------------------------------------------------------
// Into<ZlimError> via From impl

/// Generate `From<Type> for ZlimError` (which provides `Into<ZlimError>`).
fn gen_zlim_into(
    input: &DeriveInput,
    name: &Ident,
    type_zlim_error: &Option<Ident>,
) -> Result<TokenStream, syn::Error> {
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let severity_opt = match &input.data {
        Data::Struct(_) => type_zlim_error.clone(),
        Data::Enum(data) => {
            return gen_enum_from_impl(input, name, type_zlim_error, data);
        }
        Data::Union(_) => None,
    };

    let Some(severity) = severity_opt else {
        return Ok(TokenStream::new());
    };

    let zlim_core = path::zlim_core_path();
    Ok(gen_struct_from_impl(
        name,
        &impl_generics,
        &ty_generics,
        &where_clause,
        &zlim_core,
        &severity,
    ))
}

/// Generate `From<Enum> for ZlimError` with default-severity + per-variant
/// override logic.
fn gen_enum_from_impl(
    input: &DeriveInput,
    name: &Ident,
    default_sev: &Option<Ident>,
    data: &syn::DataEnum,
) -> Result<TokenStream, syn::Error> {
    let has_default = default_sev.is_some();
    let mut arm_data: Vec<(TokenStream, Ident)> = Vec::new();

    for v in &data.variants {
        let vname = &v.ident;
        let v_sev = parse_zlim_error_attr(&v.attrs)?;

        if let Some(sev) = v_sev {
            arm_data.push((variant_pat(name, v), sev));
        } else if !has_default {
            let message = format!(
                "variant `{vname}` is missing `#[zlim_error(severity)]` (no default #[zlim_error] on the enum)"
            );
            return Err(syn::Error::new_spanned(v, message));
        }
    }

    // At this point either has_default is true or some variant carries the
    // attribute — otherwise we would have errored above.
    if arm_data.is_empty() && !has_default {
        return Ok(TokenStream::new());
    }

    let zlim_core = path::zlim_core_path();
    let specific_arms = arm_data
        .into_iter()
        .map(|(pat, sev)| quote! { #pat => #zlim_core::error::ZlimError::#sev(err), });

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let zlim_error_path = path::zlim_error(&zlim_core);

    let from_body = if let Some(def_sev) = default_sev {
        quote! {
            match err {
                #(#specific_arms)*
                _ => #zlim_error_path::#def_sev(err),
            }
        }
    } else {
        quote! {
            match err {
                #(#specific_arms)*
            }
        }
    };

    Ok(quote! {
        impl #impl_generics ::core::convert::From<#name #ty_generics>
            for #zlim_error_path #where_clause
        {
            fn from(err: #name #ty_generics) -> Self {
                #from_body
            }
        }
    })
}

// ----------------------------------------------------------------------------
// Shared helpers

/// Generate the `From<Type> for ZlimError` impl for a struct.
fn gen_struct_from_impl(
    name: &Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: &Option<&syn::WhereClause>,
    zlim_core: &syn::Path,
    severity: &Ident,
) -> TokenStream {
    let zlim_error_path = path::zlim_error(zlim_core);
    let from_body = quote! { #zlim_error_path::#severity(err) };

    quote! {
        impl #impl_generics ::core::convert::From<#name #ty_generics>
            for #zlim_error_path #where_clause
        {
            fn from(err: #name #ty_generics) -> Self {
                #from_body
            }
        }
    }
}

/// Build a match-arm pattern for an enum variant that ignores all fields
/// (so the whole `err` remains usable in the arm body).
fn variant_pat(name: &Ident, v: &syn::Variant) -> TokenStream {
    let vname = &v.ident;
    match &v.fields {
        Fields::Named(_) => quote! { #name::#vname { .. } },
        Fields::Unnamed(fields) if !fields.unnamed.is_empty() => {
            quote! { #name::#vname(..) }
        }
        _ => quote! { #name::#vname },
    }
}
