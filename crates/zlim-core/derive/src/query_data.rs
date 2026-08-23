//! `#[derive(QueryData)]` — composes a `QueryData` implementation from a
//! struct whose fields are themselves `QueryData` entries.

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::punctuated::Punctuated;
use syn::visit_mut::VisitMut;
use syn::{Data, DeriveInput, Fields, GenericParam};

// -----------------------------------------------------------------------------
// Attributes

struct QueryDataAttrs {
    readonly: bool,
    /// When set, the derive also implements `QuerySlice` for the struct,
    /// using the named type as the slice-item companion.
    query_slice: Option<syn::Ident>,
}

fn parse_query_data_attrs(attrs: &[syn::Attribute]) -> syn::Result<QueryDataAttrs> {
    let mut out = QueryDataAttrs {
        readonly: false,
        query_slice: None,
    };

    for attr in attrs {
        if attr.path().is_ident("query_data") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("readonly") {
                    out.readonly = true;
                    Ok(())
                } else {
                    Err(meta.error("unsupported query_data option; expected `readonly`."))
                }
            })?;
        } else if attr.path().is_ident("query_slice") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("type") {
                    let value: syn::Ident = meta.value()?.parse()?;
                    out.query_slice = Some(value);
                    Ok(())
                } else {
                    Err(meta.error("unsupported query_slice option; expected `type = Name`."))
                }
            })?;
        }
    }

    Ok(out)
}

// -----------------------------------------------------------------------------
// Validation

fn validate_lifetimes(generics: &syn::Generics) -> syn::Result<()> {
    let lifetimes_len = generics.lifetimes().count();
    if lifetimes_len > 1 {
        return Err(syn::Error::new_spanned(
            generics,
            "`QueryData` only accepts a single lifetime named `'w` (or without any lifetime param).",
        ));
    }

    if lifetimes_len == 1 && !generics.lifetimes().any(|lt| lt.lifetime.ident == "w") {
        return Err(syn::Error::new_spanned(
            &generics.params,
            "`QueryData` accepts at most one lifetime parameter, and it must be `'w`.",
        ));
    }

    Ok(())
}

fn validate_no_mut_reference(data: &syn::DataStruct) -> syn::Result<()> {
    for field in &data.fields {
        let ty = &field.ty;
        if let syn::Type::Reference(reference) = ty
            && reference.mutability.is_some()
        {
            return Err(syn::Error::new_spanned(
                ty,
                "`&mut T` is not supported in `#[derive(QueryData)]`; use `Mut<T>` instead.",
            ));
        }
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// Type helpers

fn build_item_ty(type_ident: &syn::Ident, generics: &syn::Generics) -> TokenStream {
    let mut item_generics = generics.clone();

    for param in &mut item_generics.params {
        if let GenericParam::Lifetime(lifetime_param) = param {
            lifetime_param.lifetime = syn::Lifetime::new("'world", Span::call_site());
        }
    }

    let (_, item_ty_g, _) = item_generics.split_for_impl();
    quote! { #type_ident #item_ty_g }
}

fn map_static_lifetimes(ty: &syn::Type) -> syn::Type {
    struct LifetimeToStatic;

    impl VisitMut for LifetimeToStatic {
        fn visit_lifetime_mut(&mut self, lifetime: &mut syn::Lifetime) {
            if lifetime.ident == "w" {
                *lifetime = syn::Lifetime::new("'static", Span::call_site());
            }
        }
    }

    let mut out = ty.clone();
    LifetimeToStatic.visit_type_mut(&mut out);

    out
}

// -----------------------------------------------------------------------------
// QuerySlice companion (`#[query_slice(type = Name)]`)
//
// Generates a slice-item companion struct whose fields are the per-field
// `QuerySlice::SliceItem` types (e.g. `&'w [T]` for a `&'w T` field), plus a
// `QuerySlice` impl on the derived struct so `Query::iter_slice()` works:
//
// ```ignore
// #[derive(QueryData)]
// #[query_data(readonly)]
// #[query_slice(type = FooSlice)]
// struct Foo<'w> { a: &'w A, b: &'w B }
// // generates `struct FooSlice<'w> { a: &'w [A], b: &'w [B], ... }`
// ```
//
// For mutable structs a read-only slice companion `{Name}ReadOnly` is also
// generated (fields use the read-only `SliceItem` forms, e.g. `SliceRef`).

fn generate_query_slice(
    slice_ident: &syn::Ident,
    readonly: bool,
    type_ident: &syn::Ident,
    vis: &syn::Visibility,
    generics: &syn::Generics,
    data: &syn::DataStruct,
    static_field_types: &[syn::Type],
    query_data_: &TokenStream,
    readonly_query_data_: &TokenStream,
    query_slice_: &TokenStream,
    world_: &TokenStream,
    world_cell_: &TokenStream,
    tick_: &TokenStream,
    component_access_: &TokenStream,
    filter_param_builder_: &TokenStream,
    table_: &TokenStream,
    entity_id_: &TokenStream,
    table_row_: &TokenStream,
) -> syn::Result<(TokenStream, TokenStream)> {
    let has_lifetime = generics.lifetimes().count() > 0;
    let has_fields = !data.fields.is_empty();
    if !has_lifetime || !has_fields {
        return Err(syn::Error::new_spanned(
            slice_ident,
            "`query_slice(type = ...)` requires a struct with a `'w` lifetime and at least one field.",
        ));
    }

    let type_param_idents: Vec<&syn::Ident> = generics
        .type_params()
        .map(|type_param| &type_param.ident)
        .collect();
    let generics_params = &generics.params;
    let (impl_g, ty_g, where_g) = generics.split_for_impl();
    let idx = (0..static_field_types.len())
        .map(syn::Index::from)
        .collect::<Vec<_>>();

    let names: Vec<&syn::Ident> = match &data.fields {
        Fields::Named(fields) => fields
            .named
            .iter()
            .map(|f| f.ident.as_ref().expect("named field"))
            .collect(),
        Fields::Unnamed(_) => Vec::new(),
        Fields::Unit => unreachable!("checked above"),
    };

    // Per-field slice item types, e.g. `&'w [T]` for `&'w T`.
    let slice_field_tys: Vec<TokenStream> = static_field_types
        .iter()
        .map(|sft| quote! { <#sft as #query_slice_>::SliceItem<'w> })
        .collect();

    // Per-field read-only slice item types, e.g. `SliceRef<'w, T>` for `Mut<'w, T>`.
    let ro_slice_field_tys: Vec<TokenStream> = static_field_types
        .iter()
        .map(|sft| {
            quote! { <<#sft as #query_data_>::ReadOnly as #query_slice_>::SliceItem<'w> }
        })
        .collect();

    let readonly_slice_ident =
        syn::Ident::new(&format!("{}ReadOnly", slice_ident), slice_ident.span());

    // Type paths with `'w` replaced by `'static`.
    let slice_static_ty = quote! { #slice_ident<'static #(, #type_param_idents)*> };
    let ro_slice_static_ty = quote! { #readonly_slice_ident<'static #(, #type_param_idents)*> };

    // `Self` item / readonly types of the slice companion itself.
    let (slice_readonly, slice_readonly_slice) = if readonly {
        (quote! { Self }, quote! { Self })
    } else {
        (ro_slice_static_ty.clone(), ro_slice_static_ty.clone())
    };

    // Struct definitions (module level).
    let slice_struct_def = struct_def(
        vis,
        slice_ident,
        generics_params,
        where_g,
        &slice_field_tys,
        &names,
        &data.fields,
        "QuerySlice companion",
    );
    let ro_struct_def = if readonly {
        quote! {}
    } else {
        struct_def(
            vis,
            &readonly_slice_ident,
            generics_params,
            where_g,
            &ro_slice_field_tys,
            &names,
            &data.fields,
            "read-only QuerySlice companion",
        )
    };

    // Per-entity fetch is a stub: slice companions are table-level views that
    // are only produced through `fetch_slice`.
    let fetch_stub = quote! {
        unsafe fn fetch<'__w>(
            _state: &Self::State,
            _cache: &mut Self::Cache<'__w>,
            _entity: #entity_id_,
            _table_row: #table_row_,
        ) -> ::core::option::Option<Self::Item<'__w>> {
            // The slice companion is a table-level view produced by
            // `fetch_slice`; per-entity fetch is unsupported and always
            // yields `None`.
            ::core::option::Option::None
        }
    };

    // fetch_slice initializers for the slice companions and the derived impl:
    // the slice companion delegates through the field types themselves, the
    // read-only companion through each field's read-only form.
    let slice_delegate_tys: Vec<TokenStream> = static_field_types
        .iter()
        .map(|sft| quote! { #sft })
        .collect();
    let ro_slice_delegate_tys: Vec<TokenStream> = static_field_types
        .iter()
        .map(|sft| quote! { <#sft as #query_data_>::ReadOnly })
        .collect();

    let slice_fetch_init = slice_init(
        slice_ident,
        &names,
        &data.fields,
        &slice_delegate_tys,
        &idx,
        query_slice_,
    );
    let ro_slice_fetch_init = slice_init(
        &readonly_slice_ident,
        &names,
        &data.fields,
        &ro_slice_delegate_tys,
        &idx,
        query_slice_,
    );

    // The slice companion's QueryData impl (delegates to the field types).
    let slice_query_data_impl = quote! {
        #[automatically_derived]
        #[expect(unsafe_code, reason = "QueryData implementation is unsafe")]
        unsafe impl #impl_g #query_data_ for #slice_ident #ty_g #where_g {
            type ReadOnly = #slice_readonly;
            type State = ( #( <#static_field_types as #query_data_>::State, )* );
            type Cache<'world> = ( #( <#static_field_types as #query_data_>::Cache<'world>, )* );
            type Item<'world> = #slice_ident<'world #(, #type_param_idents)*>;

            fn build_state(world: &#world_) -> Self::State {
                ( #( <#static_field_types as #query_data_>::build_state(world), )* )
            }

            unsafe fn build_cache<'__w>(
                state: &Self::State,
                world: #world_cell_<'__w>,
                last_run: #tick_,
                this_run: #tick_,
            ) -> Self::Cache<'__w> {
                unsafe {
                    ( #( <#static_field_types as #query_data_>::build_cache(&state.#idx, world, last_run, this_run), )* )
                }
            }

            fn register_filter(state: &Self::State, out: &mut ::std::vec::Vec<#filter_param_builder_>) {
                #( <#static_field_types as #query_data_>::register_filter(&state.#idx, out); )*
            }

            fn register_access(state: &Self::State, out: &mut #component_access_) -> bool {
                let mut all_ok = true;
                #(
                    all_ok &= <#static_field_types as #query_data_>::register_access(&state.#idx, out);
                )*
                all_ok
            }

            unsafe fn update_table<'__w>(
                state: &Self::State,
                cache: &mut Self::Cache<'__w>,
                table: &'__w mut #table_,
            ) {
                unsafe {
                    let ptr = table as *mut #table_;
                    #( <#static_field_types as #query_data_>::update_table(&state.#idx, &mut cache.#idx, &mut *ptr); )*
                }
            }

            #fetch_stub
        }
    };

    // The slice companion's QuerySlice impl.
    let slice_query_slice_impl = quote! {
        #[automatically_derived]
        #[expect(unsafe_code, reason = "QuerySlice implementation is unsafe")]
        unsafe impl #impl_g #query_slice_ for #slice_ident #ty_g #where_g {
            type SliceItem<'world> = #slice_ident<'world #(, #type_param_idents)*>;
            type ReadOnlySlice = #slice_readonly_slice;

            unsafe fn fetch_slice<'__w>(
                state: &Self::State,
                cache: &mut Self::Cache<'__w>,
                entities: &'__w [#entity_id_],
            ) -> ::core::option::Option<Self::SliceItem<'__w>> {
                ::core::option::Option::Some(#slice_fetch_init)
            }
        }
    };

    // The slice companion's ReadOnlyQueryData impl (readonly case only).
    let slice_readonly_impl = if readonly {
        quote! {
            #[expect(unsafe_code, reason = "ReadOnlyQueryData implementation is unsafe")]
            unsafe impl #impl_g #readonly_query_data_ for #slice_ident #ty_g #where_g {}
        }
    } else {
        quote! {}
    };

    // The read-only slice companion's impls (mutable case only): its QueryData
    // delegates through each field's read-only form.
    let ro_delegate_tys: Vec<TokenStream> = static_field_types
        .iter()
        .map(|sft| quote! { <#sft as #query_data_>::ReadOnly })
        .collect();

    let ro_slice_impls = if readonly {
        quote! {}
    } else {
        quote! {
            #[expect(unsafe_code, reason = "ReadOnlyQueryData implementation is unsafe")]
            unsafe impl #impl_g #readonly_query_data_ for #readonly_slice_ident #ty_g #where_g {}

            #[automatically_derived]
            #[expect(unsafe_code, reason = "QueryData implementation is unsafe")]
            unsafe impl #impl_g #query_data_ for #readonly_slice_ident #ty_g #where_g {
                type ReadOnly = Self;
                type State = ( #( <#static_field_types as #query_data_>::State, )* );
                type Cache<'world> = ( #( <<#static_field_types as #query_data_>::ReadOnly as #query_data_>::Cache<'world>, )* );
                type Item<'world> = #readonly_slice_ident<'world #(, #type_param_idents)*>;

                fn build_state(world: &#world_) -> Self::State {
                    ( #( <#ro_delegate_tys as #query_data_>::build_state(world), )* )
                }

                unsafe fn build_cache<'__w>(
                    state: &Self::State,
                    world: #world_cell_<'__w>,
                    last_run: #tick_,
                    this_run: #tick_,
                ) -> Self::Cache<'__w> {
                    unsafe {
                        ( #( <#ro_delegate_tys as #query_data_>::build_cache(&state.#idx, world, last_run, this_run), )* )
                    }
                }

                fn register_filter(state: &Self::State, out: &mut ::std::vec::Vec<#filter_param_builder_>) {
                    #( <#ro_delegate_tys as #query_data_>::register_filter(&state.#idx, out); )*
                }

                fn register_access(state: &Self::State, out: &mut #component_access_) -> bool {
                    let mut all_ok = true;
                    #(
                        all_ok &= <#ro_delegate_tys as #query_data_>::register_access(&state.#idx, out);
                    )*
                    all_ok
                }

                unsafe fn update_table<'__w>(
                    state: &Self::State,
                    cache: &mut Self::Cache<'__w>,
                    table: &'__w mut #table_,
                ) {
                    unsafe {
                        let ptr = table as *mut #table_;
                        #( <#ro_delegate_tys as #query_data_>::update_table(&state.#idx, &mut cache.#idx, &mut *ptr); )*
                    }
                }

                #fetch_stub
            }

            #[automatically_derived]
            #[expect(unsafe_code, reason = "QuerySlice implementation is unsafe")]
            unsafe impl #impl_g #query_slice_ for #readonly_slice_ident #ty_g #where_g {
                type SliceItem<'world> = #readonly_slice_ident<'world #(, #type_param_idents)*>;
                type ReadOnlySlice = Self;

                unsafe fn fetch_slice<'__w>(
                    state: &Self::State,
                    cache: &mut Self::Cache<'__w>,
                    entities: &'__w [#entity_id_],
                ) -> ::core::option::Option<Self::SliceItem<'__w>> {
                    ::core::option::Option::Some(#ro_slice_fetch_init)
                }
            }
        }
    };

    // The derived struct's QuerySlice impl — `SliceItem` is the generated
    // slice companion, `ReadOnlySlice` its read-only counterpart (or the
    // companion itself for `#[query_data(readonly)]` structs).
    let derived_readonly_slice = if readonly {
        slice_static_ty
    } else {
        ro_slice_static_ty
    };
    let derived_slice_impl = quote! {
        #[automatically_derived]
        #[expect(unsafe_code, reason = "QuerySlice implementation is unsafe")]
        unsafe impl #impl_g #query_slice_ for #type_ident #ty_g #where_g {
            type SliceItem<'world> = #slice_ident<'world #(, #type_param_idents)*>;
            type ReadOnlySlice = #derived_readonly_slice;

            unsafe fn fetch_slice<'__w>(
                state: &Self::State,
                cache: &mut Self::Cache<'__w>,
                entities: &'__w [#entity_id_],
            ) -> ::core::option::Option<Self::SliceItem<'__w>> {
                ::core::option::Option::Some(#slice_fetch_init)
            }
        }
    };

    let struct_defs = quote! {
        #slice_struct_def
        #ro_struct_def
    };
    let impls = quote! {
        #slice_readonly_impl
        #slice_query_data_impl
        #slice_query_slice_impl
        #ro_slice_impls
        #derived_slice_impl
    };

    Ok((struct_defs, impls))
}

/// Builds a struct definition for a named or tuple companion struct.
fn struct_def(
    vis: &syn::Visibility,
    ident: &syn::Ident,
    generics_params: &Punctuated<GenericParam, syn::Token![,]>,
    where_g: Option<&syn::WhereClause>,
    field_tys: &[TokenStream],
    names: &[&syn::Ident],
    fields: &Fields,
    doc: &str,
) -> TokenStream {
    match fields {
        Fields::Named(_) => {
            let names = names.to_vec();
            quote! {
                #[doc(hidden)]
                #[doc = #doc]
                #vis struct #ident <#generics_params> #where_g {
                    #( pub #names: #field_tys, )*
                    #[doc(hidden)]
                    pub __phantom: ::core::marker::PhantomData<&'w ()>,
                }
            }
        }
        Fields::Unnamed(_) => {
            quote! {
                #[doc(hidden)]
                #[doc = #doc]
                #vis struct #ident <#generics_params> #where_g (
                    #( pub #field_tys, )*
                    #[doc(hidden)]
                    pub ::core::marker::PhantomData<&'w ()>,
                );
            }
        }
        Fields::Unit => unreachable!(),
    }
}

/// Builds a struct initializer for a named or tuple companion struct.
///
/// Each field is filled by `fetch_slice` through `delegate_tys` (the field
/// type itself, or its read-only form for the read-only companion).
fn slice_init(
    ident: &syn::Ident,
    names: &[&syn::Ident],
    fields: &Fields,
    delegate_tys: &[TokenStream],
    idx: &[syn::Index],
    query_slice_: &TokenStream,
) -> TokenStream {
    match fields {
        Fields::Named(_) => {
            let names = names.to_vec();
            quote! {
                #ident {
                    #( #names: {
                        <#delegate_tys as #query_slice_>::fetch_slice(
                            &state.#idx,
                            &mut cache.#idx,
                            entities,
                        )?
                    }, )*
                    __phantom: ::core::marker::PhantomData,
                }
            }
        }
        Fields::Unnamed(_) => {
            quote! {
                #ident(
                    #( {
                        <#delegate_tys as #query_slice_>::fetch_slice(
                            &state.#idx,
                            &mut cache.#idx,
                            entities,
                        )?
                    }, )*
                    ::core::marker::PhantomData,
                )
            }
        }
        Fields::Unit => unreachable!(),
    }
}

// -----------------------------------------------------------------------------
// Expansion

pub(crate) fn expand(ast: DeriveInput) -> TokenStream {
    let zlim_core_path = crate::path::zlim_core_path();
    let query_data_ = crate::path::query_data_(&zlim_core_path);
    let readonly_query_data_ = crate::path::readonly_query_data_(&zlim_core_path);
    let query_slice_ = crate::path::query_slice_(&zlim_core_path);
    let world_ = crate::path::world_(&zlim_core_path);
    let world_cell_ = crate::path::world_cell_(&zlim_core_path);
    let tick_ = crate::path::tick_(&zlim_core_path);
    let component_access_ = crate::path::component_access_(&zlim_core_path);
    let filter_param_builder_ = crate::path::filter_param_builder_(&zlim_core_path);
    let table_ = crate::path::table_(&zlim_core_path);
    let table_row_ = crate::path::table_row_(&zlim_core_path);
    let entity_id_ = crate::path::entity_id_(&zlim_core_path);

    let Data::Struct(data) = &ast.data else {
        return syn::Error::new_spanned(
            ast,
            "`QueryData` can only be derived for structs (named, tuple, or unit).",
        )
        .into_compile_error();
    };

    let attrs = match parse_query_data_attrs(&ast.attrs) {
        Ok(attrs) => attrs,
        Err(err) => return err.into_compile_error(),
    };

    if let Err(err) = validate_lifetimes(&ast.generics) {
        return err.into_compile_error();
    }

    if let Err(err) = validate_no_mut_reference(data) {
        return err.into_compile_error();
    }

    let type_ident = ast.ident.clone();
    let vis = ast.vis.clone();

    let field_types: Vec<&syn::Type> = data.fields.iter().map(|f| &f.ty).collect();
    let static_field_types: Vec<syn::Type> = field_types
        .iter()
        .map(|ty| map_static_lifetimes(ty))
        .collect();
    let idx = (0..field_types.len())
        .map(syn::Index::from)
        .collect::<Vec<_>>();

    // Type parameters of the struct, needed to build the companion's type
    // path (e.g. `FooReadOnly<'static, T>`).
    let type_param_idents: Vec<syn::Ident> = ast
        .generics
        .type_params()
        .map(|type_param| type_param.ident.clone())
        .collect();

    // NOTE: we deliberately do not add per-field `QueryData` where-bounds to
    // the generated impls.  A where-bound like `&'static T: QueryData` would
    // shadow the associated-type definitions of the real impl (rustc#152409),
    // preventing `<&'static T as QueryData>::Item<'w>` from normalizing to
    // `&T` and breaking generic fields.  Field types are already well-formed
    // on the struct itself, so their bounds (e.g. `T: Component`) are carried
    // by the struct's generics, and the built-in query-data impls apply.

    let item_ty = build_item_ty(&type_ident, &ast.generics);
    let (impl_g, ty_g, where_g) = ast.generics.split_for_impl();

    let has_lifetime = ast.generics.lifetimes().count() > 0;
    let has_fields = !data.fields.is_empty();

    // A companion ReadOnly struct is generated for non-readonly structs that
    // carry a `'w` lifetime (i.e. may hold mutable borrows) and have fields.
    let generate_readonly_struct = !attrs.readonly && has_lifetime && has_fields;

    // The `ReadOnly` type of the original impl, plus any companion struct
    // definition and its impls.
    let (readonly_type, readonly_struct_def, readonly_impls_in_const) = if generate_readonly_struct
    {
        let readonly_ident = syn::Ident::new(&format!("{}ReadOnly", type_ident), type_ident.span());

        // Field types of the companion: the read-only item type of each
        // field, e.g. `Ref<'w, T>` for a `Mut<'w, T>` field.
        let readonly_field_tys: Vec<TokenStream> = static_field_types
            .iter()
            .map(|sft| {
                quote! {
                    <<#sft as #query_data_>::ReadOnly as #query_data_>::Item<'w>
                }
            })
            .collect();

        // The companion's own item type: `FooReadOnly<'world, T>`.
        let readonly_item_ty = build_item_ty(&readonly_ident, &ast.generics);

        // Type path used as `type ReadOnly = ...`: `FooReadOnly<'static, T>`.
        // `'w` is replaced by `'static`; other params are kept as-is.
        let type_param_idents_refs: Vec<&syn::Ident> = type_param_idents.iter().collect();
        let readonly_static_ty = quote! {
            #readonly_ident<'static #(, #type_param_idents_refs)*>
        };

        // Companion struct definition, emitted at module level so its fields
        // are publicly accessible (e.g. `for item in ro_query { item.field }`).
        // The definition keeps the *full* generic parameter list (including
        // bounds); only the impls below use the split `impl_g`/`ty_g` forms.
        let generics_params = &ast.generics.params;
        let struct_def = match &data.fields {
            Fields::Named(fields) => {
                let names: Vec<_> = fields
                    .named
                    .iter()
                    .map(|f| f.ident.as_ref().expect("named field"))
                    .collect();
                quote! {
                    #[doc(hidden)]
                    #vis struct #readonly_ident <#generics_params> #where_g {
                        #( pub #names: #readonly_field_tys, )*
                        #[doc(hidden)]
                        pub __phantom: ::core::marker::PhantomData<&'w ()>,
                    }
                }
            }
            Fields::Unnamed(_) => {
                quote! {
                    #[doc(hidden)]
                    #vis struct #readonly_ident <#generics_params> #where_g (
                        #( pub #readonly_field_tys, )*
                        #[doc(hidden)]
                        pub ::core::marker::PhantomData<&'w ()>,
                    );
                }
            }
            Fields::Unit => {
                unreachable!("unit structs are handled by generate_readonly_struct=false")
            }
        };

        // The read-only form of each field, used for delegation.
        let readonly_delegate_tys: Vec<TokenStream> = static_field_types
            .iter()
            .map(|sft| quote! { <#sft as #query_data_>::ReadOnly })
            .collect();

        let readonly_fetch_init = match &data.fields {
            Fields::Named(fields) => {
                let names: Vec<_> = fields
                    .named
                    .iter()
                    .map(|f| f.ident.as_ref().expect("named field"))
                    .collect();
                quote! {
                    #readonly_ident {
                        #( #names: {
                            <#readonly_delegate_tys as #query_data_>::fetch(
                                &state.#idx,
                                &mut cache.#idx,
                                entity,
                                table_row,
                            )?
                        }, )*
                        __phantom: ::core::marker::PhantomData,
                    }
                }
            }
            Fields::Unnamed(_) => {
                quote! {
                    #readonly_ident(
                        #( {
                            <#readonly_delegate_tys as #query_data_>::fetch(
                                &state.#idx,
                                &mut cache.#idx,
                                entity,
                                table_row,
                            )?
                        }, )*
                        ::core::marker::PhantomData,
                    )
                }
            }
            Fields::Unit => unreachable!(),
        };

        // Impls for the companion struct, emitted inside `const _`.
        let impls = quote! {
            #[expect(unsafe_code, reason = "ReadOnlyQueryData implementation is unsafe")]
            unsafe impl #impl_g #readonly_query_data_ for #readonly_ident #ty_g #where_g {}

            #[expect(unsafe_code, reason = "QueryData implementation is unsafe")]
            unsafe impl #impl_g #query_data_ for #readonly_ident #ty_g #where_g {
                type ReadOnly = Self;
                // Use the original field States (not the ReadOnly::State
                // aliases) so Rust can trivially verify
                // `State = <Original as QueryData>::State`.
                type State = ( #( <#static_field_types as #query_data_>::State, )* );
                type Cache<'world> = ( #( <<#static_field_types as #query_data_>::ReadOnly as #query_data_>::Cache<'world>, )* );
                type Item<'world> = #readonly_item_ty;

                fn build_state(world: &#world_) -> Self::State {
                    ( #( <#readonly_delegate_tys as #query_data_>::build_state(world), )* )
                }

                unsafe fn build_cache<'__w>(
                    state: &Self::State,
                    world: #world_cell_<'__w>,
                    last_run: #tick_,
                    this_run: #tick_,
                ) -> Self::Cache<'__w> {
                    unsafe {
                        ( #( <#readonly_delegate_tys as #query_data_>::build_cache(&state.#idx, world, last_run, this_run), )* )
                    }
                }

                fn register_filter(state: &Self::State, out: &mut ::std::vec::Vec<#filter_param_builder_>) {
                    #( <#readonly_delegate_tys as #query_data_>::register_filter(&state.#idx, out); )*
                }

                fn register_access(state: &Self::State, out: &mut #component_access_) -> bool {
                    let mut all_ok = true;
                    #(
                        all_ok &= <#readonly_delegate_tys as #query_data_>::register_access(&state.#idx, out);
                    )*
                    all_ok
                }

                unsafe fn update_table<'__w>(
                    state: &Self::State,
                    cache: &mut Self::Cache<'__w>,
                    table: &'__w mut #table_,
                ) {
                    unsafe {
                        let ptr = table as *mut #table_;
                        #( <#readonly_delegate_tys as #query_data_>::update_table(&state.#idx, &mut cache.#idx, &mut *ptr); )*
                    }
                }

                unsafe fn fetch<'__w>(
                    state: &Self::State,
                    cache: &mut Self::Cache<'__w>,
                    entity: #entity_id_,
                    table_row: #table_row_,
                ) -> ::core::option::Option<Self::Item<'__w>> {
                    ::core::option::Option::Some(#readonly_fetch_init)
                }
            }
        };

        (quote! { #readonly_static_ty }, struct_def, impls)
    } else {
        // Readonly / unit / no-'w: `ReadOnly = Self`; implement
        // `ReadOnlyQueryData` directly.
        let ro_impl = quote! {
            #[expect(unsafe_code, reason = "ReadOnlyQueryData implementation is unsafe")]
            unsafe impl #impl_g #readonly_query_data_ for #type_ident #ty_g #where_g {}
        };

        (quote! { Self }, quote! {}, ro_impl)
    };

    let fetch_init = match &data.fields {
        Fields::Named(fields) => {
            let names = fields
                .named
                .iter()
                .map(|f| f.ident.as_ref().expect("named field"));
            quote! {
                #type_ident {
                    #( #names: {
                        <#static_field_types as #query_data_>::fetch(
                            &state.#idx,
                            &mut cache.#idx,
                            entity,
                            table_row,
                        )?
                    }, )*
                }
            }
        }
        Fields::Unnamed(_) => {
            quote! {
                #type_ident(
                    #( {
                        <#static_field_types as #query_data_>::fetch(
                            &state.#idx,
                            &mut cache.#idx,
                            entity,
                            table_row,
                        )?
                    }, )*
                )
            }
        }
        Fields::Unit => quote! { #type_ident },
    };

    // For fieldless (unit) structs, the per-field delegating bodies would
    // collapse to a bare `( )` unit expression, which clippy flags; emit
    // empty bodies instead.
    let build_state_body = if has_fields {
        quote! { ( #( <#static_field_types as #query_data_>::build_state(world), )* ) }
    } else {
        quote! {}
    };

    let build_cache_body = if has_fields {
        quote! {
            unsafe {
                ( #( <#static_field_types as #query_data_>::build_cache(&state.#idx, world, last_run, this_run), )* )
            }
        }
    } else {
        quote! { unsafe {} }
    };

    let update_table_body = if has_fields {
        quote! {
            unsafe {
                let ptr = table as *mut #table_;
                #( <#static_field_types as #query_data_>::update_table(&state.#idx, &mut cache.#idx, &mut *ptr); )*
            }
        }
    } else {
        quote! {}
    };

    // `#[query_data(query_slice(type = Name))]`: generate the slice-item
    // companion(s) and the derived struct's `QuerySlice` impl.
    let (slice_struct_defs, slice_impls) = match &attrs.query_slice {
        Some(slice_ident) => match generate_query_slice(
            slice_ident,
            attrs.readonly,
            &type_ident,
            &vis,
            &ast.generics,
            data,
            &static_field_types,
            &query_data_,
            &readonly_query_data_,
            &query_slice_,
            &world_,
            &world_cell_,
            &tick_,
            &component_access_,
            &filter_param_builder_,
            &table_,
            &entity_id_,
            &table_row_,
        ) {
            Ok((struct_defs, impls)) => (struct_defs, impls),
            Err(err) => return err.into_compile_error(),
        },
        None => (quote! {}, quote! {}),
    };

    quote! {
        #readonly_struct_def
        #slice_struct_defs

        const _: () = {
            #readonly_impls_in_const
            #slice_impls

            #[automatically_derived]
            #[expect(unsafe_code, reason = "QueryData implementation is unsafe")]
            unsafe impl #impl_g #query_data_ for #type_ident #ty_g #where_g {
                type ReadOnly = #readonly_type;
                type State = ( #( <#static_field_types as #query_data_>::State, )* );
                type Cache<'world> = ( #( <#static_field_types as #query_data_>::Cache<'world>, )* );
                type Item<'world> = #item_ty;

                fn build_state(world: &#world_) -> Self::State {
                    #build_state_body
                }

                unsafe fn build_cache<'__w>(
                    state: &Self::State,
                    world: #world_cell_<'__w>,
                    last_run: #tick_,
                    this_run: #tick_,
                ) -> Self::Cache<'__w> {
                    #build_cache_body
                }

                fn register_filter(state: &Self::State, out: &mut ::std::vec::Vec<#filter_param_builder_>) {
                    #( <#static_field_types as #query_data_>::register_filter(&state.#idx, out); )*
                }

                fn register_access(state: &Self::State, out: &mut #component_access_) -> bool {
                    let mut all_ok = true;
                    #(
                        all_ok &= <#static_field_types as #query_data_>::register_access(&state.#idx, out);
                    )*
                    all_ok
                }

                unsafe fn update_table<'__w>(
                    state: &Self::State,
                    cache: &mut Self::Cache<'__w>,
                    table: &'__w mut #table_,
                ) {
                    #update_table_body
                }

                unsafe fn fetch<'__w>(
                    state: &Self::State,
                    cache: &mut Self::Cache<'__w>,
                    entity: #entity_id_,
                    table_row: #table_row_,
                ) -> ::core::option::Option<Self::Item<'__w>> {
                    ::core::option::Option::Some(#fetch_init)
                }
            }
        };
    }
}
