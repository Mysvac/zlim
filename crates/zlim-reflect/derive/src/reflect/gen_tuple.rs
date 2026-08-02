use proc_macro2::TokenStream;
use quote::quote;

use crate::reflect::data::{ReflectStruct, StructFieldAccessors};

pub(crate) fn gen_tuple(info: &ReflectStruct) -> TokenStream {
    let meta = info.meta();

    let zlim_reflect_path = meta.zlim_reflect();
    let tuple_ = crate::path::tuple_trait(zlim_reflect_path);
    let reflect_ = crate::path::reflect_trait(zlim_reflect_path);
    let tuple_field_iter_ = crate::path::tuple_field_iter(zlim_reflect_path);

    let StructFieldAccessors {
        fields_ref,
        fields_mut,
        field_indices,
        field_count,
    } = info.field_accessors();

    let members: Vec<_> = info.active_fields().map(|f| f.to_member()).collect();

    let real_ident = meta.ident();
    let (impl_generics, ty_generics, where_clause) = meta.split_generics();

    quote! {
        impl #impl_generics #tuple_ for #real_ident #ty_generics #where_clause {
            fn field(&self, __index__: usize) -> ::core::option::Option<&dyn #reflect_> {
                match __index__ {
                    #(#field_indices => ::core::option::Option::Some(#fields_ref),)*
                    _ => ::core::option::Option::None,
                }
            }

            fn field_mut(&mut self, __index__: usize) -> ::core::option::Option<&mut dyn #reflect_> {
                match __index__ {
                    #(#field_indices => ::core::option::Option::Some(#fields_mut),)*
                    _ => ::core::option::Option::None,
                }
            }

            #[inline]
            fn field_len(&self) -> usize {
                #field_count
            }

            #[inline]
            fn iter_fields(&self) -> #tuple_field_iter_<'_> {
                #tuple_field_iter_::new(self)
            }

            fn unpack(self: ::std::boxed::Box<Self>) -> ::std::vec::Vec<::std::boxed::Box<dyn #reflect_>> {
                ::std::vec![
                    #( Box::new( self.#members ), )*
                ]
            }
        }
    }
}
