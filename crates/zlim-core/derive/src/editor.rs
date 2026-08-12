use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, Fields, Ident, Index, Type};

// -----------------------------------------------------------------------------
// Editor field metadata
// -----------------------------------------------------------------------------

/// Describes the editor visibility of a field.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorKind {
    /// `#[editor(mutable)]` — field appears in both `FIELDS` and
    /// `MUTABLE_FIELDS`; exposed via `field` and `field_mut`.
    Mutable,
    /// `#[editor(readonly)]` — field appears in `FIELDS` and
    /// `READONLY_FIELDS`; exposed via `field` only.
    Readonly,
}

/// A field annotated with `#[editor(…)]`, carrying its name/accessor and
/// type for constraint generation.
pub(crate) struct EditorField<'a> {
    /// The field identifier (name or positional index token stream).
    pub(crate) access: TokenStream,
    /// The field name as a string literal for the const arrays.
    pub(crate) name_str: String,
    /// The field type.
    pub(crate) ty: &'a Type,
    /// Mutable vs Readonly.
    pub(crate) kind: EditorKind,
}

// -----------------------------------------------------------------------------
// Attribute parsing
// -----------------------------------------------------------------------------

/// Parse `#[editor(mutable)]` / `#[editor(readonly)]` from the given
/// attributes.  Returns `None` when the field has no editor annotation.
pub(crate) fn parse_editor_kind(attrs: &[syn::Attribute]) -> Option<EditorKind> {
    for attr in attrs {
        if !attr.path().is_ident("editor") {
            continue;
        }
        let Ok(param) = attr.parse_args::<Ident>() else {
            continue;
        };
        match param.to_string().as_str() {
            "mutable" => return Some(EditorKind::Mutable),
            "readonly" => return Some(EditorKind::Readonly),
            _ => continue,
        }
    }
    None
}

// -----------------------------------------------------------------------------
// Field collection
// -----------------------------------------------------------------------------

/// Walk the struct fields, picking up every `#[editor(…)]` entry.
pub(crate) fn collect_editor_fields(data: &Data) -> Result<Vec<EditorField<'_>>, syn::Error> {
    let fields = match data {
        Data::Struct(s) => &s.fields,
        Data::Enum(_) => {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "can only be derived for structs",
            ));
        }
        Data::Union(_) => {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "can only be derived for structs",
            ));
        }
    };

    let mut result = Vec::new();

    match fields {
        Fields::Named(named) => {
            for field in &named.named {
                let Some(kind) = parse_editor_kind(&field.attrs) else {
                    continue;
                };
                let ident = field.ident.as_ref().unwrap();
                let name = ident.to_string();
                result.push(EditorField {
                    access: quote! { #ident },
                    name_str: name,
                    ty: &field.ty,
                    kind,
                });
            }
        }
        Fields::Unnamed(unnamed) => {
            for (i, field) in unnamed.unnamed.iter().enumerate() {
                let Some(kind) = parse_editor_kind(&field.attrs) else {
                    continue;
                };
                let index = Index::from(i);
                let name = format!("{i}");
                result.push(EditorField {
                    access: quote! { #index },
                    name_str: name,
                    ty: &field.ty,
                    kind,
                });
            }
        }
        Fields::Unit => {}
    }

    Ok(result)
}

// -----------------------------------------------------------------------------
// Token generation helpers
// -----------------------------------------------------------------------------

/// Generates the `const FIELDS` / `MUTABLE_FIELDS` / `READONLY_FIELDS`
/// and `fn field` / `fn field_mut` tokens from the collected editor fields.
pub(crate) struct EditorTokens {
    pub(crate) fields: TokenStream,
    pub(crate) mutable_fields: TokenStream,
    pub(crate) readonly_fields: TokenStream,
    pub(crate) field_fn: TokenStream,
    pub(crate) field_mut_fn: TokenStream,
}

/// Build all editor-related token streams from the collected fields.
pub(crate) fn gen_editor_tokens(
    editor_fields: &[EditorField<'_>],
    reflect_: &TokenStream,
) -> EditorTokens {
    let all_names: Vec<_> = editor_fields.iter().map(|f| &f.name_str).collect();
    let mutable_names: Vec<_> = editor_fields
        .iter()
        .filter(|f| f.kind == EditorKind::Mutable)
        .map(|f| &f.name_str)
        .collect();
    let readonly_names: Vec<_> = editor_fields
        .iter()
        .filter(|f| f.kind == EditorKind::Readonly)
        .map(|f| &f.name_str)
        .collect();

    let field_arms = editor_fields.iter().map(|f| {
        let name = &f.name_str;
        let access = &f.access;
        quote! {
            #name => ::core::option::Option::Some(
                &self.#access as &dyn #reflect_
            ),
        }
    });

    let mut_arms = editor_fields
        .iter()
        .filter(|f| f.kind == EditorKind::Mutable)
        .map(|f| {
            let name = &f.name_str;
            let access = &f.access;
            quote! {
                #name => ::core::option::Option::Some(
                    &mut self.#access as &mut dyn #reflect_
                ),
            }
        });

    EditorTokens {
        fields: quote! { &[ #( #all_names ),* ] },
        mutable_fields: quote! { &[ #( #mutable_names ),* ] },
        readonly_fields: quote! { &[ #( #readonly_names ),* ] },
        field_fn: quote! {
            fn field(&self, name: &str) -> ::core::option::Option<&dyn #reflect_> {
                match name {
                    #( #field_arms )*
                    _ => ::core::option::Option::None,
                }
            }
        },
        field_mut_fn: quote! {
            fn field_mut(&mut self, name: &str) -> ::core::option::Option<&mut dyn #reflect_> {
                match name {
                    #( #mut_arms )*
                    _ => ::core::option::Option::None,
                }
            }
        },
    }
}
