use proc_macro2::TokenStream;
use quote::quote;

use super::meta::ReflectMeta;

pub(crate) fn gen_opaque(meta: &ReflectMeta) -> TokenStream {
    let zlim_reflect_path = meta.zlim_reflect();
    let opaque_ = crate::path::opaque_trait(zlim_reflect_path);

    let real_ident = meta.ident();
    let ident_str = real_ident.to_string();
    let (impl_generics, ty_generics, where_clause) = meta.split_generics();
    let msg = format!("expect {ident_str}");

    quote! {
        #[automatically_derived]
        impl #impl_generics #opaque_ for #real_ident #ty_generics #where_clause {
            fn apply_str(&mut self, v: &str) -> ::core::result::Result<(), ::std::string::String> {
                if v == #ident_str {
                    ::core::result::Result::Ok(())
                } else {
                    ::core::result::Result::Err(::std::string::String::from(#msg))
                }
            }

            fn stringify(&self) -> ::std::string::String {
                ::std::string::String::from(#ident_str)
            }
        }
    }
}
