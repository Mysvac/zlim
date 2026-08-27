//! Implementation for the `#[derive(ScheduleStage)]` macro.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields};

/// Expands `#[derive(ScheduleStage)]`.
///
/// Only **unit structs** and **data-less enums** (every variant is a unit
/// variant) are supported; generics, unions, structs with fields, and enums
/// with data-carrying variants are rejected.
pub(crate) fn expand(ast: DeriveInput) -> TokenStream {
    let zlim_core = crate::path::zlim_core_path();
    let schedule_stage_ = crate::path::schedule_stage_(&zlim_core);
    let type_path_ = crate::path::type_path_trait_(&zlim_core);

    if !ast.generics.params.is_empty() {
        let message = "ScheduleStage does not support generic types";
        return syn::Error::new_spanned(&ast.generics, message).into_compile_error();
    }

    let type_ident = ast.ident;

    let stage_name_body = match &ast.data {
        Data::Struct(data) => match &data.fields {
            Fields::Unit => {
                quote! {
                    ::std::borrow::Cow::Borrowed(<Self as #type_path_>::type_path())
                }
            }
            _ => {
                let message = "ScheduleStage can only be derived for unit structs";
                return syn::Error::new_spanned(&type_ident, message).into_compile_error();
            }
        },
        Data::Enum(data) => {
            // Data-less enums only: every variant must be a unit variant.
            for variant in &data.variants {
                if !matches!(variant.fields, Fields::Unit) {
                    let message = "ScheduleStage can only be derived for data-less enums \
                                   (every variant must be a unit variant)";
                    return syn::Error::new_spanned(variant, message).into_compile_error();
                }
            }

            let arms = data.variants.iter().map(|variant| {
                let variant_ident = &variant.ident;
                let variant_name = variant_ident.to_string();
                quote! {
                    Self::#variant_ident => {
                        ::std::borrow::Cow::Owned(::std::format!(
                            "{}::{}",
                            <Self as #type_path_>::type_path(),
                            #variant_name
                        ))
                    }
                }
            });

            quote! {
                match self {
                    #(#arms,)*
                }
            }
        }
        Data::Union(_) => {
            let message = "ScheduleStage can only be derived for unit structs and data-less enums";
            return syn::Error::new_spanned(&type_ident, message).into_compile_error();
        }
    };

    quote! {
        const _: () = {
            #[automatically_derived]
            impl #schedule_stage_ for #type_ident {
                fn stage_name(&self) -> ::std::borrow::Cow<'_, str> {
                    #stage_name_body
                }
            }
        };
    }
}
