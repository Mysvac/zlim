use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;
use syn::Ident;
use syn::parse_quote;

use crate::editor;
use crate::utils::contains_any_idents;

// ----------------------------------------------------------------------------
// Expand
// ----------------------------------------------------------------------------

pub(crate) fn expand(ast: DeriveInput) -> TokenStream {
    let zlim_core = crate::path::zlim_core_path();
    let resource_ = crate::path::resource_(&zlim_core);
    let reflect_ = crate::path::reflect_(&zlim_core);

    let type_ident = &ast.ident;
    let mut generics = ast.generics;

    let editor_fields = match editor::collect_editor_fields(&ast.data) {
        Ok(f) => f,
        Err(e) => return e.into_compile_error(),
    };

    // --- generic bounds ------------------------------------------------
    if generics.type_params().next().is_some() {
        let type_path_ = crate::path::type_path_(&zlim_core);

        generics
            .make_where_clause()
            .predicates
            .push(parse_quote! { Self: #type_path_ + ::core::marker::Sized + 'static });
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

    let editor_tokens = editor::gen_editor_tokens(&editor_fields, &reflect_);
    let ef = editor_tokens.fields;
    let emf = editor_tokens.mutable_fields;
    let erf = editor_tokens.readonly_fields;
    let eff = editor_tokens.field_fn;
    let efm = editor_tokens.field_mut_fn;

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        const _: () = {
            #[automatically_derived]
            impl #impl_generics #resource_ for #type_ident #ty_generics #where_clause {
                const FIELDS: &'static [&'static str] = #ef;
                const MUTABLE_FIELDS: &'static [&'static str] = #emf;
                const READONLY_FIELDS: &'static [&'static str] = #erf;

                #eff
                #efm
            }
        };
    }
}
