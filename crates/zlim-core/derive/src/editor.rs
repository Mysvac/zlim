use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, Fields, Index, Type};

// -----------------------------------------------------------------------------
// Editor field metadata
// -----------------------------------------------------------------------------

/// Describes the editor access of a field.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct EditorAccess {
    /// `#[editor(get)]` — field appears in `GETTER`; readable via
    /// `get_field`.
    pub(crate) getter: bool,
    /// `#[editor(set)]` — field appears in `SETTER`; writable via
    /// `set_field`.
    pub(crate) setter: bool,
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
    /// Getter / setter access.
    pub(crate) kind: EditorAccess,
}

// -----------------------------------------------------------------------------
// Attribute parsing
// -----------------------------------------------------------------------------

/// Parse `#[editor(get)]` / `#[editor(set)]` (or a comma-separated
/// combination) from the given attributes.  Returns `None` when the field
/// has no editor annotation.
pub(crate) fn parse_editor_kind(attrs: &[syn::Attribute]) -> Option<EditorAccess> {
    let mut out = EditorAccess::default();

    for attr in attrs {
        if !attr.path().is_ident("editor") {
            continue;
        }
        let Ok(_) = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("get") {
                out.getter = true;
                Ok(())
            } else if meta.path.is_ident("set") {
                out.setter = true;
                Ok(())
            } else {
                Err(meta.error("unsupported editor option; expected `get` or `set`."))
            }
        }) else {
            continue;
        };
    }

    (out.getter || out.setter).then_some(out)
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

/// Generates the `const GETTER` / `const SETTER` lists and the
/// `fn get_field` / `fn set_field` tokens from the collected editor fields.
pub(crate) struct EditorTokens {
    pub(crate) getter: TokenStream,
    pub(crate) setter: TokenStream,
    pub(crate) get_field_fn: TokenStream,
    pub(crate) set_field_fn: TokenStream,
}

/// Build all editor-related token streams from the collected fields.
///
/// `type_name` is the derive-time identifier of the type, embedded in the
/// error messages returned by `set_field`.
pub(crate) fn gen_editor_tokens(
    editor_fields: &[EditorField<'_>],
    type_name: &str,
    reflect_: &TokenStream,
) -> EditorTokens {
    let getter_names: Vec<_> = editor_fields
        .iter()
        .filter(|f| f.kind.getter)
        .map(|f| &f.name_str)
        .collect();
    let setter_names: Vec<_> = editor_fields
        .iter()
        .filter(|f| f.kind.setter)
        .map(|f| &f.name_str)
        .collect();

    let getter_arms = editor_fields.iter().filter(|f| f.kind.getter).map(|f| {
        let name = &f.name_str;
        let access = &f.access;
        quote! {
            #name => ::core::option::Option::Some(
                &self.#access as &dyn #reflect_
            ),
        }
    });

    let setter_arms = editor_fields.iter().filter(|f| f.kind.setter).map(|f| {
        let name = &f.name_str;
        let access = &f.access;
        // Built at derive time — quote does not interpolate inside
        // string literals.  `{{e}}` escapes to a runtime `{e}` capture.
        let err_msg = format!("Type `{type_name}` failed to assign field `{name}`: {{e}}");
        quote! {
            #name => {
                #reflect_::reflect_apply(&mut self.#access, value).map_err(|e| {
                    ::std::format!(#err_msg)
                })
            },
        }
    });

    // `{{name}}` escapes to a runtime `{name}` capture.
    let missing_msg = format!("Type `{type_name}` is missing field `{{name}}`");

    EditorTokens {
        getter: quote! { &[ #( #getter_names ),* ] },
        setter: quote! { &[ #( #setter_names ),* ] },
        get_field_fn: quote! {
            fn get_field<'a>(&'a self, name: &str) -> ::core::option::Option<&'a dyn #reflect_> {
                match name {
                    #( #getter_arms )*
                    _ => ::core::option::Option::None,
                }
            }
        },
        set_field_fn: quote! {
            fn set_field(
                &mut self,
                name: &str,
                value: &dyn #reflect_,
            ) -> ::core::result::Result<(), ::std::string::String> {
                match name {
                    #( #setter_arms )*
                    _ => ::core::result::Result::Err(::std::format!(#missing_msg)),
                }
            }
        },
    }
}
