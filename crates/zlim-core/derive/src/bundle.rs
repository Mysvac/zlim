use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Index, Type, parse_quote};

use crate::utils::field_type_constraint;

pub(crate) fn expand(ast: DeriveInput) -> TokenStream {
    let zlim_core = crate::path::zlim_core_path();
    let bundle_ = crate::path::bundle_(&zlim_core);
    let bundle_writer_ = crate::path::bundle_writer_(&zlim_core);
    let component_collector_ = crate::path::component_collector_(&zlim_core);
    let component_writer_ = crate::path::component_writer_(&zlim_core);
    let components_ = crate::path::components_(&zlim_core);
    let owning_ptr_ = crate::path::owning_ptr_(&zlim_core);

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

    let field_access: Vec<(TokenStream, &Type)> = match &ast.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields) => fields
                .named
                .iter()
                .map(|field| {
                    let ident = field.ident.as_ref().unwrap();
                    let ty = &field.ty;
                    field_type_constraint(&mut generics, ty, &bundle_);
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
                    field_type_constraint(&mut generics, ty, &bundle_);
                    (quote! { #index }, ty)
                })
                .collect(),
            Fields::Unit => {
                let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
                return quote! {
                    const _: () = {
                        #[automatically_derived]
                        // #[expect(unsafe_code, reason = "bundle implementation is unsafe.")]
                        // No needed and cannot use `expect`, `forbid(unsafe_code)` disallows it.
                        unsafe impl #impl_generics #bundle_ for #type_ident #ty_generics #where_clause {
                            #[inline(always)]
                            fn collect_explicit(_collector: &mut #component_collector_) {}
                            #[inline(always)]
                            fn collect_required(_collector: &mut #component_collector_) {}
                            #[inline(always)]
                            unsafe fn push_to(_data: #owning_ptr_<'_>, _writer: &mut #bundle_writer_, _infos: ::core::option::Option<&#components_>) {}
                            #[inline(always)]
                            unsafe fn write_explicit(_data: #owning_ptr_<'_>, _writer: &mut #component_writer_) {}
                            #[inline(always)]
                            unsafe fn write_required(_writer: &mut #component_writer_) {}
                        }
                    };
                };
            }
        },
        _ => {
            return syn::Error::new_spanned(&type_ident, "Bundle can only be derived for structs")
                .into_compile_error();
        }
    };

    let collect_explicit_calls = field_access.iter().map(|(_, ty)| {
        quote! {
            <#ty as #bundle_>::collect_explicit(__collector__);
        }
    });

    let collect_required_calls = field_access.iter().map(|(_, ty)| {
        quote! {
            <#ty as #bundle_>::collect_required(__collector__);
        }
    });

    let push_calls = field_access.iter().map(|(ident, ty)| {
        quote! {
            unsafe {
                let __offset__ = ::core::mem::offset_of!(Self, #ident);
                <#ty as #bundle_>::push_to(<#owning_ptr_>::take_field(&mut __ptr__, __offset__), __writer__, __infos__);
            }
        }
    });

    let write_calls = field_access.iter().map(|(ident, ty)| {
        quote! {
            unsafe {
                let __offset__ = ::core::mem::offset_of!(Self, #ident);
                <#ty as #bundle_>::write_explicit(<#owning_ptr_>::take_field(&mut __ptr__, __offset__), __writer__);
            }
        }
    });

    let write_required_calls = field_access.iter().map(|(_, ty)| {
        quote! {
            unsafe {
                <#ty as #bundle_>::write_required(__writer__);
            }
        }
    });

    let write_mut = if field_access.is_empty() {
        TokenStream::new()
    } else {
        quote! { mut }
    };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        const _: () = {
            #[automatically_derived]
            // #[expect(unsafe_code, reason = "bundle implementation is unsafe.")]
            // No needed and cannot use `expect`, `forbid(unsafe_code)` disallows it.
            unsafe impl #impl_generics #bundle_ for #type_ident #ty_generics #where_clause {
                fn collect_explicit(__collector__: &mut #component_collector_) {
                    #(#collect_explicit_calls)*
                }

                fn collect_required(__collector__: &mut #component_collector_) {
                    #(#collect_required_calls)*
                }

                unsafe fn push_to(#write_mut __ptr__: #owning_ptr_<'_>, __writer__: &mut #bundle_writer_, __infos__: ::core::option::Option<&#components_>) {
                    #(#push_calls)*
                }

                unsafe fn write_explicit(#write_mut __ptr__: #owning_ptr_<'_>, __writer__: &mut #component_writer_) {
                    #(#write_calls)*
                }

                unsafe fn write_required(__writer__: &mut #component_writer_) {
                    #(#write_required_calls)*
                }
            }
        };
    }
}
