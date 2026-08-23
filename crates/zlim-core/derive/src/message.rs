//! Implementation for the `Message` derive macro.

use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_quote};

/// Expands `#[derive(Message)]` into a marker-trait implementation.
pub(crate) fn expand(ast: DeriveInput) -> TokenStream {
    let zlim_core = crate::path::zlim_core_path();
    let message_ = crate::path::message_(&zlim_core);
    let type_path_ = crate::path::type_path_(&zlim_core);

    let type_ident = ast.ident;

    let mut generics = ast.generics;
    if generics.type_params().next().is_some() {
        generics.make_where_clause().predicates.push(parse_quote! {
            Self: ::core::marker::Send + ::core::marker::Sync + #type_path_ + 'static
        });
    } else if generics.lifetimes().next().is_some() {
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote! { Self: 'static });
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        const _: () = {
            impl #impl_generics #message_ for #type_ident #ty_generics #where_clause {}
        };
    }
    .into()
}
