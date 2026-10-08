//! Implementation of the `#[derive(Error)]` proc-macro.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, Ident};

// -----------------------------------------------------------------------------
// Internal expansion

pub fn expand(input: &DeriveInput) -> TokenStream {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Parse custom attributes (preserved via `attributes(error, zlim_error)`).
    let type_error = match find_error_attr(&input.attrs) {
        Ok(tokens) => tokens,
        Err(e) => return e.into_compile_error(),
    };
    let type_severity = match parse_zlim_error_attr(&input.attrs) {
        Ok(sev) => sev,
        Err(e) => return e.into_compile_error(),
    };

    // 1. Always: core::error::Error
    let error_impl = quote! {
        #[automatically_derived]
        impl #impl_generics ::core::error::Error for #name #ty_generics #where_clause {}
    };

    // 2. Optional: Display
    let display_impl = match gen_display(input, &type_error) {
        Ok(ts) => ts,
        Err(e) => return e.into_compile_error(),
    };

    // 3. Optional: Into<ZlimError> via From impl
    let zlim_impls = match gen_zlim_into(input, name, &type_severity) {
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

// -----------------------------------------------------------------------------
// Attribute parsing

/// The parsed content of an `#[error(...)]` attribute.
enum ErrorAttr {
    /// `#[error("...")]` — a `format!`-style template, optionally followed by
    /// extra arguments.
    Format(TokenStream),
    /// `#[error(transparent)]` — forward `Display` to the single field.
    Transparent,
}

/// Extract `#[error("format string")]`, `#[error("fmt", extra...)]` or
/// `#[error(transparent)]`.
fn find_error_attr(attrs: &[syn::Attribute]) -> Result<Option<ErrorAttr>, syn::Error> {
    for attr in attrs {
        if attr.path().is_ident("error") {
            let tokens: TokenStream = attr.parse_args()?;
            if tokens.to_string() == "transparent" {
                return Ok(Some(ErrorAttr::Transparent));
            } else {
                return Ok(Some(ErrorAttr::Format(tokens)));
            }
        }
    }
    Ok(None)
}

/// Parse `#[zlim_error(severity)]`.  Returns an error for invalid severity
/// values so the user gets a clear compile-time diagnostic.
///
/// Accepted severity identifiers match the `ZlimError` constructors:
/// `ignore`, `debug`, `info`, `warning`, `error`, `panic`.
fn parse_zlim_error_attr(attrs: &[syn::Attribute]) -> Result<Option<Ident>, syn::Error> {
    const SEV: &str = "ignore | debug | info | warning | error | panic";

    for attr in attrs {
        if attr.path().is_ident("zlim_error") {
            let severity: Ident = attr.parse_args().map_err(|_| {
                let msg = format!("expected `#[zlim_error({SEV})]`");
                syn::Error::new_spanned(attr, msg)
            })?;

            let s = severity.to_string();

            match s.as_str() {
                "ignore" | "debug" | "info" | "warning" | "error" | "panic" => {}
                "warn" => return Err(syn::Error::new_spanned(&severity, "use `warning` instead")),
                _ => {
                    let msg = format!("invalid severity `{s}`; expected one of `{SEV}`");
                    return Err(syn::Error::new_spanned(&severity, msg));
                }
            }

            return Ok(Some(severity));
        }
    }
    Ok(None)
}

// -----------------------------------------------------------------------------
// Display generation

/// Returns `Ok(empty)` when there is no `#[error]` at all (Display is
/// optional).
fn gen_display(
    input: &DeriveInput,
    type_error: &Option<ErrorAttr>,
) -> Result<TokenStream, syn::Error> {
    const E: &str = "Error derive does not support unions";
    match &input.data {
        Data::Struct(data) => match type_error {
            Some(attr) => gen_struct_display(input, data, attr),
            None => Ok(TokenStream::new()),
        },
        Data::Enum(data) => gen_enum_display(input, data, type_error),
        Data::Union(_) => Err(syn::Error::new_spanned(input, E)),
    }
}

/// Generates the `Display` impl of a struct (or of a newtype used as the
/// error type itself).
fn gen_struct_display(
    input: &DeriveInput,
    data: &syn::DataStruct,
    attr: &ErrorAttr,
) -> Result<TokenStream, syn::Error> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let body = match attr {
        ErrorAttr::Transparent => {
            if !is_single_tuple_field(&data.fields) {
                const E: &str = "#[error(transparent)]` is only valid on a single-field tuple.";
                return Err(syn::Error::new_spanned(input, E));
            }

            // Forward straight to the inner value's `Display`: no formatting
            // machinery and no intermediate allocation.
            quote! { ::core::fmt::Display::fmt(&self.0, __f__) }
        }
        ErrorAttr::Format(tokens) => match &data.fields {
            Fields::Named(fields) if fields.named.is_empty() => {
                quote! { ::core::write!(__f__, #tokens) }
            }
            Fields::Named(fields) => {
                let idents = fields.named.iter().map(|f| f.ident.as_ref().unwrap());
                quote! {
                    // // use `#[automatically_derived]` instead.
                    // #[expect(clippy::allow_attributes, reason = "allow unused destructure bindings")]
                    // #[allow(unused, reason = "not all fields may appear in the format string")]
                    let Self { #(#idents),* } = self;
                    ::core::write!(__f__, #tokens)
                }
            }
            Fields::Unnamed(fields) => {
                let n = fields.unnamed.len();
                let pats = (0..n).map(|i| format_ident!("_{i}"));
                quote! {
                    // // use `#[automatically_derived]` instead.
                    // #[expect(clippy::allow_attributes, reason = "allow unused destructure bindings")]
                    // #[allow(unused, reason = "not all fields may appear in the format string")]
                    let Self(#(#pats),*) = self;
                    ::core::write!(__f__, #tokens)
                }
            }
            Fields::Unit => {
                quote! { ::core::write!(__f__, #tokens) }
            }
        },
    };

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::core::fmt::Display for #name #ty_generics #where_clause {
            fn fmt(&self, __f__: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                #body
            }
        }
    })
}

fn gen_enum_display(
    input: &DeriveInput,
    data: &syn::DataEnum,
    default_attr: &Option<ErrorAttr>,
) -> Result<TokenStream, syn::Error> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // `transparent` needs exactly one field to delegate to, which an enum
    // itself never has.
    if matches!(default_attr, Some(ErrorAttr::Transparent)) {
        const E: &str = "`#[error(transparent)]` cannot be applied to an enum; place it on a single-field tuple variant instead";
        return Err(syn::Error::new_spanned(input, E));
    }

    let mut specific_arms = Vec::new();
    let has_default = default_attr.is_some();

    for v in &data.variants {
        let vname = &v.ident;

        let Some(attr) = find_error_attr(&v.attrs)? else {
            if has_default {
                continue;
            } else {
                let message = format!(
                    "variant `{vname}` is missing `#[error(\"...\")]` (no default #[error] on the enum)"
                );
                return Err(syn::Error::new_spanned(v, message));
            }
        };

        let arm = match &attr {
            ErrorAttr::Format(tokens) => match &v.fields {
                Fields::Named(fields) if fields.named.is_empty() => {
                    quote! { #name::#vname {} => ::core::write!(__f__, #tokens) }
                }
                Fields::Named(fields) => {
                    let idents = fields.named.iter().map(|f| f.ident.as_ref().unwrap());
                    quote! {
                        // // use `#[automatically_derived]` instead.
                        // #[expect(clippy::allow_attributes, reason = "allow unused destructure bindings")]
                        // #[allow(unused, reason = "not all fields may appear in the format string")]
                        #name::#vname { #(#idents),* } => ::core::write!(__f__, #tokens)
                    }
                }
                Fields::Unnamed(fields) => {
                    let n = fields.unnamed.len();
                    let pats = (0..n).map(|i| format_ident!("_{i}"));
                    quote! {
                        // // use `#[automatically_derived]` instead.
                        // #[expect(clippy::allow_attributes, reason = "allow unused destructure bindings")]
                        // #[allow(unused, reason = "not all fields may appear in the format string")]
                        #name::#vname(#(#pats),*) => ::core::write!(__f__, #tokens)
                    }
                }
                Fields::Unit => {
                    quote! { #name::#vname => ::core::write!(__f__, #tokens) }
                }
            },
            ErrorAttr::Transparent => {
                if !is_single_tuple_field(&v.fields) {
                    const E: &str =
                        "`#[error(transparent)]` is only valid on a single-field tuple variant";
                    return Err(syn::Error::new_spanned(v, E));
                }

                // Forward straight to the inner value's `Display`.
                quote! { #name::#vname(__inner__) => ::core::fmt::Display::fmt(__inner__, __f__) }
            }
        };

        specific_arms.push(arm);
    }

    if !has_default && specific_arms.is_empty() {
        return Ok(TokenStream::new());
    }

    let fallback = match default_attr {
        Some(ErrorAttr::Format(tokens)) => quote! { _ => ::core::write!(__f__, #tokens) },
        // `Transparent` is rejected above; `None` means every variant has its
        // own arm.
        _ => TokenStream::new(),
    };

    Ok(quote! {
        #[automatically_derived]
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

/// Returns `true` for a single-field tuple (`(T,)`), the only shape
/// `#[error(transparent)]` accepts.
fn is_single_tuple_field(fields: &Fields) -> bool {
    matches!(fields, Fields::Unnamed(fields) if fields.unnamed.len() == 1)
}

// -----------------------------------------------------------------------------
// Into<ZlimError> via From impl

/// `::zlim_error` or `::zlim::error`
fn zlim_error_crate() -> syn::Path {
    zlim_derive_utils::crate_path("zlim_error")
}

/// ZlimError type
fn zlim_error(path: &syn::Path) -> TokenStream {
    quote! { #path::ZlimError }
}

/// Generate `From<Type> for ZlimError` (which provides `Into<ZlimError>`).
fn gen_zlim_into(
    input: &DeriveInput,
    name: &Ident,
    type_severity: &Option<Ident>,
) -> Result<TokenStream, syn::Error> {
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    const E: &str = "Error derive does not support unions";
    match &input.data {
        Data::Struct(_) => {}
        Data::Enum(data) => return gen_enum_from_impl(input, name, type_severity, data),
        Data::Union(_) => return Err(syn::Error::new_spanned(input, E)),
    };

    let Some(severity) = type_severity else {
        return Ok(TokenStream::new());
    };

    Ok(gen_struct_from_impl(
        name,
        &impl_generics,
        &ty_generics,
        &where_clause,
        severity,
    ))
}

/// Generate `From<Enum> for ZlimError` with default-severity + per-variant
/// override logic.
///
/// Attribute resolution rules:
///
/// - If **no** `#[zlim_error]` exists on either the enum or any variant,
///   no `From` impl is generated and no error is raised.
/// - If **all** variants carry `#[zlim_error]`, the enum-level attribute
///   is optional.
/// - If **some but not all** variants carry `#[zlim_error]`, the
///   enum-level attribute is required and used as the fallback.
fn gen_enum_from_impl(
    input: &DeriveInput,
    name: &Ident,
    default_sev: &Option<Ident>,
    data: &syn::DataEnum,
) -> Result<TokenStream, syn::Error> {
    let has_default = default_sev.is_some();
    let mut arm_data: Vec<(TokenStream, Ident)> = Vec::new();

    // First pass: collect per-variant severities and coverage flags.
    let mut any_variant_sev = false;
    let mut all_variants_have_sev = true;

    for v in &data.variants {
        let v_sev = parse_zlim_error_attr(&v.attrs)?;

        match v_sev {
            Some(sev) => {
                any_variant_sev = true;
                arm_data.push((variant_pat(name, v), sev));
            }
            None => all_variants_have_sev = false,
        }
    }

    // No `#[zlim_error]` anywhere: skip the impl entirely, no error.
    if !has_default && !any_variant_sev {
        return Ok(TokenStream::new());
    }

    // Some variants carry the attribute, others do not, and there is no
    // enum-level default to fall back on — report the first offender.
    if !has_default && !all_variants_have_sev {
        for v in &data.variants {
            if parse_zlim_error_attr(&v.attrs)?.is_none() {
                let vname = &v.ident;
                let message = format!(
                    "variant `{vname}` is missing `#[zlim_error(severity)]` (no default #[zlim_error] on the enum)"
                );
                return Err(syn::Error::new_spanned(v, message));
            }
        }
        unreachable!("coverage check guarantees a missing variant exists");
    }

    let zlim_error_crate = zlim_error_crate();
    let zlim_error = zlim_error(&zlim_error_crate);

    let specific_arms = arm_data
        .into_iter()
        .map(|(pat, sev)| quote! { #pat => #zlim_error::#sev(err), });

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let from_body = if let Some(def_sev) = default_sev {
        quote! {
            match err {
                #(#specific_arms)*
                _ => #zlim_error::#def_sev(err),
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
        #[automatically_derived]
        impl #impl_generics ::core::convert::From<#name #ty_generics> for #zlim_error #where_clause {
            #[cold]
            #[track_caller]
            fn from(err: #name #ty_generics) -> Self {
                #from_body
            }
        }
    })
}

// -----------------------------------------------------------------------------
// Shared helpers

/// Generate the `From<Type> for ZlimError` impl for a struct.
fn gen_struct_from_impl(
    name: &Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: &Option<&syn::WhereClause>,
    severity: &Ident,
) -> TokenStream {
    let zlim_error_crate = zlim_error_crate();
    let zlim_error = zlim_error(&zlim_error_crate);

    quote! {
        #[automatically_derived]
        impl #impl_generics ::core::convert::From<#name #ty_generics> for #zlim_error #where_clause {
            #[cold]
            #[track_caller]
            fn from(err: #name #ty_generics) -> Self {
                #zlim_error::#severity(err)
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
        Fields::Unnamed(_) => quote! { #name::#vname(..) },
        Fields::Unit => quote! { #name::#vname },
    }
}
