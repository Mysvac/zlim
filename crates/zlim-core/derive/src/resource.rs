use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_quote};

// -----------------------------------------------------------------------------
// Attributes
// -----------------------------------------------------------------------------

/// Parsed `#[resource(...)]` type-level attributes.
struct ResourceAttrs {
    /// `#[resource(reflect)]`: register through `register_reflect`.
    reflect: bool,
}

fn parse_resource_attrs(attrs: &[syn::Attribute]) -> syn::Result<ResourceAttrs> {
    let mut ret = ResourceAttrs { reflect: false };

    for attr in attrs {
        if !attr.path().is_ident("resource") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("reflect") {
                ret.reflect = true;
                Ok(())
            } else {
                Err(meta.error("unsupported resource option; expected `reflect`."))
            }
        })?;
    }

    Ok(ret)
}

// -----------------------------------------------------------------------------
// Expand
// -----------------------------------------------------------------------------

pub(crate) fn expand(ast: DeriveInput) -> TokenStream {
    let zlim_core = crate::path::zlim_core_path();
    let resource_ = crate::path::resource_(&zlim_core);

    let type_ident = &ast.ident;
    let mut generics = ast.generics;

    let attrs = match parse_resource_attrs(&ast.attrs) {
        Ok(a) => a,
        Err(e) => return e.into_compile_error(),
    };

    // --- generic bounds ------------------------------------------------
    if generics.type_params().next().is_some() {
        let predicates = &mut generics.make_where_clause().predicates;
        predicates.push(parse_quote! { Self: ::core::marker::Sized + 'static });

        if attrs.reflect {
            let type_database_ = crate::path::type_database_(&zlim_core);
            predicates.push(parse_quote! { Self: #type_database_ });
        }
    } else if generics.lifetimes().next().is_some() {
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote! { Self: 'static });
    }

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // --- register ------------------------------------------------------
    // `#[resource(reflect)]` resources override the trait's default
    // registration so the reflection pointers are filled in.
    let register_tokens = if attrs.reflect {
        let resource_db_ = crate::path::resource_db_(&zlim_core);
        let register_reflect_ = crate::path::resource_register_reflect_(&zlim_core);
        quote! {
            const REGISTER: fn() -> &'static #resource_db_ = #register_reflect_::<Self>;
        }
    } else {
        TokenStream::new()
    };

    // --- auto-registration (non-generic types only) -------------------
    let auto_register = if generics.type_params().next().is_none() {
        let submit_ = crate::path::submit_(&zlim_core);
        quote! {
            #submit_!(
                #zlim_core::resource::__internal__::__ResourceReg__::of::<#type_ident>()
                => #zlim_core::resource::__internal__::__ResourceReg__
            );
        }
    } else {
        TokenStream::new()
    };

    quote! {
        const _: () = {
            #[automatically_derived]
            impl #impl_generics #resource_ for #type_ident #ty_generics #where_clause {
                #register_tokens
            }

            #auto_register
        };
    }
}
