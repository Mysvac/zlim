//! Expansion of `#[derive(Asset)]` and `#[derive(VisitAssetDependencies)]`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DataEnum, DeriveInput, Fields, Member};

use crate::zlim_asset_path;

// -----------------------------------------------------------------------------
// Attributes

/// Returns `true` when the field carries `#[asset(dependency)]`.
fn is_dependency(field: &syn::Field) -> syn::Result<bool> {
    let mut dependency = false;

    for attr in &field.attrs {
        if !attr.path().is_ident("asset") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("dependency") {
                dependency = true;
                Ok(())
            } else {
                Err(meta.error("unsupported `asset` option; the only option is `dependency`."))
            }
        })?;
    }

    Ok(dependency)
}

/// The members of the fields marked with `#[asset(dependency)]`, in declaration order.
fn dependencies(fields: &Fields) -> syn::Result<Vec<Member>> {
    let mut members = Vec::new();

    for (index, field) in fields.iter().enumerate() {
        if !is_dependency(field)? {
            continue;
        }

        members.push(match &field.ident {
            Some(ident) => Member::Named(ident.clone()),
            None => Member::Unnamed(syn::Index::from(index)),
        });
    }

    Ok(members)
}

// -----------------------------------------------------------------------------
// `visit_dependencies`

/// Body of `visit_dependencies` for a struct: visit every marked field in place.
fn struct_body(fields: &Fields, zlim_asset: &syn::Path) -> syn::Result<Option<TokenStream>> {
    let deps = dependencies(fields)?;

    let calls = deps.into_iter().map(|member| {
        quote!(#zlim_asset::asset::VisitAssetDependencies::visit_dependencies(&self.#member, visit);)
    });

    Ok(Some(quote!(#(#calls)*)))
}

/// Body of `visit_dependencies` for an enum: destructure the matched variant and visit its
/// marked fields. `None` when no variant declares a dependency.
fn enum_body(data: &DataEnum, zlim_asset: &syn::Path) -> syn::Result<Option<TokenStream>> {
    let mut arms = Vec::new();

    for variant in &data.variants {
        let members = dependencies(&variant.fields)?;
        if members.is_empty() {
            continue;
        }

        let ident = &variant.ident;

        let bindings: Vec<_> = (0..members.len())
            .map(|index| format_ident!("__dep_{index}", span = ident.span()))
            .collect();

        let calls = bindings.iter().map(|binding| {
            quote!(#zlim_asset::asset::VisitAssetDependencies::visit_dependencies(#binding, visit);)
        });

        arms.push(quote! {
            Self::#ident { #(#members: #bindings,)* .. } => {
                #(#calls)*
            }
        });
    }

    if arms.is_empty() {
        return Ok(None);
    }

    // Variants without dependencies are handled by the fallback arm.
    Ok(Some(quote!(match self { #(#arms)* _ => {} })))
}

/// The whole `VisitAssetDependencies` impl block.
fn visit_impl(ast: &DeriveInput, zlim_asset: &syn::Path) -> syn::Result<TokenStream> {
    let ident = &ast.ident;
    let (impl_generics, type_generics, where_clause) = ast.generics.split_for_impl();

    let body = match &ast.data {
        Data::Struct(data) => struct_body(&data.fields, zlim_asset)?,
        Data::Enum(data) => enum_body(data, zlim_asset)?,
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                ident,
                "`VisitAssetDependencies` cannot be derived for unions",
            ));
        }
    };

    // The visitor parameter is unused when no field is marked.
    let visitor = match body {
        Some(_) => quote!(visit),
        None => quote!(_visit),
    };
    let body = body.unwrap_or_default();

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics #zlim_asset::asset::VisitAssetDependencies for #ident #type_generics #where_clause {
            fn visit_dependencies(&self, #visitor: &mut dyn ::core::ops::FnMut(#zlim_asset::ident::ErasedAssetId)) { #body }
        }
    })
}

// -----------------------------------------------------------------------------
// Expansion

/// `#[derive(Asset)]`: the marker trait plus the dependency visitor.
pub(crate) fn expand_asset(ast: &mut DeriveInput) -> TokenStream {
    let zlim_asset = zlim_asset_path();

    let ident = &ast.ident;

    let (impl_generics, type_generics, where_clause) = ast.generics.split_for_impl();

    let visit = match visit_impl(ast, &zlim_asset) {
        Ok(visit) => visit,
        Err(error) => return error.into_compile_error(),
    };

    quote! {
        #[automatically_derived]
        impl #impl_generics #zlim_asset::asset::Asset for #ident #type_generics #where_clause {}

        #visit
    }
}

/// `#[derive(VisitAssetDependencies)]`: only the dependency visitor.
pub(crate) fn expand_visit_dependencies(ast: &DeriveInput) -> TokenStream {
    let zlim_asset = zlim_asset_path();

    match visit_impl(ast, &zlim_asset) {
        Ok(visit) => visit,
        Err(error) => error.into_compile_error(),
    }
}
