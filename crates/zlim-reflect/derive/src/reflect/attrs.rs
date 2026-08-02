//! Parsing for `#[reflect(...)]` attributes.

use syn::Attribute;
use syn::Expr;
use syn::Token;
use syn::parse::ParseStream;
use syn::punctuated::Punctuated;

// ----------------------------------------------------------------------------
// TypeAttrs
// ----------------------------------------------------------------------------

/// Parsed `#[reflect(...)]` type-level attributes.
#[derive(Debug, Default)]
pub(crate) struct TypeAttrs {
    pub(crate) is_opaque: bool,
    pub(crate) has_clone: bool,
    pub(crate) has_eq: bool,
    pub(crate) has_hash: bool,
    pub(crate) has_debug: bool,
    pub(crate) has_default: bool,
    pub(crate) has_serialize: bool,
    pub(crate) has_deserialize: bool,
    pub(crate) docs: Vec<String>,
    pub(crate) custom_attrs: Vec<Expr>,
    pub(crate) override_is_compatible: Option<Expr>,
    pub(crate) override_from_reflect: Option<Expr>,
    pub(crate) override_reflect_apply: Option<Expr>,
    pub(crate) addtional_on_register: Option<Expr>,
}

impl TypeAttrs {
    /// Parse all `#[reflect(...)]` attributes from a type's attribute list.
    pub(crate) fn parse(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut result = TypeAttrs {
            docs: collect_docs(attrs),
            ..Default::default()
        };

        for attr in find_reflect_attrs(attrs) {
            let content: TypeAttrsContent = attr.parse_args()?;
            result.merge(content.attrs)?;
        }

        Ok(result)
    }

    /// Returns the merged doc string, if any.
    pub(crate) fn doc_string(&self) -> Option<String> {
        if self.docs.is_empty() {
            return None;
        }
        Some(self.docs.join("\n"))
    }

    fn merge(&mut self, other: TypeAttrs) -> syn::Result<()> {
        if other.is_opaque && self.is_opaque {
            return Err(duplicate_flag("Opaque"));
        }
        if other.has_clone && self.has_clone {
            return Err(duplicate_flag("Clone"));
        }
        if other.has_eq && self.has_eq {
            return Err(duplicate_flag("Eq"));
        }
        if other.has_hash && self.has_hash {
            return Err(duplicate_flag("Hash"));
        }
        if other.has_debug && self.has_debug {
            return Err(duplicate_flag("Debug"));
        }
        if other.has_default && self.has_default {
            return Err(duplicate_flag("Default"));
        }
        if other.has_serialize && self.has_serialize {
            return Err(duplicate_flag("Serialize"));
        }
        if other.has_deserialize && self.has_deserialize {
            return Err(duplicate_flag("Deserialize"));
        }

        self.is_opaque |= other.is_opaque;
        self.has_clone |= other.has_clone;
        self.has_eq |= other.has_eq;
        self.has_hash |= other.has_hash;
        self.has_debug |= other.has_debug;
        self.has_default |= other.has_default;
        self.has_serialize |= other.has_serialize;
        self.has_deserialize |= other.has_deserialize;
        self.custom_attrs.extend(other.custom_attrs);

        if let Some(v) = other.override_is_compatible {
            if self.override_is_compatible.is_some() {
                return Err(duplicate_override("is_compatible"));
            }
            self.override_is_compatible = Some(v);
        }
        if let Some(v) = other.override_from_reflect {
            if self.override_from_reflect.is_some() {
                return Err(duplicate_override("from_reflect"));
            }
            self.override_from_reflect = Some(v);
        }
        if let Some(v) = other.override_reflect_apply {
            if self.override_reflect_apply.is_some() {
                return Err(duplicate_override("reflect_apply"));
            }
            self.override_reflect_apply = Some(v);
        }
        if let Some(v) = other.addtional_on_register {
            if self.addtional_on_register.is_some() {
                return Err(duplicate_override("on_register"));
            }
            self.addtional_on_register = Some(v);
        }
        Ok(())
    }
}

// ----------------------------------------------------------------------------
// FieldAttrs
// ----------------------------------------------------------------------------

/// Parsed `#[reflect(...)]` field-level attributes.
#[derive(Debug, Default)]
pub(crate) struct FieldAttrs {
    pub(crate) is_ignored: bool,
    pub(crate) has_default: bool,
    pub(crate) has_clone: bool,
    pub(crate) custom_attrs: Vec<Expr>,
    pub(crate) docs: Vec<String>,
}

impl FieldAttrs {
    /// Parse all `#[reflect(...)]` attributes from a field's attribute list.
    pub(crate) fn parse(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut result = FieldAttrs {
            docs: collect_docs(attrs),
            ..Default::default()
        };

        for attr in find_reflect_attrs(attrs) {
            let content: FieldAttrsContent = attr.parse_args()?;
            result.merge(content.attrs)?;
        }

        Ok(result)
    }

    /// Returns the merged doc string, if any.
    pub(crate) fn doc_string(&self) -> Option<String> {
        if self.docs.is_empty() {
            return None;
        }
        Some(self.docs.join("\n"))
    }

    fn merge(&mut self, other: FieldAttrs) -> syn::Result<()> {
        if other.is_ignored && self.is_ignored {
            return Err(duplicate_flag("ignored"));
        }
        if other.has_default && self.has_default {
            return Err(duplicate_flag("default"));
        }
        if other.has_clone && self.has_clone {
            return Err(duplicate_flag("clone"));
        }

        self.is_ignored |= other.is_ignored;
        self.has_default |= other.has_default;
        self.has_clone |= other.has_clone;
        self.custom_attrs.extend(other.custom_attrs);

        Ok(())
    }
}

// ----------------------------------------------------------------------------
// collect_docs
// ----------------------------------------------------------------------------

fn collect_docs(_attrs_: &[Attribute]) -> Vec<String> {
    #[cfg(not(feature = "reflect_docs"))]
    return Vec::new();

    #[cfg(feature = "reflect_docs")]
    return _attrs_
        .iter()
        .filter_map(|a| {
            if !a.path().is_ident("doc") {
                return None;
            }
            let meta: syn::MetaNameValue = a.parse_args().ok()?;
            match &meta.value {
                Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) => Some(s.value()),
                _ => None,
            }
        })
        .collect();
}

// ----------------------------------------------------------------------------
// find_reflect_attrs
// ----------------------------------------------------------------------------

fn find_reflect_attrs(attrs: &[Attribute]) -> impl Iterator<Item = &'_ Attribute> {
    attrs.iter().filter(|a| a.path().is_ident("reflect"))
}

// ----------------------------------------------------------------------------
// Error
// ----------------------------------------------------------------------------

fn duplicate_flag(name: &str) -> syn::Error {
    let msg = format!(
        "duplicate `{name}` across multiple `#[reflect(...)]` attributes; \
         each flag can only be set once",
    );
    syn::Error::new(proc_macro2::Span::call_site(), msg)
}

fn duplicate_override(name: &str) -> syn::Error {
    let msg = format!(
        "duplicate `{name}` across multiple `#[reflect(...)]` attributes; \
         each override can only be set once",
    );
    syn::Error::new(proc_macro2::Span::call_site(), msg)
}

// ----------------------------------------------------------------------------
// TypeMetaItem
// ----------------------------------------------------------------------------

enum TypeMetaItem {
    CustomAttr(Expr),
    Expr {
        name: syn::Ident,
        expr: Expr,
    },
    Flag {
        name: syn::Ident,
        span: proc_macro2::Span,
    },
}

impl syn::parse::Parse for TypeMetaItem {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Token![@]) {
            input.parse::<Token![@]>()?;
            return Ok(Self::CustomAttr(input.parse()?));
        }

        let ident: syn::Ident = input.parse()?;

        if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            return Ok(Self::Expr {
                name: ident,
                expr: input.parse()?,
            });
        }

        Ok(Self::Flag {
            span: ident.span(),
            name: ident,
        })
    }
}

// ----------------------------------------------------------------------------
// TypeAttrsContent
// ----------------------------------------------------------------------------

struct TypeAttrsContent {
    attrs: TypeAttrs,
}

impl syn::parse::Parse for TypeAttrsContent {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut attrs = TypeAttrs::default();
        let items: Punctuated<TypeMetaItem, Token![,]> =
            input.parse_terminated(TypeMetaItem::parse, Token![,])?;

        for item in items {
            apply_type_item(&mut attrs, item)?;
        }

        Ok(Self { attrs })
    }
}

fn apply_type_item(attrs: &mut TypeAttrs, item: TypeMetaItem) -> syn::Result<()> {
    match item {
        TypeMetaItem::CustomAttr(expr) => attrs.custom_attrs.push(expr),
        TypeMetaItem::Expr { name, expr } => set_expr(attrs, &name, expr)?,
        TypeMetaItem::Flag { name, span } => set_flag(attrs, &name, span)?,
    }
    Ok(())
}

const VALID_FLAGS: &str = "Opaque, Clone, Eq, Hash, Debug, Default, Serialize, Deserialize";

fn set_flag(attrs: &mut TypeAttrs, name: &syn::Ident, span: proc_macro2::Span) -> syn::Result<()> {
    let slot = match name.to_string().as_str() {
        "Opaque" => &mut attrs.is_opaque,
        "Clone" => &mut attrs.has_clone,
        "Eq" => &mut attrs.has_eq,
        "Hash" => &mut attrs.has_hash,
        "Debug" => &mut attrs.has_debug,
        "Default" => &mut attrs.has_default,
        "Serialize" => &mut attrs.has_serialize,
        "Deserialize" => &mut attrs.has_deserialize,
        _ => {
            let msg = format!("unknown reflect attribute `{name}`; valid flags are: {VALID_FLAGS}");
            return Err(syn::Error::new(span, msg));
        }
    };
    if *slot {
        let msg = format!("duplicate `{name}`; each flag can only be set once");
        return Err(syn::Error::new(span, msg));
    }
    *slot = true;

    Ok(())
}

const VALID_OVERRIDES: &str = "is_compatible, from_reflect, reflect_apply, on_register";

fn set_expr(attrs: &mut TypeAttrs, name: &syn::Ident, expr: Expr) -> syn::Result<()> {
    let slot = match name.to_string().as_str() {
        "is_compatible" => &mut attrs.override_is_compatible,
        "from_reflect" => &mut attrs.override_from_reflect,
        "reflect_apply" => &mut attrs.override_reflect_apply,
        "on_register" => &mut attrs.addtional_on_register,
        _ => {
            let msg = format!(
                "unknown override `{name}`; valid overrides are: {VALID_OVERRIDES}. \
                 Use `#[reflect({name} = your_fn)]` syntax."
            );
            return Err(syn::Error::new(name.span(), msg));
        }
    };
    if slot.is_some() {
        let msg = format!("duplicate `{name}`; each override can only be set once");
        return Err(syn::Error::new(name.span(), msg));
    }
    *slot = Some(expr);
    Ok(())
}

// ----------------------------------------------------------------------------
// FieldMetaItem
// ----------------------------------------------------------------------------

enum FieldMetaItem {
    CustomAttr(Expr),
    Flag {
        name: syn::Ident,
        span: proc_macro2::Span,
    },
}

impl syn::parse::Parse for FieldMetaItem {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Token![@]) {
            input.parse::<Token![@]>()?;
            return Ok(Self::CustomAttr(input.parse()?));
        }
        let ident: syn::Ident = input.parse()?;
        Ok(Self::Flag {
            span: ident.span(),
            name: ident,
        })
    }
}

// ----------------------------------------------------------------------------
// FieldAttrsContent
// ----------------------------------------------------------------------------

struct FieldAttrsContent {
    attrs: FieldAttrs,
}

impl syn::parse::Parse for FieldAttrsContent {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut attrs = FieldAttrs::default();
        let items: Punctuated<FieldMetaItem, Token![,]> =
            input.parse_terminated(FieldMetaItem::parse, Token![,])?;

        for item in items {
            apply_field_item(&mut attrs, item)?;
        }

        Ok(Self { attrs })
    }
}

const VALID_FIELD_FLAGS: &str = "ignore, default, clone";

fn apply_field_item(attrs: &mut FieldAttrs, item: FieldMetaItem) -> syn::Result<()> {
    match item {
        FieldMetaItem::CustomAttr(expr) => attrs.custom_attrs.push(expr),
        FieldMetaItem::Flag { name, span } => set_field_flag(attrs, &name, span)?,
    }
    Ok(())
}

fn set_field_flag(
    attrs: &mut FieldAttrs,
    name: &syn::Ident,
    span: proc_macro2::Span,
) -> syn::Result<()> {
    let slot = match name.to_string().as_str() {
        "ignore" => &mut attrs.is_ignored,
        "default" => &mut attrs.has_default,
        "clone" => &mut attrs.has_clone,
        _ => {
            let msg = format!(
                "unknown field attribute `{name}`; valid field attributes are: {VALID_FIELD_FLAGS}"
            );
            return Err(syn::Error::new(span, msg));
        }
    };
    if *slot {
        let msg = format!("duplicate `{name}`; each flag can only be set once");
        return Err(syn::Error::new(span, msg));
    }
    *slot = true;

    Ok(())
}
