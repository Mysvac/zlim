use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::parse_quote;
use syn::{Data, DeriveInput, Fields, Ident, Index, Type};

use crate::editor;
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
}

/// Parses a hook path from a nested meta, supporting both
/// `on_add = path::function` and `on_add(path::function)` syntax.
fn parse_hook_expr(meta: &syn::meta::ParseNestedMeta) -> syn::Result<syn::ExprPath> {
    if meta.input.peek(syn::Token![=]) {
        let value = meta.value()?;
        value.parse::<syn::ExprPath>()
    } else {
        let content;
        syn::parenthesized!(content in meta.input);
        content.parse::<syn::ExprPath>()
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
    };

    for attr in attrs {
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
            } else {
                Err(meta.error("unsupported component attribute"))
            }
        })?;
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
    let reflect_ = crate::path::reflect_(&zlim_core);

    let type_ident = &ast.ident;
    let mut generics = ast.generics;

    // --- generic bounds ------------------------------------------------
    if generics.type_params().next().is_some() {
        let type_path_ = crate::path::type_path_(&zlim_core);
        let serialize_ = crate::path::serialize_(&zlim_core);
        let deserialize_ = crate::path::deserialize_(&zlim_core);

        let predicates = &mut generics.make_where_clause().predicates;
        match &attrs.cloner {
            Cloner::Copy => predicates.push(parse_quote! {
                Self: ::core::marker::Copy + #type_path_ + #serialize_ + for<'__de_x_> #deserialize_<'__de_x_>
                    + ::core::marker::Send + ::core::marker::Sync + ::core::marker::Sized + 'static
            }),
            _ => predicates.push(parse_quote! {
                Self: ::core::clone::Clone + #type_path_ + #serialize_ + for<'__de_x_> #deserialize_<'__de_x_> +
                    ::core::marker::Send + ::core::marker::Sync + ::core::marker::Sized + 'static
            }),
        }
    } else if generics.lifetimes().next().is_some() {
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote! { Self: 'static });
    }

    let generic_idents: Vec<Ident> = generics.type_params().map(|p| p.ident.clone()).collect();

    // --- editor fields -------------------------------------------------
    let editor_fields = match editor::collect_editor_fields(&ast.data) {
        Ok(f) => f,
        Err(e) => return e.into_compile_error(),
    };

    for f in &editor_fields {
        if contains_any_idents(f.ty, &generic_idents) {
            field_type_constraint(&mut generics, f.ty, &reflect_);
        }
    }

    let et = editor::gen_editor_tokens(&editor_fields, &reflect_);
    let ef = et.fields;
    let emf = et.mutable_fields;
    let erf = et.readonly_fields;
    let eff = et.field_fn;
    let efm = et.field_mut_fn;

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
    let no_entity = !entity_fields.is_empty();

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
        Some(p) => quote! { const ON_ADD: Option<#component_hook_> = Some(#p); },
        None => TokenStream::new(),
    };
    let on_clone_tokens = match &attrs.on_clone {
        Some(p) => quote! { const ON_CLONE: Option<#component_hook_> = Some(#p); },
        None => TokenStream::new(),
    };
    let on_insert_tokens = match &attrs.on_insert {
        Some(p) => quote! { const ON_INSERT: Option<#component_hook_> = Some(#p); },
        None => TokenStream::new(),
    };
    let on_remove_tokens = match &attrs.on_remove {
        Some(p) => quote! { const ON_REMOVE: Option<#component_hook_> = Some(#p); },
        None => TokenStream::new(),
    };
    let on_discard_tokens = match &attrs.on_discard {
        Some(p) => quote! { const ON_DISCARD: Option<#component_hook_> = Some(#p); },
        None => TokenStream::new(),
    };
    let on_despawn_tokens = match &attrs.on_despawn {
        Some(p) => quote! { const ON_DESPAWN: Option<#component_hook_> = Some(#p); },
        None => TokenStream::new(),
    };

    // --- map_entities --------------------------------------------------
    let map_entities_tokens = if attrs.map_entities.is_some() {
        // User-specified function — NO_ENTITY stays false (default).
        TokenStream::new()
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
            fn map_entities<__M_Z_: #entity_mapper_>(&mut self, mapper: &mut __M_Z_) {
                #(#calls)*
            }
        }
    };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // --- auto-registration (non-generic types only) -------------------
    let auto_register = if generics.type_params().next().is_none() {
        quote! {
            #zlim_core::component::__internal__::submit!(
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
                const NO_ENTITY: bool = #no_entity;

                #cloner_tokens

                #on_add_tokens
                #on_clone_tokens
                #on_insert_tokens
                #on_remove_tokens
                #on_discard_tokens
                #on_despawn_tokens

                const FIELDS: &'static [&'static str] = #ef;
                const MUTABLE_FIELDS: &'static [&'static str] = #emf;
                const READONLY_FIELDS: &'static [&'static str] = #erf;

                #eff
                #efm

                #map_entities_tokens
            }

            #auto_register
        };
    }
}
