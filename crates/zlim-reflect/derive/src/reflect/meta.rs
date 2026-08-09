//! Intermediate representation (IR) for the Reflect derive macro.

use std::collections::HashSet;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::{GenericParam, Generics, ImplGenerics};
use syn::{Ident, Path, Token, Type, TypeGenerics};

use super::attrs::TypeAttrs;

// -----------------------------------------------------------------------------
// ReflectMeta
// -----------------------------------------------------------------------------

pub(crate) struct ReflectMeta<'a> {
    ident: &'a Ident,
    generics: &'a Generics,
    attrs: TypeAttrs,
    zlim_reflect: Path,
    // cannot use `BTreeSet` becausee `syn::Type` does not impl `Ord`.
    pub active_types: HashSet<&'a Type, FixedState>,
}

impl<'a> ReflectMeta<'a> {
    #[inline]
    pub fn new(
        ident: &'a Ident,
        generics: &'a Generics,
        attrs: TypeAttrs,
        zlim_reflect: Path,
    ) -> Self {
        Self {
            ident,
            generics,
            attrs,
            zlim_reflect,
            active_types: HashSet::with_hasher(FixedState),
        }
    }

    #[inline]
    pub fn ident(&self) -> &'a Ident {
        self.ident
    }

    // #[inline]
    // pub fn generics(&self) -> &'a Generics {
    //     self.generics
    // }

    #[inline]
    pub fn no_generics(&self) -> bool {
        self.generics.params.iter().next().is_none()
    }

    #[inline]
    pub fn only_lifetime_generics(&self) -> bool {
        self.generics
            .params
            .iter()
            .all(|p| matches!(p, GenericParam::Lifetime { .. }))
    }

    #[inline]
    pub fn attrs(&self) -> &TypeAttrs {
        &self.attrs
    }

    #[inline]
    pub fn zlim_reflect(&self) -> &Path {
        &self.zlim_reflect
    }

    /// Generate attributes codes
    ///
    /// If `attributes` is empty, this function will return an empty token stream.
    ///
    /// Otherwise, it will return content similar to this:
    ///
    /// ```ignore
    /// .with_attributes(
    ///     <PATH>::Attributes::builder()
    ///         $(.with( ... ))*
    ///         .finish()
    /// )
    /// ```
    #[inline]
    pub fn with_attributes_expression(&self) -> TokenStream {
        if self.attrs.custom_attrs.is_empty() {
            return TokenStream::new();
        }

        let attrs = &self.attrs.custom_attrs;

        let with_attrs = attrs.iter().map(|value| quote!(.with(#value)));
        let attributes = crate::path::attributes(&self.zlim_reflect);

        quote! {
            .with_attributes(#attributes::builder() #(#with_attrs)* .finish())
        }
    }

    /// Generate generics codes
    ///
    /// Similar to following:
    ///
    /// ```ignore
    /// .with_generics(
    ///     <PATH>::Generics::new(&[
    ///         <PATH>::GenericsInfo::Type(<PATH>::TypeParamInfo::new::<_>(..)),
    ///         <PATH>::GenericsInfo::Const(....),
    ///         ......
    ///     ])
    /// )
    /// ```
    pub fn with_generics_expression(&self) -> TokenStream {
        let zlim_reflect_path = &self.zlim_reflect;
        let generics_ = crate::path::generics(zlim_reflect_path);
        let generic_info_ = crate::path::generic_info(zlim_reflect_path);
        let type_param_info_ = crate::path::type_param_info(zlim_reflect_path);
        let const_param_info_ = crate::path::const_param_info(zlim_reflect_path);
        let const_param_ = crate::path::const_param(zlim_reflect_path);

        let generics = self
            .generics
            .params
            .iter()
            .filter_map(|param| match param {
                GenericParam::Lifetime(_) => None,
                GenericParam::Type(type_param) => {
                    let ident = &type_param.ident;
                    let name: String = ident.to_string();
                    let with_default = type_param
                        .default
                        .as_ref()
                        .map(|ty| quote!(.with_default::<#ty>()));

                    Some(quote! {
                        #generic_info_::Type(
                            #type_param_info_::new::<#ident>(#name)
                            #with_default
                        )
                    })
                }
                GenericParam::Const(const_param) => {
                    let ty: &Type = &const_param.ty;
                    // Capitalize the first letter to match enumeration variant names.
                    let type_str = quote! { #ty }.to_string();
                    let type_upper = if let Some(first) = type_str.chars().next() {
                        format!(
                            "{}{}",
                            first.to_ascii_uppercase(),
                            &type_str[first.len_utf8()..]
                        )
                    } else {
                        type_str
                    };
                    let variant = Ident::new(&type_upper, Span::call_site());

                    let ident = &const_param.ident;
                    let name = const_param.ident.to_string();
                    let with_value = quote!(.with_value(#const_param_::#variant(#ident)));

                    Some(quote! {
                        #generic_info_::Const(
                            #const_param_info_::new::<#ty>(#name)
                            #with_value
                        )
                    })
                }
            })
            .collect::<Punctuated<_, Token![,]>>();

        if generics.is_empty() {
            return TokenStream::new();
        }

        quote! {
            .with_generics(
                #generics_::new(&[ #generics ])
            )
        }
    }

    /// Generate TypeInfo Codes
    ///
    /// # Returns
    ///
    /// - 0 -  `TokenStream`: TypeInfo Codes
    /// - 1 - `bool`: is const expression
    pub fn type_info_tokens(&self) -> (TokenStream, bool) {
        let zlim_reflect_path = &self.zlim_reflect;

        let opaque_info_ = crate::path::opaque_info(zlim_reflect_path);
        let type_info_ = crate::path::type_info(zlim_reflect_path);
        let with_attributes = self.with_attributes_expression();
        let with_generics = self.with_generics_expression();

        // Can be replaced with `only_lifetime_generics` ?
        let is_const_express =
            self.no_generics() && with_attributes.is_empty() && with_generics.is_empty();

        let ident = self.ident;
        let self_token = if is_const_express {
            quote! { #ident }
        } else {
            quote! { Self }
        };

        (
            quote! {
                #type_info_::Opaque(
                    #opaque_info_::new::<#self_token>()
                        #with_attributes
                        #with_generics
                )
            },
            is_const_express,
        )
    }

    /// Add generic constraints
    ///
    /// 1. All generic parameters must implement `TypePath`
    /// 2. All field types that contain generic parameters must implement
    ///    `TypeDatabase`, unless marked with `ignore`.
    pub fn split_generics(&self) -> (ImplGenerics<'_>, TypeGenerics<'_>, TokenStream) {
        let type_path_ = crate::path::type_path_trait(&self.zlim_reflect);
        let generics = self.generics;

        let mut generic_where_clause = TokenStream::new();

        if generics.type_params().next().is_some() {
            generic_where_clause
                .extend(quote! { Self: ::core::marker::Send + ::core::marker::Sync + 'static, });
        } else if generics.lifetimes().next().is_some() {
            generic_where_clause.extend(quote! { Self: 'static, });
        }

        let (impl_gen, ty_gen, where_clause) = generics.split_for_impl();
        if let Some(where_clause) = where_clause {
            let predicates = where_clause.predicates.iter();
            generic_where_clause.extend(quote! { #(#predicates,)* });
        }

        let mut predicates: Punctuated<TokenStream, Token![,]> = Punctuated::new();

        // TypePath Predicates
        let p1 = self.generics.type_params().map(move |param| {
            let ident = &param.ident;
            quote!(#ident : #type_path_)
        });
        predicates.extend(p1);

        // TypeDatabase Predicates
        let p2 = self.field_type_predicates();
        predicates.extend(p2);

        generic_where_clause.extend(quote! { #predicates });

        let where_clause = if generic_where_clause.is_empty() {
            TokenStream::new()
        } else {
            let mut buffer = quote! { where };
            buffer.extend(generic_where_clause);
            buffer
        };

        (impl_gen, ty_gen, where_clause)
    }

    fn field_type_predicates(&self) -> Vec<TokenStream> {
        struct IdentFinder<'a> {
            idents: &'a [&'a Ident],
            found: bool,
        }

        impl<'a> Visit<'a> for IdentFinder<'a> {
            fn visit_ident(&mut self, ident: &'a Ident) {
                if !self.found {
                    self.found = self.idents.contains(&ident);
                }
            }
            fn visit_type(&mut self, ty: &'a Type) {
                if !self.found {
                    syn::visit::visit_type(self, ty);
                }
            }
        }

        fn contains_any_idents(ty: &Type, idents: &[&Ident]) -> bool {
            let mut finder = IdentFinder {
                idents,
                found: false,
            };
            finder.visit_type(ty);
            finder.found
        }

        if self.active_types.is_empty() {
            return Vec::new();
        }

        let type_param_idents: Vec<&Ident> = self
            .generics
            .type_params()
            .map(|type_param| &type_param.ident)
            .collect::<Vec<&Ident>>();

        if type_param_idents.is_empty() {
            return Vec::new();
        }

        let type_database_trait = crate::path::type_database_trait(&self.zlim_reflect);

        self.active_types
            .iter()
            .filter(|&&ty| contains_any_idents(ty, &type_param_idents))
            .map(|&ty| quote! { #ty: #type_database_trait })
            .collect()
    }
}

// -----------------------------------------------------------------------------
// FixedState
// -----------------------------------------------------------------------------

/// A simple fixed hash state, used to ensure that `where` expression generated
/// multiple times is consistent by the same `ReflectMeta`. (multiple compilations)
#[derive(Copy, Clone, Default)]
pub(crate) struct FixedState;

impl core::hash::BuildHasher for FixedState {
    type Hasher = FixedHasher;

    #[inline]
    fn build_hasher(&self) -> Self::Hasher {
        FixedHasher(0)
    }
}

#[repr(transparent)]
pub(crate) struct FixedHasher(u64);

impl core::hash::Hasher for FixedHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x100000001b3);
            self.0 ^= self.0 >> 31;
            self.0 = self.0.wrapping_mul(0xc6a4a7935bd1e995);
        }
    }
}

// -----------------------------------------------------------------------------
