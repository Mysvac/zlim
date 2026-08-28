use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::{Fields, Ident};

use crate::reflect::data::{EnumVariantFields, ReflectDerive, ReflectEnum, ReflectStruct};
use crate::reflect::meta::ReflectMeta;

pub(crate) fn gen_reflect(derive: &ReflectDerive<'_>) -> TokenStream {
    let meta = derive.meta();

    let zlim_reflect = meta.zlim_reflect();

    let reflect_ = crate::path::reflect_trait(zlim_reflect);
    let reflect_kind_ = crate::path::reflect_kind(zlim_reflect);
    let reflect_ref_ = crate::path::reflect_ref(zlim_reflect);
    let reflect_mut_ = crate::path::reflect_mut(zlim_reflect);
    let reflect_owned_ = crate::path::reflect_owned(zlim_reflect);

    let real_ident = meta.ident();
    let (impl_generics, ty_generics, where_clause) = meta.split_generics();

    let reflect_kind_token = match derive {
        ReflectDerive::Struct(_) => Ident::new("Struct", Span::call_site()),
        ReflectDerive::Tuple(_) => Ident::new("Tuple", Span::call_site()),
        ReflectDerive::UnitStruct(_) => Ident::new("Opaque", Span::call_site()),
        ReflectDerive::Enum(_) => Ident::new("Enum", Span::call_site()),
        ReflectDerive::Opaque(_) => Ident::new("Opaque", Span::call_site()),
    };

    let reflect_debug_tokens = gen_debug(meta);
    let reflect_hash_tokens = gen_hash(meta);
    let reflect_eq_tokens = gen_eq(meta);
    let reflect_clone_tokens = gen_clone(derive);
    let reflect_apply_tokens = gen_apply(derive);
    let from_reflect_tokens = gen_from_reflect(derive);

    quote! {
        #[automatically_derived]
        impl #impl_generics #reflect_ for #real_ident #ty_generics #where_clause {
            #[inline]
            fn reflect_assign(
                &mut self,
                value: ::std::boxed::Box<dyn #reflect_>,
            ) -> ::core::result::Result<(), ::std::boxed::Box<dyn #reflect_>> {
                *self = *<dyn #reflect_>::downcast::<Self>(value)?;
                Ok(()) // ↑ Faster than default implementation.
            }

            #[inline]
            fn reflect_kind(&self) -> #reflect_kind_ {
                #reflect_kind_::#reflect_kind_token
            }

            #[inline]
            fn reflect_ref(&self) -> #reflect_ref_<'_> {
                #reflect_ref_::#reflect_kind_token(self)
            }

            #[inline]
            fn reflect_mut(&mut self) -> #reflect_mut_<'_> {
                #reflect_mut_::#reflect_kind_token(self)
            }

            #[inline]
            fn reflect_owned(self: ::std::boxed::Box<Self>) -> #reflect_owned_ {
                #reflect_owned_::#reflect_kind_token(self)
            }

            #reflect_debug_tokens
            #reflect_hash_tokens
            #reflect_eq_tokens
            #reflect_clone_tokens
            #reflect_apply_tokens
            #from_reflect_tokens
        }
    }
}

fn gen_debug(meta: &ReflectMeta<'_>) -> TokenStream {
    if !meta.attrs().has_debug {
        return TokenStream::new();
    }

    quote! {
        #[inline]
        fn reflect_debug(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            <Self as ::core::fmt::Debug>::fmt(self, f)
        }
    }
}

fn gen_hash(meta: &ReflectMeta<'_>) -> TokenStream {
    if !meta.attrs().has_hash {
        return TokenStream::new();
    }

    let reflect_hasher = crate::path::reflect_hasher(meta.zlim_reflect());

    quote! {
        #[inline]
        fn reflect_hash(&self) -> u64 {
            let mut hasher = #reflect_hasher();
            <Self as ::core::hash::Hash>::hash(self, &mut hasher);
            ::core::hash::Hasher::finish(&hasher)
        }
    }
}

fn gen_eq(meta: &ReflectMeta<'_>) -> TokenStream {
    if !meta.attrs().has_eq {
        return TokenStream::new();
    }

    let reflect_ = crate::path::reflect_trait(meta.zlim_reflect());

    quote! {
        #[inline]
        fn reflect_eq(&self, other: &dyn #reflect_) -> bool {
            const fn assert_impl_eq<T: ::core::cmp::Eq>() {}
            assert_impl_eq::<Self>();

            if let ::core::option::Option::Some(o) = <dyn #reflect_>::downcast_ref::<Self>(other) {
                ::core::cmp::PartialEq::eq(self, o)
            } else {
                false
            }
        }
    }
}

fn gen_clone(derive: &ReflectDerive<'_>) -> TokenStream {
    let meta = derive.meta();
    let zlim_reflect = meta.zlim_reflect();
    let reflect_ = crate::path::reflect_trait(zlim_reflect);
    let clone_error_ = crate::path::clone_error(zlim_reflect);
    let type_path_trait_ = crate::path::type_path_trait(zlim_reflect);

    if meta.attrs().has_clone {
        return quote! {
            #[inline]
            fn reflect_clone(&self) -> ::core::result::Result<::std::boxed::Box<dyn #reflect_>, #clone_error_> {
                ::core::result::Result::Ok( ::std::boxed::Box::new(
                    <Self as ::core::clone::Clone>::clone(self)
                ) as ::std::boxed::Box<dyn #reflect_> )
            }
        };
    }

    if matches!(derive, ReflectDerive::UnitStruct(_)) {
        return quote! {
            #[inline]
            fn reflect_clone(&self) -> ::core::result::Result<::std::boxed::Box<dyn #reflect_>, #clone_error_> {
                ::core::result::Result::Ok( ::std::boxed::Box::new(Self) as ::std::boxed::Box<dyn #reflect_> )
            }
        };
    }

    if matches!(derive, ReflectDerive::Opaque(_)) {
        return quote! {
            fn reflect_clone(&self) -> ::core::result::Result<::std::boxed::Box<dyn #reflect_>, #clone_error_> {
                ::core::result::Result::Err( #clone_error_::Unsupport { type_path: <Self as #type_path_trait_>::type_path() } )
            }
        };
    }

    match derive {
        ReflectDerive::Struct(r) => gen_struct_clone(r),
        ReflectDerive::Tuple(r) => gen_struct_clone(r),
        ReflectDerive::Enum(r) => gen_enum_clone(r),
        _ => unreachable!(),
    }
}

fn gen_struct_clone(info: &ReflectStruct<'_>) -> TokenStream {
    let meta = info.meta();
    let zlim_reflect = meta.zlim_reflect();
    let reflect_ = crate::path::reflect_trait(zlim_reflect);
    let clone_error_ = crate::path::clone_error(zlim_reflect);
    let reflect_clone_field = crate::path::reflect_clone_field(zlim_reflect);

    if meta.attrs().has_default {
        let tokens = info.fields().iter().filter_map(|field| {
            if field.cloneable() {
                let member = field.to_member();
                Some(quote! { __new_value__.#member = ::core::clone::Clone::clone(&self.#member); })
            } else if !field.is_ignore() {
                let field_ty = field.ty();
                let member = field.to_member();
                Some(quote! { __new_value__.#member = #reflect_clone_field::<#field_ty>(&self.#member)?; })
            } else {
                None
            }
        });

        return quote! {
            fn reflect_clone(&self) -> ::core::result::Result<::std::boxed::Box<dyn #reflect_>, #clone_error_> {
                let mut __new_value__ = <Self as ::core::default::Default>::default();
                #( #tokens )*
                ::core::result::Result::Ok(::std::boxed::Box::new(__new_value__)  as ::std::boxed::Box<dyn #reflect_>)
            }
        };
    }

    let type_path_trait_ = crate::path::type_path_trait(zlim_reflect);

    let mut unsupported: Option<String> = None;
    for field in info.fields() {
        if field.is_ignore() && !field.cloneable() && !field.defaultable() {
            unsupported = Some(field.to_member().to_token_stream().to_string());
            break;
        }
    }
    if let Some(field) = unsupported {
        return quote! {
            fn reflect_clone(&self) -> ::core::result::Result<::std::boxed::Box<dyn #reflect_>, #clone_error_> {
                ::core::result::Result::Err(#clone_error_::FieldUnsupport {
                    type_path: <Self as #type_path_trait_>::type_path(),
                    field_name: #field,
                })
            }
        };
    }

    let tokens = info.fields().iter().map(|field| {
        let field_ty = &field.data.ty;
        let member = field.to_member();
        if field.cloneable() {
            quote! { #member: ::core::clone::Clone::clone(&self.#member), }
        } else if !field.is_ignore() {
            quote! { #member: #reflect_clone_field::<#field_ty>(&self.#member)?, }
        } else {
            debug_assert!(
                field.defaultable(),
                "already checked above, see `unsupported` checker"
            );
            quote! { #member: <#field_ty as ::core::default::Default>::default(), }
        }
    });

    // There is currently a special case that is not handled: when a field is annotated with `default`
    // but not with `clone` or `ignore`, `reflect_clone` will be used for cloning.
    //
    // The current behavior is to return a "failure" directly after cloning fails.
    // However, using `Default` to construct a default value after `reflect_clone` fails
    // is also a viable alternative.
    //
    // But default construction after a cloning failure may affect the cloning semantics,
    // i.e., it may produce a value of a different type than the original.

    quote! {
        fn reflect_clone(&self) -> ::core::result::Result<::std::boxed::Box<dyn #reflect_>, #clone_error_> {
            ::core::result::Result::Ok(::std::boxed::Box::new(Self {
                #( #tokens )*
            })  as ::std::boxed::Box<dyn #reflect_>)
        }
    }
}

fn gen_enum_clone(info: &ReflectEnum<'_>) -> TokenStream {
    let meta = info.meta();
    let zlim_reflect = meta.zlim_reflect();
    let reflect_ = crate::path::reflect_trait(zlim_reflect);
    let clone_error_ = crate::path::clone_error(zlim_reflect);
    let reflect_clone_field = crate::path::reflect_clone_field(zlim_reflect);
    let type_path_trait_ = crate::path::type_path_trait(zlim_reflect);

    let mut match_tokens = TokenStream::new();

    // --------------------------------------------------------------
    // Handle all Variants
    for variant in info.variants.iter() {
        let ident = &variant.data.ident;
        let variant_path_ = quote!( Self::#ident );

        // ------------------------ Unit Variant ------------------------
        if matches!(&variant.data.fields, Fields::Unit) {
            match_tokens.extend(quote! {
                #variant_path_ => ::core::result::Result::Ok(
                    ::std::boxed::Box::new(#variant_path_) as ::std::boxed::Box<dyn #reflect_>
                ),
            });
            continue;
        }

        // ------------------------- Unsupport Checker ------------------------
        let mut unsupported: Option<String> = None;
        for field in variant.fields() {
            if field.is_ignore() && !field.cloneable() && !field.defaultable() {
                unsupported = Some(ident.to_string());
                break;
            }
        }
        if let Some(variant) = unsupported {
            match_tokens.extend(quote! {
                #variant_path_{ .. } => ::core::result::Result::Err(#clone_error_::VariantUnsupport {
                    type_path: <Self as #type_path_trait_>::type_path(),
                    variant_name: #variant,
                }),
            });
            continue;
        }

        // ------------------------- Clone Fields ------------------------
        let mut member_tokens = TokenStream::new();
        let mut clone_tokens = TokenStream::new();

        for (index, field) in variant.fields().iter().enumerate() {
            let field_ty = &field.data.ty;
            let member = field.to_member();
            let accessor = Ident::new(&format!("__mem_{index}x_"), Span::call_site());
            member_tokens.extend(quote! { #member: #accessor, });

            if field.cloneable() {
                clone_tokens.extend(quote! { #member: ::core::clone::Clone::clone(#accessor), });
            } else if !field.is_ignore() {
                clone_tokens
                    .extend(quote! { #member: #reflect_clone_field::<#field_ty>(#accessor)?, });
            } else {
                debug_assert!(
                    field.defaultable(),
                    "already checked above, see `unsupported` checker"
                );
                clone_tokens.extend(
                    quote! { #member: <#field_ty as ::core::default::Default>::default(), },
                );
            }
        }

        // There is currently a special case that is not handled: when a field is annotated with `default`
        // but not with `clone` or `ignore`, `reflect_clone` will be used for cloning.
        //
        // The current behavior is to return a "failure" directly after cloning fails.
        // However, using `Default` to construct a default value after `reflect_clone` fails
        // is also a viable alternative.
        //
        // But default construction after a cloning failure may affect the cloning semantics,
        // i.e., it may produce a value of a different type than the original.

        match_tokens.extend(quote! {
            #variant_path_{ #member_tokens } => ::core::result::Result::Ok(
                ::std::boxed::Box::new(#variant_path_ { #clone_tokens }) as ::std::boxed::Box<dyn #reflect_>
            ),
        });
    }

    quote! {
        fn reflect_clone(&self) -> ::core::result::Result<::std::boxed::Box<dyn #reflect_>, #clone_error_> {
            match self {
                #match_tokens
                _ => ::core::unreachable!(), // handle `#[non_exhaustive]`
            }
        }
    }
}

fn gen_apply(derive: &ReflectDerive<'_>) -> TokenStream {
    let meta = derive.meta();
    let zlim_reflect = meta.zlim_reflect();
    let reflect_ = crate::path::reflect_trait(zlim_reflect);
    let apply_error_ = crate::path::apply_error(zlim_reflect);

    if let Some(custom) = &meta.attrs().override_reflect_apply {
        return quote! {
            fn reflect_apply(&mut self, __value_xz_: &dyn #reflect_) -> ::core::result::Result<(), #apply_error_> {
                #custom(self, __value_xz_)
            }
        };
    }

    // ---------------------- ! enum -------------------------
    let info = match derive {
        ReflectDerive::Struct(_) => {
            let struct_apply = crate::path::struct_apply(zlim_reflect);
            return quote! {
                fn reflect_apply(&mut self, __value_xz_: &dyn #reflect_) -> ::core::result::Result<(), #apply_error_> {
                    #struct_apply(self, __value_xz_)
                }
            };
        }
        ReflectDerive::Tuple(_) => {
            let tuple_apply = crate::path::tuple_apply(zlim_reflect);
            return quote! {
                fn reflect_apply(&mut self, __value_xz_: &dyn #reflect_) -> ::core::result::Result<(), #apply_error_> {
                    #tuple_apply(self, __value_xz_)
                }
            };
        }
        ReflectDerive::UnitStruct(_) | ReflectDerive::Opaque(_) => {
            let opaque_apply = crate::path::opaque_apply(zlim_reflect);
            return quote! {
                fn reflect_apply(&mut self, __value_xz_: &dyn #reflect_) -> ::core::result::Result<(), #apply_error_> {
                    #opaque_apply(self, __value_xz_)
                }
            };
        }
        ReflectDerive::Enum(info) => info,
    };

    // ----------------------- enum --------------------------
    let input_ = Ident::new("__input_xy_", Span::call_site());
    let enum_try_apply_ = crate::path::enum_try_apply(zlim_reflect);
    let type_path_ = crate::path::type_path_trait(zlim_reflect);
    let enum_ = crate::path::enum_trait(zlim_reflect);

    let mut match_tokens = TokenStream::new();
    for variant in info.variants.iter() {
        let ident = &variant.data.ident;
        let variant_path_ = quote!( Self::#ident );
        let variant_name_ = ident.to_string();

        if matches!(&variant.data.fields, Fields::Unit) {
            match_tokens.extend(quote! {
                #variant_name_ => #variant_path_,
            });
            continue;
        }

        let mut unsupported: Option<String> = None;
        for field in variant.fields().iter() {
            if field.is_ignore() && !field.defaultable() {
                unsupported = Some(field.to_member().to_token_stream().to_string());
                break;
            }
        }
        if let Some(field) = unsupported {
            match_tokens.extend(quote! {
                #variant_name_ => return ::core::result::Result::Err(#apply_error_ {
                    src: <Self as #type_path_>::type_path(),
                    apply: <dyn #reflect_>::reflect_type_path(#input_),
                    error: ::std::format!( "the variant field `{}` is ignored but not defaultable", #field),
                }),
            });
            continue;
        }

        let mut clone_tokens = TokenStream::new();
        for field in variant.fields().iter() {
            let field_ty = &field.data.ty;
            let member = field.to_member();
            let member_str = member.to_token_stream().to_string();

            if field.is_ignore() {
                clone_tokens.extend(quote! {
                    #member: <#field_ty as ::core::default::Default>::default(),
                });
                continue;
            }

            let getter = match &field.data.ident {
                Some(id) => {
                    let name = id.to_string();
                    quote! { #enum_::field(#input_, #name) }
                }
                None => {
                    let index = field.field_index;
                    quote! { #enum_::field_at(#input_, #index) }
                }
            };

            let cloned = quote! {
                match #getter {
                    ::core::option::Option::None => return ::core::result::Result::Err(#apply_error_ {
                        src: <Self as #type_path_>::type_path(),
                        apply: <dyn #reflect_>::reflect_type_path(#input_),
                        error: ::std::format!( "the variant field `{}` does not exist", #member_str),
                    }),
                    ::core::option::Option::Some(__cloned_x_) => {
                        match <dyn #reflect_>::reflect_clone(__cloned_x_) {
                            ::core::result::Result::Ok(__cv__) => __cv__,
                            ::core::result::Result::Err(__e__) => return ::core::result::Result::Err(#apply_error_ {
                                src: <Self as #type_path_>::type_path(),
                                apply: <dyn #reflect_>::reflect_type_path(#input_),
                                error: ::std::format!( "the variant field `{}` clone failed: {}", #member_str, __e__),
                            }),
                        }
                    },
                }
            };

            clone_tokens.extend(quote! {
                #member: {
                    match <#field_ty as #reflect_>::from_reflect(#cloned){
                        ::core::result::Result::Ok(__v__) => *__v__,
                        ::core::result::Result::Err(__e__) => return ::core::result::Result::Err(#apply_error_ {
                            src: <Self as #type_path_>::type_path(),
                            apply: <dyn #reflect_>::reflect_type_path(#input_),
                            error: ::std::format!( "the variant field `{}` convert failed: `{:?}`", #member_str, &*__e__),
                        }),
                    }
                },
            });
        }

        match_tokens.extend(quote! { #variant_name_ => #variant_path_ { #clone_tokens }, });
    }

    quote! {
        fn reflect_apply(&mut self, #input_: &dyn #reflect_) -> ::core::result::Result<(), #apply_error_> {
            if let Err(#input_) = #enum_try_apply_(self, #input_)? {
                *self = match #enum_::variant_name(#input_) {
                    #match_tokens
                    __name_error_ => return ::core::result::Result::Err(#apply_error_ {
                        src: <Self as #type_path_>::type_path(),
                        apply: <dyn #reflect_>::reflect_type_path(#input_),
                        error: ::std::format!( "invalid variant name: {}", __name_error_),
                    }),
                };
            }

            ::core::result::Result::Ok(())
        }
    }
}

fn gen_from_reflect(derive: &ReflectDerive<'_>) -> TokenStream {
    let meta = derive.meta();
    let zlim_reflect = meta.zlim_reflect();
    let reflect_ = crate::path::reflect_trait(zlim_reflect);
    let type_db_ = crate::path::type_db(zlim_reflect);

    if let Some(custom) = &meta.attrs().override_from_reflect {
        return quote! {
            #[inline]
            fn from_reflect(
                value: ::std::boxed::Box<dyn #reflect_>
            ) -> ::core::result::Result<::std::boxed::Box<Self>, ::std::boxed::Box<dyn #reflect_>>
            where
                Self: Sized
            {
                #custom(value)
            }
        };
    }

    if matches!(
        derive,
        ReflectDerive::Opaque(_) | ReflectDerive::UnitStruct(_)
    ) {
        return quote! {
            fn from_reflect(
                value: ::std::boxed::Box<dyn #reflect_>
            ) -> ::core::result::Result<::std::boxed::Box<Self>, ::std::boxed::Box<dyn #reflect_>>
            where
                Self: Sized
            {
                // Phase 1: same type → downcast
                let mut value = match <dyn #reflect_>::downcast::<Self>(value) {
                    ::core::result::Result::Ok(ret) => return ::core::result::Result::Ok(ret),
                    ::core::result::Result::Err(e) => e,
                };

                // Phase 2: TypeDB conversion
                if let ::core::option::Option::Some(db) = <dyn #reflect_>::type_db(&*value) {
                    match #type_db_::convert(db, value, ::core::any::TypeId::of::<Self>()) {
                        ::core::result::Result::Ok(ret) => {
                            return ::core::result::Result::Ok(
                                <dyn #reflect_>::downcast::<Self>(ret).unwrap()
                            );
                        }
                        ::core::result::Result::Err(v) => value = v,
                    }
                }

                ::core::result::Result::Err(value)
            }
        };
    }

    match derive {
        ReflectDerive::Struct(r) => gen_struct_from_reflect(r),
        ReflectDerive::Tuple(r) => gen_tuple_from_reflect(r),
        ReflectDerive::Enum(r) => gen_enum_from_reflect(r),
        _ => unreachable!(),
    }
}

fn gen_struct_from_reflect(info: &ReflectStruct) -> TokenStream {
    let meta = info.meta();
    let zlim_reflect = meta.zlim_reflect();
    let reflect_ = crate::path::reflect_trait(zlim_reflect);
    let reflect_kind_ = crate::path::reflect_kind(zlim_reflect);
    let type_db_ = crate::path::type_db(zlim_reflect);
    let struct_ = crate::path::struct_trait(zlim_reflect);
    let is_convertable_ = crate::path::is_convertable(zlim_reflect);

    // ------------------------- Some Type Checker ------------------------
    let phase_1_2 = quote! {
        let mut value = match <dyn #reflect_>::downcast::<Self>(value) {
            ::core::result::Result::Ok(ret) => return ::core::result::Result::Ok(ret),
            ::core::result::Result::Err(e) => e,
        };

        if let ::core::option::Option::Some(db) = <dyn #reflect_>::type_db(&*value) {
            match #type_db_::convert(db, value, ::core::any::TypeId::of::<Self>()) {
                ::core::result::Result::Ok(ret) => {
                    return ::core::result::Result::Ok(
                        ::core::result::Result::unwrap(<dyn #reflect_>::downcast::<Self>(ret))
                    );
                }
                ::core::result::Result::Err(v) => value = v,
            }
        }
    };

    // ------------------------- Fail Path --------------------------

    let cannot_construct: bool = info
        .fields()
        .iter()
        .any(|f| f.is_ignore() && !f.defaultable());

    if !meta.attrs().has_default && cannot_construct {
        return quote! {
            fn from_reflect(
                value: ::std::boxed::Box<dyn #reflect_>
            ) -> ::core::result::Result<::std::boxed::Box<Self>, ::std::boxed::Box<dyn #reflect_>>
            where
                Self: Sized
            {
                #phase_1_2

                ::core::result::Result::Err(value)
            }
        };
    }

    // ------------------------- Reflect Kind checker --------------------------

    let value_item = Ident::new("__val_xy_", Span::call_site());

    let into_struct_tokens = quote! {
        if <dyn #reflect_>::reflect_kind(&*value) != #reflect_kind_::Struct {
            return ::core::result::Result::Err(value);
        }

        let #value_item: ::std::boxed::Box<dyn #struct_> = ::core::result::Result::unwrap(
            <dyn #reflect_>::reflect_owned(value).into_struct()
        );
    };

    // ------------------------- Construct Self --------------------------------

    let convert_checks = info.active_fields().map(|field| {
        let field_ty = field.ty();
        let field_name = field.data.ident.as_ref().unwrap().to_string();

        if field.defaultable() {
            quote! {
                if let ::core::option::Option::Some(__field_x_) = #struct_::field(&*#value_item, #field_name)
                    && !#is_convertable_(__field_x_, ::core::any::TypeId::of::<#field_ty>())
                {
                    return ::core::result::Result::Err(#value_item);
                }
            }
        } else {
            quote! {
                match #struct_::field(&*#value_item, #field_name) {
                    ::core::option::Option::None => return ::core::result::Result::Err(#value_item),
                    ::core::option::Option::Some(__field_x_) => {
                        if !#is_convertable_(__field_x_, ::core::any::TypeId::of::<#field_ty>()) {
                            return ::core::result::Result::Err(#value_item);
                        }
                    }
                }
            }
        }
    });

    let contains_active_fields: bool = info.active_fields().next().is_some();
    let field_kv_val = Ident::new("__kv_val_", Span::call_site());

    // ------------------------- Defautable Pash --------------------------

    if meta.attrs().has_default {
        let this_val = Ident::new("__this_valx_", Span::call_site());

        let create_this = quote! {
            let mut #this_val = <Self as ::core::default::Default>::default();
        };

        let assignments = info.active_fields().map(|field| {
            let field_ty = field.ty();
            let member = field.to_member();
            let field_name = field.data.ident.as_ref().unwrap().to_string();

            quote! {
                #field_name => {
                    #this_val.#member = * ::core::result::Result::unwrap(<#field_ty as #reflect_>::from_reflect(#field_kv_val));
                },
            }
        });

        let assign_active_fields = if contains_active_fields {
            quote! {
                for ( __kv_key_, #field_kv_val ) in <dyn #struct_>::unpack(#value_item) {
                    match <::std::borrow::Cow<str> as ::core::convert::AsRef<str>>::as_ref(&__kv_key_) {
                        #(#assignments)*
                        _ => {},
                    }
                }
            }
        } else {
            TokenStream::new()
        };

        return quote! {
            fn from_reflect(
                value: ::std::boxed::Box<dyn #reflect_>
            ) -> ::core::result::Result<::std::boxed::Box<Self>, ::std::boxed::Box<dyn #reflect_>>
            where
                Self: Sized
            {
                #phase_1_2

                #into_struct_tokens

                #(#convert_checks)*

                #create_this

                #assign_active_fields

                ::core::result::Result::Ok(::std::boxed::Box::new(#this_val))
            }
        };
    }

    // ------------------------- Fields Construct --------------------------

    #[inline]
    fn get_item_ident(idx: usize) -> Ident {
        Ident::new(&format!("__item_f{idx}_"), Span::call_site())
    }

    let def_active_fields = info.active_fields().map(|field| {
        let field_ty = field.ty();

        let item_ident = get_item_ident(field.field_index);

        quote! {
            let mut #item_ident: ::core::option::Option<#field_ty> = ::core::option::Option::None;
        }
    });

    let contains_active_fields: bool = info.active_fields().next().is_some();

    let field_kv_val = Ident::new("__kv_val_", Span::call_site());

    let assignments = info.active_fields().map(|field| {
        let field_ty = field.ty();
        let field_name = field.data.ident.as_ref().unwrap().to_string();

        let item_ident = get_item_ident(field.field_index);

        quote! {
            #field_name => {
                #item_ident = ::core::option::Option::Some(
                    * ::core::result::Result::unwrap(<#field_ty as #reflect_>::from_reflect(#field_kv_val))
                );
            },
        }
    });

    let assign_active_fields = if contains_active_fields {
        quote! {
            for ( __kv_key_, #field_kv_val ) in <dyn #struct_>::unpack(#value_item) {
                match <::std::borrow::Cow<str> as ::core::convert::AsRef<str>>::as_ref(&__kv_key_) {
                    #(#assignments)*
                    _ => {},
                }
            }
        }
    } else {
        TokenStream::new()
    };

    let field_tokens = info.fields().iter().map(|field| {
        let member = field.to_member();

        if field.is_ignore() {
            let field_ty = field.ty();
            return quote! { #member: <#field_ty as ::core::default::Default>::default(), };
        }

        let item_ident = get_item_ident(field.field_index);

        if field.defaultable() {
            quote! { #member: ::core::option::Option::unwrap_or_default(#item_ident), }
        } else {
            quote! { #member: ::core::option::Option::unwrap(#item_ident), }
        }
    });

    quote! {
        fn from_reflect(
            value: ::std::boxed::Box<dyn #reflect_>
        ) -> ::core::result::Result<::std::boxed::Box<Self>, ::std::boxed::Box<dyn #reflect_>>
        where
            Self: Sized
        {
            #phase_1_2

            #into_struct_tokens

            #(#convert_checks)*

            #(#def_active_fields)*

            #assign_active_fields

            ::core::result::Result::Ok(::std::boxed::Box::new(Self { #(#field_tokens)* }))
        }
    }
}

fn gen_tuple_from_reflect(info: &ReflectStruct) -> TokenStream {
    let meta = info.meta();
    let zlim_reflect = meta.zlim_reflect();
    let reflect_ = crate::path::reflect_trait(zlim_reflect);
    let reflect_kind_ = crate::path::reflect_kind(zlim_reflect);
    let type_db_ = crate::path::type_db(zlim_reflect);
    let tuple_ = crate::path::tuple_trait(zlim_reflect);
    let is_convertable_ = crate::path::is_convertable(zlim_reflect);

    // Phase 1 & 2 — shared for both paths.
    let phase_1_2 = quote! {
        // Phase 1: same type → downcast
        let mut value = match <dyn #reflect_>::downcast::<Self>(value) {
            ::core::result::Result::Ok(ret) => return ::core::result::Result::Ok(ret),
            ::core::result::Result::Err(e) => e,
        };

        // Phase 2: TypeDB conversion
        if let ::core::option::Option::Some(db) = <dyn #reflect_>::type_db(&*value) {
            match #type_db_::convert(db, value, ::core::any::TypeId::of::<Self>()) {
                ::core::result::Result::Ok(ret) => {
                    return ::core::result::Result::Ok(
                        <dyn #reflect_>::downcast::<Self>(ret).unwrap()
                    );
                }
                ::core::result::Result::Err(v) => value = v,
            }
        }
    };

    if info
        .fields()
        .iter()
        .any(|f| f.is_ignore() && !f.defaultable())
    {
        return quote! {
            fn from_reflect(
                value: ::std::boxed::Box<dyn #reflect_>
            ) -> ::core::result::Result<::std::boxed::Box<Self>, ::std::boxed::Box<dyn #reflect_>>
            where
                Self: Sized
            {
                #phase_1_2

                ::core::result::Result::Err(value)
            }
        };
    }

    let value_item = Ident::new("__val_xy_", Span::call_site());

    let into_tuple_tokens = quote! {
        if <dyn #reflect_>::reflect_kind(&*value) != #reflect_kind_::Tuple {
            return ::core::result::Result::Err(value);
        }

        let #value_item: ::std::boxed::Box<dyn #tuple_> = ::core::result::Result::unwrap(
            <dyn #reflect_>::reflect_owned(value).into_tuple()
        );
    };

    let convert_checks = info.active_fields().map(|field| {
        let field_ty = field.ty();
        let index = field.reflect_index;
        quote! {
            match #tuple_::field(&*#value_item, #index) {
                ::core::option::Option::None => return ::core::result::Result::Err(#value_item),
                ::core::option::Option::Some(__field_x_) => {
                    if !#is_convertable_(__field_x_, ::core::any::TypeId::of::<#field_ty>()) {
                        return ::core::result::Result::Err(#value_item);
                    }
                }
            }
        }
    });

    #[inline]
    fn get_item_ident(idx: usize) -> Ident {
        Ident::new(&format!("__item_f{idx}_"), Span::call_site())
    }

    let def_active_fields = info.active_fields().map(|field| {
        let field_ty = field.ty();
        let item_ident = get_item_ident(field.field_index);

        quote!{ let mut #item_ident: ::core::option::Option<#field_ty> = ::core::option::Option::None; }
    });

    let contains_active_fields: bool = info.active_fields().next().is_some();

    let field_val = Ident::new("__field_val_", Span::call_site());

    let assignments = info.active_fields().map(|field| {
        let field_ty = field.ty();
        let field_index = field.reflect_index;

        let item_ident = get_item_ident(field.field_index);

        quote! {
            #field_index => {
                #item_ident = ::core::option::Option::Some(
                    * ::core::result::Result::unwrap(<#field_ty as #reflect_>::from_reflect(#field_val))
                );
            },
        }
    });

    let assign_active_fields = if contains_active_fields {
        quote! {
            let mut __kv_key_ = 0usize;
            for #field_val in <dyn #tuple_>::unpack(#value_item) {
                match __kv_key_ {
                    #(#assignments)*
                    _ => {},
                }
                __kv_key_ += 1;
            }
        }
    } else {
        quote! { let _ = #value_item; }
    };

    let field_tokens = info.fields().iter().map(|field| {
        let member = field.to_member();

        if field.is_ignore() {
            let field_ty = field.ty();
            return quote! { #member: <#field_ty as ::core::default::Default>::default(), };
        }

        let item_ident = get_item_ident(field.field_index);

        // fields do not support default.
        quote! { #member: ::core::option::Option::unwrap(#item_ident), }
    });

    quote! {
        fn from_reflect(
            value: ::std::boxed::Box<dyn #reflect_>
        ) -> ::core::result::Result<::std::boxed::Box<Self>, ::std::boxed::Box<dyn #reflect_>>
        where
            Self: Sized
        {
            #phase_1_2

            #into_tuple_tokens

            #(#convert_checks)*

            #(#def_active_fields)*

            #assign_active_fields

            ::core::result::Result::Ok(::std::boxed::Box::new(Self { #(#field_tokens)* }))
        }
    }
}

fn gen_enum_from_reflect(info: &ReflectEnum) -> TokenStream {
    let meta = info.meta();
    let zlim_reflect = meta.zlim_reflect();
    let reflect_ = crate::path::reflect_trait(zlim_reflect);
    let reflect_kind_ = crate::path::reflect_kind(zlim_reflect);
    let type_db_ = crate::path::type_db(zlim_reflect);
    let enum_ = crate::path::enum_trait(zlim_reflect);
    let variant_kind_ = crate::path::variant_kind(zlim_reflect);
    let is_convertable_ = crate::path::is_convertable(zlim_reflect);

    let phase_1_2 = quote! {
        let mut value = match <dyn #reflect_>::downcast::<Self>(value) {
            ::core::result::Result::Ok(ret) => return ::core::result::Result::Ok(ret),
            ::core::result::Result::Err(e) => e,
        };

        if let ::core::option::Option::Some(db) = <dyn #reflect_>::type_db(&*value) {
            match #type_db_::convert(db, value, ::core::any::TypeId::of::<Self>()) {
                ::core::result::Result::Ok(ret) => {
                    return ::core::result::Result::Ok(
                        ::core::result::Result::unwrap(<dyn #reflect_>::downcast::<Self>(ret))
                    );
                }
                ::core::result::Result::Err(v) => value = v,
            }
        }
    };

    let value_item = Ident::new("__val_xy_", Span::call_site());

    let into_enum_tokens = quote! {
        if <dyn #reflect_>::reflect_kind(&*value) != #reflect_kind_::Enum {
            return ::core::result::Result::Err(value);
        }

        let #value_item: ::std::boxed::Box<dyn #enum_> = ::core::result::Result::unwrap(
            <dyn #reflect_>::reflect_owned(value).into_enum()
        );
    };

    #[inline]
    fn get_item_ident(idx: usize) -> Ident {
        Ident::new(&format!("__item_f{idx}_"), Span::call_site())
    }

    let field_kv_val = Ident::new("__kv_val_", Span::call_site());

    let variant_checks = info.variants().iter().map(|variant| {
        let ident = &variant.data.ident;
        let variant_name = ident.to_string();

        if variant.fields().iter().any(|f| f.is_ignore() && !f.defaultable()) {
            return quote! {
                #variant_name => return ::core::result::Result::Err(#value_item),
            };
        }

        match &variant.fields {
            EnumVariantFields::Unit => {
                quote! {
                    #variant_name => {
                        if #enum_::variant_kind(&*#value_item) != #variant_kind_::Unit {
                            return ::core::result::Result::Err(#value_item);
                        }
                        ::core::result::Result::Ok(::std::boxed::Box::new(Self::#ident))
                    },
                }
            }
            EnumVariantFields::Unnamed(fields) => {
                let active_cnt =
                    fields.iter().filter(|f| !f.is_ignore()).count();

                // Phase 4: convert_checks.
                let convert_checks = fields
                    .iter()
                    .filter(|f| !f.is_ignore())
                    .map(|field| {
                        let field_ty = field.ty();
                        let index = field.reflect_index;
                        quote! {
                            match #enum_::field_at(&*#value_item, #index) {
                                ::core::option::Option::None => {
                                    return ::core::result::Result::Err(#value_item);
                                }
                                ::core::option::Option::Some(__field_x_) => {
                                    if !#is_convertable_(__field_x_, ::core::any::TypeId::of::<#field_ty>()) {
                                        return ::core::result::Result::Err(#value_item);
                                    }
                                }
                            }
                        }
                    });

                // Declare Option<FieldTy> per active field.
                let def_active_fields = fields.iter().filter(|f| !f.is_ignore()).map(|field| {
                    let field_ty = field.ty();
                    let item_ident = get_item_ident(field.field_index);
                    quote! {
                        let mut #item_ident: ::core::option::Option<#field_ty> = ::core::option::Option::None;
                    }
                });

                let contains_active = active_cnt > 0;

                // Match unpack items by index.
                let assignments = fields.iter().filter(|f| !f.is_ignore()).map(|field| {
                    let field_ty = field.ty();
                    let field_index = field.field_index;
                    let item_ident = get_item_ident(field_index);
                    quote! {
                        #field_index => {
                            #item_ident = ::core::option::Option::Some(
                                * ::core::result::Result::unwrap(<#field_ty as #reflect_>::from_reflect(#field_kv_val))
                            );
                        },
                    }
                });

                let assign_active_fields = if contains_active {
                    quote! {
                        let mut __kv_key_ = 0usize;
                        for (_, #field_kv_val) in <dyn #enum_>::unpack(#value_item) {
                            match __kv_key_ {
                                #(#assignments)*
                                _ => {},
                            }
                            __kv_key_ += 1;
                        }
                    }
                } else {
                    quote! { let _ = #value_item; }
                };

                // Construct variant fields.
                let field_tokens = fields.iter().map(|field| {
                    if field.is_ignore() {
                        let field_ty = field.ty();
                        quote! { <#field_ty as ::core::default::Default>::default() }
                    } else {
                        let item_ident = get_item_ident(field.field_index);
                        quote! { ::core::option::Option::unwrap(#item_ident) }
                    }
                });

                quote! {
                    #variant_name => {
                        if #enum_::variant_kind(&*#value_item) != #variant_kind_::Tuple {
                            return ::core::result::Result::Err(#value_item);
                        }
                        if #enum_::field_len(&*#value_item) != #active_cnt {
                            return ::core::result::Result::Err(#value_item);
                        }

                        #(#convert_checks)*

                        #(#def_active_fields)*

                        #assign_active_fields

                        ::core::result::Result::Ok(
                            ::std::boxed::Box::new(Self::#ident(#(#field_tokens,)*))
                        )
                    },
                }
            }
            EnumVariantFields::Named(fields) => {
                // Phase 4: convert_checks.
                let convert_checks = fields.iter().filter(|f| !f.is_ignore()).map(|field| {
                    let field_ty = field.ty();
                    let field_name = field.data.ident.as_ref().unwrap().to_string();

                    if field.defaultable() {
                        quote! {
                            if let ::core::option::Option::Some(__field_x_) = #enum_::field(&*#value_item, #field_name)
                                && !#is_convertable_(__field_x_, ::core::any::TypeId::of::<#field_ty>())
                            {
                                return ::core::result::Result::Err(#value_item);
                            }
                        }
                    } else {
                        quote! {
                            match #enum_::field(&*#value_item, #field_name) {
                                ::core::option::Option::None => {
                                    return ::core::result::Result::Err(#value_item);
                                }
                                ::core::option::Option::Some(__field_x_) => {
                                    if !#is_convertable_(__field_x_, ::core::any::TypeId::of::<#field_ty>()) {
                                        return ::core::result::Result::Err(#value_item);
                                    }
                                }
                            }
                        }
                    }
                });

                // Declare Option<FieldTy> per active field.
                let def_active_fields = fields.iter().filter(|f| !f.is_ignore()).map(|field| {
                    let field_ty = field.ty();
                    let item_ident = get_item_ident(field.field_index);
                    quote! {
                        let mut #item_ident: ::core::option::Option<#field_ty> = ::core::option::Option::None;
                    }
                });

                let contains_active = fields.iter().any(|f| !f.is_ignore());

                // Match unpack items by field name.
                let assignments = fields.iter().filter(|f| !f.is_ignore()).map(|field| {
                    let field_ty = field.ty();
                    let field_name = field.data.ident.as_ref().unwrap().to_string();
                    let item_ident = get_item_ident(field.field_index);
                    quote! {
                        #field_name => {
                            #item_ident = ::core::option::Option::Some(
                                * ::core::result::Result::unwrap(<#field_ty as #reflect_>::from_reflect(#field_kv_val))
                            );
                        },
                    }
                });

                let assign_active_fields = if contains_active {
                    quote! {
                        for (__kv_key_, #field_kv_val) in <dyn #enum_>::unpack(#value_item) {
                            let __kv_key2_ = ::core::option::Option::unwrap(__kv_key_);
                            match <::std::borrow::Cow<str> as ::core::convert::AsRef<str>>::as_ref(&__kv_key2_) {
                                #(#assignments)*
                                _ => {},
                            }
                        }
                    }
                } else {
                    quote! { let _ = #value_item; }
                };

                // Construct variant fields.
                let field_tokens = fields.iter().map(|field| {
                    let member = field.to_member();

                    if field.is_ignore() {
                        let field_ty = field.ty();
                        return quote! { #member: <#field_ty as ::core::default::Default>::default() };
                    }

                    let item_ident = get_item_ident(field.field_index);
                    if field.defaultable() {
                        quote! {
                            #member: ::core::option::Option::unwrap_or_default(#item_ident)
                        }
                    } else {
                        quote! {
                            #member: ::core::option::Option::unwrap(#item_ident)
                        }
                    }
                });

                quote! {
                    #variant_name => {
                        if #enum_::variant_kind(&*#value_item) != #variant_kind_::Struct {
                            return ::core::result::Result::Err(#value_item);
                        }

                        #(#convert_checks)*

                        #(#def_active_fields)*

                        #assign_active_fields

                        ::core::result::Result::Ok(
                            ::std::boxed::Box::new(Self::#ident {
                                #(#field_tokens,)*
                            })
                        )
                    },
                }
            }
        }
    });

    quote! {
        fn from_reflect(
            value: ::std::boxed::Box<dyn #reflect_>
        ) -> ::core::result::Result<::std::boxed::Box<Self>, ::std::boxed::Box<dyn #reflect_>>
        where
            Self: Sized
        {
            #phase_1_2

            #into_enum_tokens

            match #enum_::variant_name(&*#value_item) {
                #(#variant_checks)*
                _ => ::core::result::Result::Err(#value_item),
            }
        }
    }
}
