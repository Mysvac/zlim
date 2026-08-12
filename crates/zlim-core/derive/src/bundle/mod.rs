use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Index, Type, parse_quote};

use crate::utils::field_type_constraint;

/// Parsed `#[bundle(...)]` type-level attributes.
struct BundleAttrs {
    no_effect: bool,
}

fn parse_bundle_attrs(attrs: &[syn::Attribute]) -> syn::Result<BundleAttrs> {
    let mut x = BundleAttrs { no_effect: false };

    for attr in attrs {
        if attr.path().is_ident("bundle") {
            let Ok(param) = attr.parse_args::<syn::Ident>() else {
                return Err(syn::Error::new_spanned(
                    attr,
                    "expected `bundle(no_effect)`",
                ));
            };
            if param == "no_effect" {
                x.no_effect = true;
            } else {
                return Err(syn::Error::new_spanned(
                    attr,
                    "expected `bundle(no_effect)`",
                ));
            }
        }
    }

    Ok(x)
}

pub(crate) fn expand(ast: DeriveInput) -> TokenStream {
    let zlim_core = crate::path::zlim_core_path();
    let bundle_ = crate::path::bundle_(&zlim_core);
    let data_bundle_ = crate::path::data_bundle_(&zlim_core);
    let component_collector_ = crate::path::component_collector_(&zlim_core);
    let component_writer_ = crate::path::component_writer_(&zlim_core);
    let entity_owned_ = crate::path::entity_owned_(&zlim_core);
    let owning_ptr_ = crate::path::owning_ptr_(&zlim_core);

    let BundleAttrs { no_effect } = match parse_bundle_attrs(&ast.attrs) {
        Ok(v) => v,
        Err(e) => return e.into_compile_error(),
    };

    let type_ident = ast.ident;
    let mut generics = ast.generics;

    if generics.type_params().next().is_some() {
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote! { Self: ::core::marker::Send + ::core::marker::Sync + ::core::marker::Sized + 'static });
    } else if generics.lifetimes().next().is_some() {
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote! { Self: 'static });
    }

    // Default: fields must be Bundle (effect mode).
    // With #[bundle(no_effect)]: fields only need DataBundle.
    let field_constraint = if no_effect { &data_bundle_ } else { &bundle_ };

    let field_access: Vec<(TokenStream, &Type)> = match &ast.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => fields
                .named
                .iter()
                .map(|field| {
                    let ident = field.ident.as_ref().unwrap();
                    let ty = &field.ty;
                    field_type_constraint(&mut generics, ty, field_constraint);
                    (quote! { #ident }, ty)
                })
                .collect(),
            Fields::Unnamed(fields) => fields
                .unnamed
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    let index = Index::from(i);
                    let ty = &field.ty;
                    field_type_constraint(&mut generics, ty, field_constraint);
                    (quote! { #index }, ty)
                })
                .collect(),
            Fields::Unit => {
                let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
                return quote! {
                    const _: () = {
                        #[automatically_derived]
                        #[expect(unsafe_code, reason = "bundle implementation is unsafe.")]
                        unsafe impl #impl_generics #bundle_ for #type_ident #ty_generics #where_clause {
                            const NEED_APPLY_EFFECT: bool = false;
                            #[inline(always)]
                            fn collect(_collector: &mut #component_collector_) {}
                            #[inline(always)]
                            unsafe fn write(_data: #owning_ptr_<'_>, _writer: &mut #component_writer_) {}
                            #[inline(always)]
                            unsafe fn apply_effect(_ptr: #owning_ptr_<'_>, _entity: &mut #entity_owned_<'_>) {}
                        }
                        #[automatically_derived]
                        #[expect(unsafe_code, reason = "bundle implementation is unsafe.")]
                        unsafe impl #impl_generics #data_bundle_ for #type_ident #ty_generics #where_clause {}
                    };
                };
            }
        },
        _ => {
            return syn::Error::new_spanned(&type_ident, "Bundle can only be derived for structs")
                .into_compile_error();
        }
    };

    let collect_calls = field_access.iter().map(|(_, ty)| {
        quote! {
            <#ty as #bundle_>::collect(__collector__);
        }
    });

    let write_calls = field_access.iter().map(|(ident, ty)| {
        quote! {
            unsafe {
                let __offset__ = ::core::mem::offset_of!(Self, #ident);
                <#ty as #bundle_>::write(<#owning_ptr_>::take_field(&mut __ptr__, __offset__), __writer__);
            }
        }
    });

    let apply_effect_calls = field_access.iter().map(|(ident, ty)| {
        if no_effect {
            TokenStream::new()
        } else {
            quote! {
                unsafe {
                    let __offset__ = ::core::mem::offset_of!(Self, #ident);
                    <#ty as #bundle_>::apply_effect(<#owning_ptr_>::take_field(&mut __ptr__, __offset__), __entity__);
                }
            }
        }
    });

    let write_mut = if !field_access.is_empty() {
        quote! { mut }
    } else {
        TokenStream::new()
    };

    let apply_mut = if !field_access.is_empty() && !no_effect {
        quote! { mut }
    } else {
        TokenStream::new()
    };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let data_bundle_impl = if no_effect {
        quote! {
            #[automatically_derived]
            #[expect(unsafe_code, reason = "bundle implementation is unsafe.")]
            unsafe impl #impl_generics #data_bundle_ for #type_ident #ty_generics #where_clause {}
        }
    } else {
        TokenStream::new()
    };

    quote! {
        const _: () = {
            #[automatically_derived]
            #[expect(unsafe_code, reason = "bundle implementation is unsafe.")]
            unsafe impl #impl_generics #bundle_ for #type_ident #ty_generics #where_clause {
                const NEED_APPLY_EFFECT: bool = !#no_effect;

                fn collect(__collector__: &mut #component_collector_) {
                    #(#collect_calls)*
                }

                unsafe fn write(#write_mut __ptr__: #owning_ptr_<'_>, __writer__: &mut #component_writer_) {
                    #(#write_calls)*
                }

                #[inline(never)]
                unsafe fn apply_effect(#apply_mut __ptr__: #owning_ptr_<'_>, __entity__: &mut #entity_owned_<'_>) {
                    #(#apply_effect_calls)*
                }
            }

            #data_bundle_impl
        };
    }
}
