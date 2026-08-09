use proc_macro2::TokenStream;
use quote::quote;

use crate::reflect::data::{ReflectStruct, StructFieldAccessors};

pub(crate) fn gen_struct(info: &ReflectStruct) -> TokenStream {
    let meta = info.meta();

    let zlim_reflect_path = meta.zlim_reflect();
    let struct_ = crate::path::struct_trait(zlim_reflect_path);
    let reflect_ = crate::path::reflect_trait(zlim_reflect_path);
    let struct_field_iter_ = crate::path::struct_field_iter(zlim_reflect_path);

    let field_members: Vec<&syn::Ident> = info
        .active_fields()
        .map(|field| field.data.ident.as_ref().unwrap())
        .collect();

    let field_names: Vec<String> = field_members.iter().map(ToString::to_string).collect();

    let StructFieldAccessors {
        fields_ref,
        fields_mut,
        field_indices,
        field_count,
    } = info.field_accessors();

    let real_ident = meta.ident();
    let (impl_generics, ty_generics, where_clause) = meta.split_generics();

    quote! {
        #[automatically_derived]
        impl #impl_generics #struct_ for #real_ident #ty_generics #where_clause {
            fn field(&self, __name__: &str) -> ::core::option::Option<&dyn #reflect_> {
                match __name__ {
                    #(#field_names => ::core::option::Option::Some(#fields_ref),)*
                    _ => ::core::option::Option::None,
                }
            }

            fn field_mut(&mut self, __name__: &str) -> ::core::option::Option<&mut dyn #reflect_> {
                match __name__ {
                    #(#field_names => ::core::option::Option::Some(#fields_mut),)*
                    _ => ::core::option::Option::None,
                }
            }

            fn field_at(&self, __index__: usize) -> ::core::option::Option<&dyn #reflect_> {
                match __index__ {
                    #(#field_indices => ::core::option::Option::Some(#fields_ref),)*
                    _ => ::core::option::Option::None,
                }
            }

            fn field_at_mut(&mut self, __index__: usize) -> ::core::option::Option<&mut dyn #reflect_> {
                match __index__ {
                    #(#field_indices => ::core::option::Option::Some(#fields_mut),)*
                    _ => ::core::option::Option::None,
                }
            }

            fn name_at(&self, __index__: usize) -> ::core::option::Option<&str> {
                match __index__ {
                    #(#field_indices => ::core::option::Option::Some(#field_names),)*
                    _ => ::core::option::Option::None,
                }
            }

            fn index_of(&self, __name__: &str) -> ::core::option::Option<usize> {
                match __name__ {
                    #(#field_names => ::core::option::Option::Some(#field_indices),)*
                    _ => ::core::option::Option::None,
                }
            }

            #[inline]
            fn field_len(&self) -> usize {
                #field_count
            }

            #[inline]
            fn iter_fields(&self) -> #struct_field_iter_<'_> {
                #struct_field_iter_::new(self)
            }

            fn unpack(self: ::std::boxed::Box<Self>) -> ::std::vec::Vec<(::std::borrow::Cow<'static, str>, ::std::boxed::Box<dyn #reflect_>)> {
                ::std::vec![
                    #( ( ::std::borrow::Cow::Borrowed(#field_names) , Box::new( self.#field_members )), )*
                ]
            }
        }
    }
}
