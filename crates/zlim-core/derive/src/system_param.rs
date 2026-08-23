use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::visit_mut::VisitMut;
use syn::{Data, DeriveInput, Fields, GenericParam};

fn validate_lifetimes(generics: &syn::Generics) -> syn::Result<()> {
    if generics.lifetimes().count() != 2 {
        return Err(syn::Error::new_spanned(
            &generics.params,
            "`SystemParam` requires exactly two lifetime parameters: `'w` and `'s`.",
        ));
    }

    let has_w = generics.lifetimes().any(|lt| lt.lifetime.ident == "w");
    let has_s = generics.lifetimes().any(|lt| lt.lifetime.ident == "s");

    if !has_w || !has_s {
        return Err(syn::Error::new_spanned(
            &generics.params,
            "`SystemParam` lifetime parameters must be exactly `'w` and `'s`.",
        ));
    }

    Ok(())
}

fn build_item_ty(type_ident: &syn::Ident, generics: &syn::Generics) -> TokenStream {
    let mut item_generics = generics.clone();

    for param in &mut item_generics.params {
        if let GenericParam::Lifetime(lifetime_param) = param {
            if lifetime_param.lifetime.ident == "w" {
                lifetime_param.lifetime = syn::Lifetime::new("'world", Span::call_site());
            } else if lifetime_param.lifetime.ident == "s" {
                lifetime_param.lifetime = syn::Lifetime::new("'state", Span::call_site());
            }
        }
    }

    let (_, item_ty_g, _) = item_generics.split_for_impl();
    quote! { #type_ident #item_ty_g }
}

fn map_static_lifetimes(ty: &syn::Type) -> syn::Type {
    struct LifetimeToStatic;

    impl VisitMut for LifetimeToStatic {
        fn visit_lifetime_mut(&mut self, lifetime: &mut syn::Lifetime) {
            if lifetime.ident == "w" || lifetime.ident == "s" {
                *lifetime = syn::Lifetime::new("'static", Span::call_site());
            }
        }
    }

    let mut out = ty.clone();
    LifetimeToStatic.visit_type_mut(&mut out);
    out
}

pub(crate) fn expand(ast: DeriveInput) -> TokenStream {
    let zlim_core_path = crate::path::zlim_core_path();
    let system_param_ = crate::path::system_param_(&zlim_core_path);
    let system_param_error_ = crate::path::system_param_error_(&zlim_core_path);
    let world_ = crate::path::world_(&zlim_core_path);
    let world_cell_ = crate::path::world_cell_(&zlim_core_path);
    let access_table_ = crate::path::access_table_(&zlim_core_path);
    let tick_ = crate::path::tick_(&zlim_core_path);
    let deferred_world_ = crate::path::deferred_world_(&zlim_core_path);

    let Data::Struct(data) = &ast.data else {
        return syn::Error::new_spanned(
            ast,
            "`SystemParam` can only be derived for structs (named, tuple).",
        )
        .into_compile_error();
    };

    if let Err(err) = validate_lifetimes(&ast.generics) {
        return err.into_compile_error();
    }

    let type_ident = ast.ident;
    let field_types: Vec<&syn::Type> = data.fields.iter().map(|f| &f.ty).collect();
    let static_field_types: Vec<syn::Type> = field_types
        .iter()
        .map(|ty| map_static_lifetimes(ty))
        .collect();
    let idx = (0..field_types.len())
        .map(syn::Index::from)
        .collect::<Vec<_>>();

    // NOTE: We deliberately do not add per-field `SystemParam` bounds to the
    // generated impl's where-clause.  A where-bound like
    // `Res<'static, T>: SystemParam` would shadow the associated-type
    // definitions of the real impl (rustc#152409), preventing
    // `<Res<'static, T> as SystemParam>::Item<'w, 's>` from normalizing to
    // `Res<'w, T>` and making generic fields impossible.  Field types are
    // already well-formed on the struct itself, so their bounds (e.g.
    // `T: Resource + Sync`) are carried by the struct's generics.

    let item_ty = build_item_ty(&type_ident, &ast.generics);
    let (impl_g, ty_g, where_g) = ast.generics.split_for_impl();

    let fetch_init = match &data.fields {
        Fields::Named(fields) => {
            let names = fields
                .named
                .iter()
                .map(|f| f.ident.as_ref().expect("named field"));
            quote! {
                #type_ident { #(
                    #names: unsafe {
                        <#static_field_types as #system_param_>::build_param(&mut state.#idx, world, last_run, this_run)?
                    },
                )* }
            }
        }
        Fields::Unnamed(_) => {
            quote! {
                #type_ident ( #(
                    unsafe {
                        <#static_field_types as #system_param_>::build_param(&mut state.#idx, world, last_run, this_run)?
                    },
                )* )
            }
        }
        Fields::Unit => quote! { #type_ident },
    };

    quote! {
        const _: () = {
            #[automatically_derived]
            #[expect(unsafe_code, reason = "SystemParam implementation is unsafe")]
            unsafe impl #impl_g #system_param_ for #type_ident #ty_g #where_g {
                type State = ( #( <#static_field_types as #system_param_>::State, )* );
                type Item<'world, 'state> = #item_ty;

                const DEFERRED: bool = false #( || <#static_field_types as #system_param_>::DEFERRED )*;
                const NON_SEND: bool = false #( || <#static_field_types as #system_param_>::NON_SEND )*;
                const EXCLUSIVE: bool = false #( || <#static_field_types as #system_param_>::EXCLUSIVE )*;

                fn init_state(world: &#world_) -> Self::State {
                    ( #( <#static_field_types as #system_param_>::init_state(world), )* )
                }

                fn register_access(
                    state: &Self::State,
                    table: &mut #access_table_,
                    mut strict: bool,
                ) -> bool {
                    let mut all_ok = true;

                    #(
                        all_ok &= <#static_field_types as #system_param_>::register_access(&state.#idx, table, strict);
                        // After a conflict occurs, relax to non-strict to
                        // avoid repeating error logs.
                        strict &= all_ok;
                    )*

                    all_ok
                }

                unsafe fn build_param<'__w, '__s>(
                    state: &'__s mut Self::State,
                    world: #world_cell_<'__w>,
                    last_run: #tick_,
                    this_run: #tick_,
                ) -> ::core::result::Result<Self::Item<'__w, '__s>, #system_param_error_> {
                    ::core::result::Result::Ok(#fetch_init)
                }

                fn queue_deferred(state: &mut Self::State, mut world: #deferred_world_) {
                    if <Self as #system_param_>::DEFERRED {
                        #( <#static_field_types as #system_param_>::queue_deferred(&mut state.#idx, world.reborrow()); )*
                    }
                }

                fn apply_deferred(state: &mut Self::State, world: &mut #world_) {
                    if <Self as #system_param_>::DEFERRED {
                        #( <#static_field_types as #system_param_>::apply_deferred(&mut state.#idx, world); )*
                    }
                }
            }
        };
    }
}
