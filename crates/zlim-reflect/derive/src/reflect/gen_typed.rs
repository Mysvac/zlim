//! Code generation for `impl Typed for Type`.

use proc_macro2::TokenStream;
use quote::quote;

use super::ReflectDerive;

pub(crate) fn gen_typed(derive: &ReflectDerive) -> TokenStream {
    let (info_tokens, is_const) = match derive {
        ReflectDerive::Struct(r) => (r.type_info_tokens(false), false),
        ReflectDerive::Tuple(r) => (r.type_info_tokens(true), false),
        ReflectDerive::UnitStruct(r) => r.type_info_tokens(),
        ReflectDerive::Enum(r) => (r.type_info_tokens(), false),
        ReflectDerive::Opaque(r) => r.type_info_tokens(),
    };

    let meta = derive.meta();
    let zlim_reflect_path = meta.zlim_reflect();
    let typed_ = crate::path::typed_trait(zlim_reflect_path);
    let type_info_ = crate::path::type_info(zlim_reflect_path);

    let inner_cell_tokens = if is_const {
        quote! {
            static INFO: #type_info_ = #info_tokens;
            &INFO
        }
    } else if meta.only_lifetime_generics() {
        quote! {
            static CELL: ::std::sync::OnceLock<#type_info_> = ::std::sync::OnceLock::new();
            CELL.get_or_init(|| { #info_tokens })
        }
    } else {
        let info_cell = crate::path::info_cell(zlim_reflect_path);
        quote! {
            static CELL: #info_cell = #info_cell::new();
            CELL.get_or_init::<Self>(|| { #info_tokens })
        }
    };

    let real_ident = meta.ident();
    let (impl_generics, ty_generics, where_clause) = meta.split_generics();

    quote! {
        #[automatically_derived]
        impl #impl_generics #typed_ for #real_ident #ty_generics #where_clause {
            fn type_info() -> &'static #type_info_ {
                #inner_cell_tokens
            }
        }
    }
}
