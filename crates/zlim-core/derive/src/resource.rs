use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Ident, parse_quote};

use crate::editor;
use crate::utils::contains_any_idents;

// -----------------------------------------------------------------------------
// Attributes
// -----------------------------------------------------------------------------

/// Parses `#[resource(serialize)]` — marks the resource with
/// `SERIALIZE = true`.
fn parse_resource_attrs(attrs: &[syn::Attribute]) -> syn::Result<bool> {
    let mut serialize = false;

    for attr in attrs {
        if !attr.path().is_ident("resource") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("serialize") {
                serialize = true;
                Ok(())
            } else {
                Err(meta.error("unsupported resource option; expected `serialize`."))
            }
        })?;
    }

    Ok(serialize)
}

// -----------------------------------------------------------------------------
// Expand
// -----------------------------------------------------------------------------

pub(crate) fn expand(ast: DeriveInput) -> TokenStream {
    let zlim_core = crate::path::zlim_core_path();
    let resource_ = crate::path::resource_(&zlim_core);
    let reflect_ = crate::path::reflect_(&zlim_core);

    let type_ident = &ast.ident;
    let mut generics = ast.generics;

    let serialize = match parse_resource_attrs(&ast.attrs) {
        Ok(v) => v,
        Err(e) => return e.into_compile_error(),
    };

    let editor_fields = match editor::collect_editor_fields(&ast.data) {
        Ok(f) => f,
        Err(e) => return e.into_compile_error(),
    };

    // --- generic bounds ------------------------------------------------
    if generics.type_params().next().is_some() {
        let type_path_ = crate::path::type_path_(&zlim_core);

        // `Serialize`/`Deserialize` are only required when the resource is
        // registered with serialization support.
        let serde_bounds = if serialize {
            let serialize_ = crate::path::serialize_(&zlim_core);
            let deserialize_ = crate::path::deserialize_(&zlim_core);
            quote! { + #serialize_ + for<'__de_x_> #deserialize_<'__de_x_> }
        } else {
            quote! {}
        };

        generics.make_where_clause().predicates.push(
            parse_quote! { Self: #type_path_ #serde_bounds + ::core::marker::Sized + 'static },
        );
    } else if generics.lifetimes().next().is_some() {
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote! { Self: 'static });
    }

    // Collect generic parameter idents for the constraint check.
    let generic_idents: Vec<Ident> = generics.type_params().map(|p| p.ident.clone()).collect();

    // Add `FieldTy: Reflect` constraints for editor fields whose type
    // references an unresolved generic parameter.
    for f in &editor_fields {
        if contains_any_idents(f.ty, &generic_idents) {
            crate::utils::field_type_constraint(&mut generics, f.ty, &reflect_);
        }
    }

    let editor_tokens =
        editor::gen_editor_tokens(&editor_fields, &type_ident.to_string(), &reflect_);
    let eg = editor_tokens.getter;
    let es = editor_tokens.setter;
    let egf = editor_tokens.get_field_fn;
    let esf = editor_tokens.set_field_fn;

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // --- serialization -------------------------------------------------
    // `#[resource(serialize)]` resources override the trait's default
    // registration to fill the serialization function pointers, and expose
    // `SERIALIZE = true`.
    let serialize_tokens = if serialize {
        let resource_db_ = crate::path::resource_db_(&zlim_core);
        quote! {
            const SERIALIZE: bool = true;

            fn register() -> &'static #resource_db_ {
                #zlim_core::resource::register_serializable::<Self>()
            }
        }
    } else {
        TokenStream::new()
    };

    // --- auto-registration (non-generic types only) -------------------
    let auto_register = if generics.type_params().next().is_none() {
        quote! {
            #zlim_core::__macro_exports__::__submit!(
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
                #serialize_tokens

                const GETTER: &'static [&'static str] = #eg;
                const SETTER: &'static [&'static str] = #es;

                #egf
                #esf
            }

            #auto_register
        };
    }
}
