use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{Data, DeriveInput, Fields, Ident, Index, Type, parse_quote};

use crate::utils::{contains_any_idents, field_type_constraint};

// -----------------------------------------------------------------------------
// Attributes
// -----------------------------------------------------------------------------

/// Cloner strategy for the `Component` derive.
enum Cloner {
    /// Default: `ComponentCloner::clonable::<Self>()` (requires `Clone`).
    Cloneable,
    /// `copy`: `ComponentCloner::copyable::<Self>()` (requires `Copy`).
    Copy,
    /// `cloner = path::function`: `ComponentCloner::custom(path::function)`.
    Custom(syn::ExprPath),
}

/// Parsed `#[component(...)]` type-level attributes.
struct ComponentAttrs {
    cloner: Cloner,
    map_entities: Option<syn::ExprPath>,
    on_add: Option<syn::ExprPath>,
    on_clone: Option<syn::ExprPath>,
    on_insert: Option<syn::ExprPath>,
    on_remove: Option<syn::ExprPath>,
    on_discard: Option<syn::ExprPath>,
    on_despawn: Option<syn::ExprPath>,
    reflect: bool,
    persist: bool,
    summary_tick: bool,
    required: Vec<Type>,
}

/// Parses a hook from a nested meta.
///
/// Two forms are accepted:
///
/// - `on_add = path::function` — the hook is the given function;
/// - a bare `on_add` — the hook is the associated function of the same name on
///   the type itself, i.e. `Self::on_add`.
fn parse_hook_expr(meta: &syn::meta::ParseNestedMeta) -> syn::Result<syn::ExprPath> {
    if meta.input.peek(syn::Token![=]) {
        let value = meta.value()?;
        value.parse::<syn::ExprPath>()
    } else {
        // A bare hook name: the hook lives on the type as `Self::<name>`.
        let name = meta
            .path
            .get_ident()
            .ok_or_else(|| meta.error("expected a hook name"))?;

        Ok(syn::parse_quote!(Self::#name))
    }
}

fn parse_component_attrs(attrs: &[syn::Attribute]) -> syn::Result<ComponentAttrs> {
    let mut ret = ComponentAttrs {
        cloner: Cloner::Cloneable,
        map_entities: None,
        on_add: None,
        on_clone: None,
        on_insert: None,
        on_remove: None,
        on_discard: None,
        on_despawn: None,
        reflect: false,
        persist: false,
        summary_tick: false,
        required: Vec::new(),
    };

    for attr in attrs {
        if attr.path().is_ident("require") {
            let types =
                attr.parse_args_with(Punctuated::<Type, syn::Token![,]>::parse_terminated)?;
            ret.required.extend(types);
            continue;
        }

        if !attr.path().is_ident("component") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("copy") {
                match &ret.cloner {
                    Cloner::Cloneable => ret.cloner = Cloner::Copy,
                    Cloner::Copy => {
                        return Err(meta.error("duplicate `copy`"));
                    }
                    Cloner::Custom(path) => {
                        let msg = format!("`copy` conflicts with `cloner = {path:?}`");
                        return Err(meta.error(msg));
                    }
                }
                Ok(())
            } else if meta.path.is_ident("cloner") {
                match &ret.cloner {
                    Cloner::Cloneable => {}
                    Cloner::Copy => {
                        return Err(meta.error("`cloner = …` conflicts with `copy`"));
                    }
                    Cloner::Custom(_) => {
                        return Err(meta.error("duplicate `cloner = …`"));
                    }
                }
                if meta.input.peek(syn::Token![=]) {
                    let value = meta.value()?;
                    ret.cloner = Cloner::Custom(value.parse::<syn::ExprPath>()?);
                } else {
                    return Err(meta.error("expected `cloner = path::function`"));
                }
                Ok(())
            } else if meta.path.is_ident("map_entities") {
                if meta.input.peek(syn::Token![=]) {
                    let value = meta.value()?;
                    ret.map_entities = Some(value.parse::<syn::ExprPath>()?);
                } else {
                    return Err(meta.error("expected `map_entities = path::function`"));
                }
                Ok(())
            } else if meta.path.is_ident("on_add") {
                ret.on_add = Some(parse_hook_expr(&meta)?);
                Ok(())
            } else if meta.path.is_ident("on_clone") {
                ret.on_clone = Some(parse_hook_expr(&meta)?);
                Ok(())
            } else if meta.path.is_ident("on_insert") {
                ret.on_insert = Some(parse_hook_expr(&meta)?);
                Ok(())
            } else if meta.path.is_ident("on_remove") {
                ret.on_remove = Some(parse_hook_expr(&meta)?);
                Ok(())
            } else if meta.path.is_ident("on_discard") {
                ret.on_discard = Some(parse_hook_expr(&meta)?);
                Ok(())
            } else if meta.path.is_ident("on_despawn") {
                ret.on_despawn = Some(parse_hook_expr(&meta)?);
                Ok(())
            } else if meta.path.is_ident("reflect") {
                ret.reflect = true;
                Ok(())
            } else if meta.path.is_ident("persist") {
                ret.persist = true;
                Ok(())
            } else if meta.path.is_ident("summary_tick") {
                ret.summary_tick = true;
                Ok(())
            } else if meta.path.is_ident("serialize") || meta.path.is_ident("into_template") {
                // Both named this attribute after the mechanism rather than after
                // what it decides, which `#[reflect(Serialize)]` already names.
                Err(meta.error("use #[component(persist)] instead"))
            } else {
                Err(meta.error("unsupported component attribute"))
            }
        })?;
    }

    if ret.persist && !ret.reflect {
        const MSG: &str = "`#[component(persist)]` requires `#[component(reflect)]`";
        return Err(syn::Error::new(Span::call_site(), MSG));
    }

    Ok(ret)
}

// -----------------------------------------------------------------------------
// Entity-field collection
// -----------------------------------------------------------------------------

/// A field annotated with `#[entities]`, carrying access info for
/// `map_entities` codegen.
struct EntityField<'a> {
    access: TokenStream,
    ty: &'a Type,
}

/// Walks struct fields and collects every `#[entities]` entry.
fn collect_entity_fields(data: &Data) -> Result<Vec<EntityField<'_>>, syn::Error> {
    let fields = match data {
        Data::Struct(s) => &s.fields,
        _ => return Ok(Vec::new()),
    };

    let mut result = Vec::new();

    match fields {
        Fields::Named(named) => {
            for field in &named.named {
                if !field.attrs.iter().any(|a| a.path().is_ident("entities")) {
                    continue;
                }
                let ident = field.ident.as_ref().unwrap();
                result.push(EntityField {
                    access: quote! { #ident },
                    ty: &field.ty,
                });
            }
        }
        Fields::Unnamed(unnamed) => {
            for (i, field) in unnamed.unnamed.iter().enumerate() {
                if !field.attrs.iter().any(|a| a.path().is_ident("entities")) {
                    continue;
                }
                let index = Index::from(i);
                result.push(EntityField {
                    access: quote! { #index },
                    ty: &field.ty,
                });
            }
        }
        Fields::Unit => {}
    }

    Ok(result)
}

// -----------------------------------------------------------------------------
// Expand
// -----------------------------------------------------------------------------

pub(crate) fn expand(ast: DeriveInput) -> TokenStream {
    let attrs = match parse_component_attrs(&ast.attrs) {
        Ok(a) => a,
        Err(e) => return e.into_compile_error(),
    };

    let zlim_core = crate::path::zlim_core_path();
    let component_ = crate::path::component_(&zlim_core);
    let component_cloner_ = crate::path::component_cloner_(&zlim_core);
    let component_hook_ = crate::path::component_hook_(&zlim_core);
    let map_entities_ = crate::path::map_entities_(&zlim_core);
    let entity_mapper_ = crate::path::entity_mapper_(&zlim_core);

    let type_ident = &ast.ident;
    let mut generics = ast.generics;

    // --- generic bounds ------------------------------------------------
    if generics.type_params().next().is_some() {
        let predicates = &mut generics.make_where_clause().predicates;

        predicates.push(parse_quote! {
            Self: ::core::marker::Send + ::core::marker::Sync + ::core::marker::Sized + 'static
        });

        match &attrs.cloner {
            Cloner::Cloneable => predicates.push(parse_quote! {
                Self: ::core::clone::Clone
            }),
            Cloner::Copy => predicates.push(parse_quote! {
                Self: ::core::marker::Copy
            }),
            _ => {}
        }

        if attrs.reflect {
            let type_database_ = crate::path::type_database_(&zlim_core);
            predicates.push(parse_quote! { Self: #type_database_ });
        }

        // A persisted component is also `Clone`: a scene document describes a value that can be
        // applied more than once, and the template of such a value is built by cloning it.
        if attrs.persist {
            predicates.push(parse_quote! { Self: ::core::clone::Clone });
        }
    } else if generics.lifetimes().next().is_some() {
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote! { Self: 'static });
    }

    let generic_idents: Vec<Ident> = generics.type_params().map(|p| p.ident.clone()).collect();

    // --- entity fields -------------------------------------------------
    let entity_fields = match collect_entity_fields(&ast.data) {
        Ok(f) => f,
        Err(e) => return e.into_compile_error(),
    };

    // Validate conflicts
    if !entity_fields.is_empty() && attrs.map_entities.is_some() {
        const MSG: &str = "`#[entities]` fields conflict with `map_entities = …`";
        return syn::Error::new(Span::call_site(), MSG).into_compile_error();
    }

    // MapEntities constraints for entity fields with unresolved generics
    for f in &entity_fields {
        if contains_any_idents(f.ty, &generic_idents) {
            field_type_constraint(&mut generics, f.ty, &map_entities_);
        }
    }

    // --- NO_ENTITY -----------------------------------------------------
    // Components with `#[entities]` fields reference entities and therefore
    // need remapping on clone: `NO_ENTITY = false`.  Components with a
    // custom `map_entities` function do too.  Components without either can
    // skip remapping entirely.
    let no_entity = entity_fields.is_empty() && attrs.map_entities.is_none();

    // --- SUMMARY_TICK -----------------------------------------------------

    let summary_tick = attrs.summary_tick;

    // --- cloner --------------------------------------------------------
    let cloner_tokens = match &attrs.cloner {
        Cloner::Cloneable => {
            quote! { const CLONER: #component_cloner_ = #component_cloner_::clonable::<Self>(); }
        }
        Cloner::Copy => {
            quote! { const CLONER: #component_cloner_ = #component_cloner_::copyable::<Self>(); }
        }
        Cloner::Custom(expr) => {
            quote! { const CLONER: #component_cloner_ = #component_cloner_::custom(#expr); }
        }
    };

    // --- hooks ---------------------------------------------------------
    let on_add_tokens = match &attrs.on_add {
        Some(p) => {
            quote! { const ON_ADD: ::core::option::Option<#component_hook_> = ::core::option::Option::Some(#p); }
        }
        None => TokenStream::new(),
    };
    let on_clone_tokens = match &attrs.on_clone {
        Some(p) => {
            quote! { const ON_CLONE: ::core::option::Option<#component_hook_> = ::core::option::Option::Some(#p); }
        }
        None => TokenStream::new(),
    };
    let on_insert_tokens = match &attrs.on_insert {
        Some(p) => {
            quote! { const ON_INSERT: ::core::option::Option<#component_hook_> = ::core::option::Option::Some(#p); }
        }
        None => TokenStream::new(),
    };
    let on_remove_tokens = match &attrs.on_remove {
        Some(p) => {
            quote! { const ON_REMOVE: ::core::option::Option<#component_hook_> = ::core::option::Option::Some(#p); }
        }
        None => TokenStream::new(),
    };
    let on_discard_tokens = match &attrs.on_discard {
        Some(p) => {
            quote! { const ON_DISCARD: ::core::option::Option<#component_hook_> = ::core::option::Option::Some(#p); }
        }
        None => TokenStream::new(),
    };
    let on_despawn_tokens = match &attrs.on_despawn {
        Some(p) => {
            quote! { const ON_DESPAWN: ::core::option::Option<#component_hook_> = ::core::option::Option::Some(#p); }
        }
        None => TokenStream::new(),
    };

    // --- map_entities --------------------------------------------------
    let map_entities_tokens = if let Some(path) = &attrs.map_entities {
        // `map_entities = path::fn` delegates remapping to a user function
        // with signature `fn(&mut Self, &mut M)` where `M: EntityMapper`.
        quote! {
            fn map_entities<__M_Z_: #entity_mapper_>(&mut self, mapper: &mut __M_Z_) {
                #path(self, mapper);
            }
        }
    } else if entity_fields.is_empty() {
        // No #[entities] fields — default no-op map_entities.
        TokenStream::new()
    } else {
        // Generate map_entities from #[entities] fields
        let calls = entity_fields.iter().map(|f| {
            let access = &f.access;
            quote! { #map_entities_::map_entities(&mut self.#access, mapper); }
        });
        quote! {
            fn map_entities<__M_Z_: #entity_mapper_>(&mut self, mapper: &mut __M_Z_) { #(#calls)* }
        }
    };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // --- register ------------------------------------------------------
    // `#[component(persist)]` components override the trait's default
    // registration to fill the persistence function pointers.
    let register_tokens = if attrs.persist {
        let component_db_ = crate::path::component_db_(&zlim_core);
        let register_persist_ = crate::path::component_register_persist_(&zlim_core);
        quote! {
            const REGISTER: fn() -> &'static #component_db_ = #register_persist_::<Self>;
        }
    } else if attrs.reflect {
        let component_db_ = crate::path::component_db_(&zlim_core);
        let register_reflect_ = crate::path::component_register_reflect_(&zlim_core);
        quote! {
            const REGISTER: fn() -> &'static #component_db_ = #register_reflect_::<Self>;
        }
    } else {
        TokenStream::new()
    };

    // --- required components -------------------------------------------
    let required_tokens = if attrs.required.is_empty() {
        TokenStream::new()
    } else if attrs.required.len() == 1 {
        let required_ = crate::path::required_(&zlim_core);
        let ts = &attrs.required[0];
        quote! {
            const REQUIRED: ::core::option::Option<#required_> = ::core::option::Option::Some(#required_::from::<#ts>());
        }
    } else if attrs.required.len() > 8 * 12 {
        ::core::hint::cold_path();
        const MSG: &str = "too many required components";
        return syn::Error::new(Span::call_site(), MSG).into_compile_error();
    } else if attrs.required.len() <= 12 {
        let required_ = crate::path::required_(&zlim_core);
        let ts = &attrs.required;
        quote! {
            const REQUIRED: ::core::option::Option<#required_> = ::core::option::Option::Some(#required_::from::<(#(#ts),*)>());
        }
    } else {
        let required_ = crate::path::required_(&zlim_core);
        let ts = &attrs.required;
        let tss = ts.chunks(8).map(|c| quote!( (#(#c),*) ));
        quote! {
            const REQUIRED: ::core::option::Option<#required_> = ::core::option::Option::Some(#required_::from::<(#(#tss),*)>());
        }
    };

    // --- auto-registration (non-generic types only) -------------------
    let auto_register = if generics.type_params().next().is_none() {
        let submit_ = crate::path::submit_(&zlim_core);
        quote! {
            #submit_!(
                #zlim_core::component::__internal__::__ComponentReg__::of::<#type_ident>()
                => #zlim_core::component::__internal__::__ComponentReg__
            );
        }
    } else {
        TokenStream::new()
    };

    quote! {
        const _: () = {
            #[automatically_derived]
            impl #impl_generics #component_ for #type_ident #ty_generics #where_clause {
                #required_tokens

                const NO_ENTITY: bool = #no_entity;
                const SUMMARY_TICK: bool = #summary_tick;

                #cloner_tokens

                #on_add_tokens
                #on_clone_tokens
                #on_insert_tokens
                #on_remove_tokens
                #on_discard_tokens
                #on_despawn_tokens

                #map_entities_tokens

                #register_tokens
            }

            #auto_register
        };
    }
}
