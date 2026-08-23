use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_quote};

pub(crate) fn expand(ast: DeriveInput) -> TokenStream {
    let zlim_core_path = crate::path::zlim_core_path();
    let trait_path = crate::path::schedule_label_(&zlim_core_path);

    if matches!(&ast.data, syn::Data::Union(_)) {
        let message = format!("Cannot derive {trait_path} for unions.");
        return syn::Error::new_spanned(ast, message).into_compile_error();
    }

    let type_ident = ast.ident;

    let mut generics = ast.generics;
    if generics.type_params().next().is_some() {
        generics.make_where_clause().predicates.push(parse_quote! {
            Self: 'static
                + ::core::marker::Send
                + ::core::marker::Sync
                + ::core::clone::Clone
                + ::core::fmt::Debug
                + ::core::hash::Hash
                + ::core::cmp::Eq
        });
    } else if generics.lifetimes().next().is_some() {
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote! { Self: 'static });
    }

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        const _:() = {
            impl #impl_generics #trait_path for #type_ident #ty_generics #where_clause {
                fn dyn_clone(&self) -> ::std::boxed::Box<dyn #trait_path> {
                    ::std::boxed::Box::new(::core::clone::Clone::clone(self))
                }
            }
        };
    }
}
