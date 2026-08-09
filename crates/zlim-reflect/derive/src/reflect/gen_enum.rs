use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, Member};

use crate::reflect::data::{EnumVariantFields, ReflectEnum, StructField};

pub(crate) fn gen_enum(info: &ReflectEnum) -> TokenStream {
    let meta = info.meta();

    let zlim_reflect_path = meta.zlim_reflect();
    let enum_ = crate::path::enum_trait(zlim_reflect_path);
    let reflect_ = crate::path::reflect_trait(zlim_reflect_path);
    let variant_kind_ = crate::path::variant_kind(zlim_reflect_path);
    let variant_field_iter_ = crate::path::variant_field_iter(zlim_reflect_path);

    let ref_name = Ident::new("__name__", Span::call_site());
    let ref_index = Ident::new("__index__", Span::call_site());

    let real_ident = meta.ident();
    let (impl_generics, ty_generics, where_clause) = meta.split_generics();

    let hint: usize = info.active_fields().count();

    let mut enum_field: Vec<TokenStream> = Vec::with_capacity(hint);
    let mut enum_field_mut: Vec<TokenStream> = Vec::with_capacity(hint);
    let mut enum_field_at: Vec<TokenStream> = Vec::with_capacity(hint);
    let mut enum_field_at_mut: Vec<TokenStream> = Vec::with_capacity(hint);
    let mut enum_index_of: Vec<TokenStream> = Vec::with_capacity(hint);
    let mut enum_name_at: Vec<TokenStream> = Vec::with_capacity(hint);
    let mut enum_field_len: Vec<TokenStream> = Vec::with_capacity(hint);
    let mut enum_variant_name: Vec<TokenStream> = Vec::with_capacity(hint);
    let mut enum_variant_index: Vec<TokenStream> = Vec::with_capacity(hint);
    let mut enum_variant_kind: Vec<TokenStream> = Vec::with_capacity(hint);
    let mut enum_variant_unpack: Vec<TokenStream> = Vec::with_capacity(hint);

    for (variant_index, variant) in info.variants().iter().enumerate() {
        let ident = &variant.data.ident;
        let name = ident.to_string();
        let variant_path_ = quote!( Self::#ident );

        let variant_type_ident = match variant.data.fields {
            syn::Fields::Unit => Ident::new("Unit", Span::call_site()),
            syn::Fields::Unnamed(..) => Ident::new("Tuple", Span::call_site()),
            syn::Fields::Named(..) => Ident::new("Struct", Span::call_site()),
        };

        enum_variant_name.push(quote! {
            #variant_path_{..} => #name
        });
        enum_variant_index.push(quote! {
            #variant_path_{..} => #variant_index
        });
        enum_variant_kind.push(quote! {
            #variant_path_{..} => #variant_kind_::#variant_type_ident
        });

        fn process_fields(
            fields: &[StructField],
            mut f: impl FnMut(&StructField) -> bool + Sized,
        ) -> usize {
            let mut field_len = 0;
            for field in fields.iter() {
                if f(field) {
                    field_len += 1;
                }
            }
            field_len
        }

        match &variant.fields {
            EnumVariantFields::Unit => {
                enum_field_len.push(quote! { #variant_path_{..} => 0usize });
                enum_variant_unpack.push(quote! { #variant_path_{..} => ::std::vec::Vec::new() });
            }
            EnumVariantFields::Unnamed(fields) => {
                let mut vars: Vec<Ident> = Vec::with_capacity(fields.len());
                let mut mems: Vec<Member> = Vec::with_capacity(fields.len());

                let field_len = process_fields(fields, |field: &StructField| {
                    if field.is_ignore() {
                        return false;
                    }

                    let reflect_index = field.reflect_index;
                    let field_index = field.to_member();

                    enum_field_at.push(quote! {
                        #variant_path_ { #field_index : __value_xz_, .. } if #ref_index == #reflect_index => ::core::option::Option::Some(__value_xz_)
                    });

                    enum_field_at_mut.push(quote! {
                        #variant_path_ { #field_index : __value_xz_, .. } if #ref_index == #reflect_index => ::core::option::Option::Some(__value_xz_)
                    });

                    let cnt = vars.len();
                    let id = Ident::new(&format!("__item_{cnt}_"), Span::call_site());
                    vars.push(id);
                    mems.push(field_index);

                    true
                });

                enum_field_len.push(quote! {
                    #variant_path_{..} => #field_len
                });
                enum_variant_unpack.push(quote! {
                    #variant_path_ { #( #mems: #vars , )* .. } => ::std::vec![
                        #( (::core::option::Option::None, ::std::boxed::Box::new(#vars)) , )*
                    ]
                });
            }
            EnumVariantFields::Named(fields) => {
                let mut vars: Vec<Ident> = Vec::with_capacity(fields.len());
                let mut ides: Vec<Ident> = Vec::with_capacity(fields.len());
                let mut nams: Vec<String> = Vec::with_capacity(fields.len());

                let field_len = process_fields(fields, |field: &StructField| {
                    if field.is_ignore() {
                        return false;
                    }

                    let field_ident = field.data.ident.as_ref().unwrap();
                    let field_name = field_ident.to_string();
                    let field_index = field.field_index;
                    let reflect_index = field.reflect_index;

                    enum_field.push(quote! {
                        #variant_path_{ #field_ident: __value_xz_, .. } if #ref_name == #field_name => ::core::option::Option::Some(__value_xz_)
                    });
                    enum_field_mut.push(quote! {
                        #variant_path_{ #field_ident: __value_xz_, .. } if #ref_name == #field_name => ::core::option::Option::Some(__value_xz_)
                    });
                    enum_field_at.push(quote! {
                        #variant_path_{ #field_ident: __value_xz_, .. } if #ref_index == #field_index => ::core::option::Option::Some(__value_xz_)
                    });
                    enum_field_at_mut.push(quote! {
                        #variant_path_{ #field_ident: __value_xz_, .. } if #ref_index == #field_index => ::core::option::Option::Some(__value_xz_)
                    });
                    enum_name_at.push(quote! {
                        #variant_path_{ .. } if #ref_index == #field_index => ::core::option::Option::Some(#field_name)
                    });
                    enum_index_of.push(quote! {
                        #variant_path_{ .. } if #ref_name == #field_name => ::core::option::Option::Some(#reflect_index)
                    });

                    let cnt = vars.len();
                    let id = Ident::new(&format!("__item_{cnt}_"), Span::call_site());
                    vars.push(id);
                    ides.push(field_ident.clone());
                    nams.push(field_name);

                    true
                });

                enum_field_len.push(quote! {
                    #variant_path_{..} => #field_len
                });

                enum_variant_unpack.push(quote! {
                    #variant_path_ { #( #ides: #vars , )* .. } => ::std::vec![
                        #( (::core::option::Option::Some(::std::borrow::Cow::Borrowed(#nams)), ::std::boxed::Box::new(#vars)) , )*
                    ]
                });
            }
        };
    }

    quote! {
        #[automatically_derived]
        impl #impl_generics #enum_ for #real_ident #ty_generics #where_clause {
            fn field(&self, #ref_name: &str) -> ::core::option::Option<&dyn #reflect_> {
                    match self {
                    #(#enum_field,)*
                    _ => ::core::option::Option::None,
                }
            }

            fn field_at(&self, #ref_index: usize) -> ::core::option::Option<&dyn #reflect_> {
                match self {
                    #(#enum_field_at,)*
                    _ => ::core::option::Option::None,
                }
            }

            fn field_mut(&mut self, #ref_name: &str) -> ::core::option::Option<&mut dyn #reflect_> {
                    match self {
                    #(#enum_field_mut,)*
                    _ => ::core::option::Option::None,
                }
            }

            fn field_at_mut(&mut self, #ref_index: usize) -> ::core::option::Option<&mut dyn #reflect_> {
                match self {
                    #(#enum_field_at_mut,)*
                    _ => ::core::option::Option::None,
                }
            }

            fn field_index_of(&self, #ref_name: &str) -> ::core::option::Option<usize> {
                match self {
                    #(#enum_index_of,)*
                    _ => ::core::option::Option::None,
                }
            }

            fn field_name_at(&self, #ref_index: usize) -> ::core::option::Option<&str> {
                match self {
                    #(#enum_name_at,)*
                    _ => ::core::option::Option::None,
                }
            }

            #[inline]
            fn iter_fields(&self) -> #variant_field_iter_<'_> {
                #variant_field_iter_::new(self)
            }

            fn field_len(&self) -> usize {
                match self {
                    #(#enum_field_len,)*
                    _ => ::core::unreachable!(), // Used to handle `#[non_exhaustive]`
                }
            }

            fn variant_name(&self) -> &str {
                match self {
                    #(#enum_variant_name,)*
                    _ => ::core::unreachable!(), // Used to handle `#[non_exhaustive]`
                }
            }

            fn variant_index(&self) -> usize {
                match self {
                    #(#enum_variant_index,)*
                    _ => ::core::unreachable!(), // Used to handle `#[non_exhaustive]`
                }
            }

            fn variant_kind(&self) -> #variant_kind_ {
                match self {
                    #(#enum_variant_kind,)*
                    _ => ::core::unreachable!(), // Used to handle `#[non_exhaustive]`
                }
            }

            fn unpack(
                self: ::std::boxed::Box<Self>
            ) -> ::std::vec::Vec<(::core::option::Option<::std::borrow::Cow<'static, str>>, ::std::boxed::Box<dyn #reflect_>)> {
                match *self {
                    #(#enum_variant_unpack,)*
                    _ => ::core::unreachable!(), // Used to handle `#[non_exhaustive]`
                }
            }
        }
    }
}
