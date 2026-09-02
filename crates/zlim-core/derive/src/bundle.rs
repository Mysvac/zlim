use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Index, Type, parse_quote};

use crate::utils::field_type_constraint;

/// Parsed `#[bundle(...)]` type-level attributes.
struct BundleAttrs {
    data: bool,
}

fn parse_bundle_attrs(attrs: &[syn::Attribute]) -> syn::Result<BundleAttrs> {
    let mut x = BundleAttrs { data: false };

    for attr in attrs {
        if attr.path().is_ident("bundle") {
            let Ok(param) = attr.parse_args::<syn::Ident>() else {
                return Err(syn::Error::new_spanned(attr, "expected `bundle(data)`"));
            };
            if param == "data" {
                x.data = true;
            } else {
                return Err(syn::Error::new_spanned(attr, "expected `bundle(data)`"));
            }
        }
    }

    Ok(x)
}

/// Emits `unsafe impl DataBundle for Type`, asserting that the type is a
/// pure-data bundle (no post-spawn side effects).
fn data_bundle_impl(
    data_bundle_: &TokenStream,
    type_ident: &syn::Ident,
    impl_generics: &syn::ImplGenerics<'_>,
    ty_generics: &syn::TypeGenerics<'_>,
    where_clause: Option<&syn::WhereClause>,
) -> TokenStream {
    quote! {
        #[automatically_derived]
        // #[expect(unsafe_code, reason = "bundle implementation is unsafe.")]
        // No needed and cannot use `expect`, `forbid(unsafe_code)` disallows it.
        unsafe impl #impl_generics #data_bundle_ for #type_ident #ty_generics #where_clause {}
    }
}

pub(crate) fn expand(ast: DeriveInput) -> TokenStream {
    let zlim_core = crate::path::zlim_core_path();
    let bundle_ = crate::path::bundle_(&zlim_core);
    let data_bundle_ = crate::path::data_bundle_(&zlim_core);
    let component_collector_ = crate::path::component_collector_(&zlim_core);
    let component_writer_ = crate::path::component_writer_(&zlim_core);
    let entity_owned_ = crate::path::entity_owned_(&zlim_core);
    let owning_ptr_ = crate::path::owning_ptr_(&zlim_core);

    let BundleAttrs { data } = match parse_bundle_attrs(&ast.attrs) {
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

    // Default: fields must be Bundle.
    // With #[bundle(data)]: fields only need DataBundle.
    let field_constraint = if data { &data_bundle_ } else { &bundle_ };

    let field_access: Vec<(TokenStream, &Type)> = match &ast.data {
        Data::Struct(data_struct) => match &data_struct.fields {
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
                let data_bundle_impl = if data {
                    data_bundle_impl(
                        &data_bundle_,
                        &type_ident,
                        &impl_generics,
                        &ty_generics,
                        where_clause,
                    )
                } else {
                    TokenStream::new()
                };
                return quote! {
                    const _: () = {
                        #[automatically_derived]
                        // #[expect(unsafe_code, reason = "bundle implementation is unsafe.")]
                        // No needed and cannot use `expect`, `forbid(unsafe_code)` disallows it.
                        unsafe impl #impl_generics #bundle_ for #type_ident #ty_generics #where_clause {
                            const NEED_APPLY_EFFECT: bool = false;
                            #[inline(always)]
                            fn collect_explicit(_collector: &mut #component_collector_) {}
                            #[inline(always)]
                            fn collect_required(_collector: &mut #component_collector_) {}
                            #[inline(always)]
                            unsafe fn write_explicit(_data: #owning_ptr_<'_>, _writer: &mut #component_writer_) {}
                            #[inline(always)]
                            unsafe fn write_required(_writer: &mut #component_writer_) {}
                            #[inline(always)]
                            unsafe fn apply_effect(_ptr: #owning_ptr_<'_>, _entity: &mut #entity_owned_<'_>) {}
                        }
                        #data_bundle_impl
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

    // `NEED_APPLY_EFFECT` is the logical OR of all field types' flags — a
    // bundle needs `apply_effect` if any of its sub-bundles does.
    let need_apply_effect = field_access.iter().map(|(_, ty)| {
        quote! { || <#ty as #bundle_>::NEED_APPLY_EFFECT }
    });

    // `#[bundle(data)]` fields are `DataBundle`s whose `apply_effect` is a
    // no-op, so no calls are emitted.
    let apply_effect_calls = field_access.iter().map(|(ident, ty)| {
        if data {
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

    let apply_mut = if !field_access.is_empty() && !data {
        quote! { mut }
    } else {
        TokenStream::new()
    };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let data_bundle_impl = if data {
        data_bundle_impl(
            &data_bundle_,
            &type_ident,
            &impl_generics,
            &ty_generics,
            where_clause,
        )
    } else {
        TokenStream::new()
    };

    let static_no_effect_assert = if data_bundle_impl.is_empty() {
        TokenStream::new()
    } else {
        quote! {
            const {
                ::core::assert!(
                    !<Self as #bundle_>::NEED_APPLY_EFFECT,
                    "try implement DataBundle for a Bundle that `NEED_APPLY_EFFECT = true`",
                );
            }
        }
    };

    quote! {
        const _: () = {
            #[automatically_derived]
            // #[expect(unsafe_code, reason = "bundle implementation is unsafe.")]
            // No needed and cannot use `expect`, `forbid(unsafe_code)` disallows it.
            unsafe impl #impl_generics #bundle_ for #type_ident #ty_generics #where_clause {
                const NEED_APPLY_EFFECT: bool = false #(#need_apply_effect)*;

                fn collect_explicit(__collector__: &mut #component_collector_) {
                    #(#collect_explicit_calls)*
                }

                fn collect_required(__collector__: &mut #component_collector_) {
                    #(#collect_required_calls)*
                }

                unsafe fn write_explicit(#write_mut __ptr__: #owning_ptr_<'_>, __writer__: &mut #component_writer_) {
                    #(#write_calls)*
                }

                unsafe fn write_required(__writer__: &mut #component_writer_) {
                    #(#write_required_calls)*
                }

                unsafe fn apply_effect(#apply_mut __ptr__: #owning_ptr_<'_>, __entity__: &mut #entity_owned_<'_>) {
                    #static_no_effect_assert
                    if <Self as #bundle_>::NEED_APPLY_EFFECT { #(#apply_effect_calls)* }
                }
            }

            #data_bundle_impl
        };
    }
}
