//! Parsing for `#[reflect(...)]` attributes.

use syn::Attribute;
use syn::Expr;
use syn::Token;
use syn::parse::ParseStream;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;

// -----------------------------------------------------------------------------
// TypeAttrs
// -----------------------------------------------------------------------------

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
    pub(crate) custom_attrs: Vec<Expr>,
    pub(crate) override_from_reflect: Option<Expr>,
    pub(crate) override_reflect_apply: Option<Expr>,
    pub(crate) addtional_on_register: Option<Expr>,
    /// The traits opted out of through `#[reflect(Trait = false)]`.
    ///
    /// The macro generates no implementation for them, so the user is expected
    /// to write their own.
    pub(crate) skips: TypeSkips,
}

/// The `#[reflect(Trait = false)]` opt-outs, one flag per generated trait.
///
/// `Opaque` is deliberately absent: the type-level `#[reflect(Opaque)]` already
/// claims that name, and it does the same thing from the other side — it says
/// the type is opaque and that the `Opaque` impl is the user's to write.
#[derive(Debug, Default)]
pub(crate) struct TypeSkips {
    pub(crate) reflect: bool,
    pub(crate) typed: bool,
    pub(crate) enum_: bool,
    pub(crate) struct_: bool,
    pub(crate) tuple: bool,
    pub(crate) type_database: bool,
}

impl TypeAttrs {
    /// Parse all `#[reflect(...)]` attributes from a type's attribute list.
    pub(crate) fn parse(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut result = TypeAttrs::default();

        for attr in find_reflect_attrs(attrs) {
            let content: TypeAttrsContent = attr.parse_args()?;
            result.merge(content.attrs)?;
        }

        Ok(result)
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

        self.skips.merge(&other.skips, duplicate_flag)?;

        self.is_opaque |= other.is_opaque;
        self.has_clone |= other.has_clone;
        self.has_eq |= other.has_eq;
        self.has_hash |= other.has_hash;
        self.has_debug |= other.has_debug;
        self.has_default |= other.has_default;
        self.has_serialize |= other.has_serialize;
        self.has_deserialize |= other.has_deserialize;
        self.custom_attrs.extend(other.custom_attrs);

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

// -----------------------------------------------------------------------------
// TypeSkips
// -----------------------------------------------------------------------------

/// The `#[reflect(Trait = false)]` names, each paired with its flag.
const SKIP_FLAGS: &[(&str, fn(&mut TypeSkips) -> &mut bool)] = &[
    ("Reflect", |s| &mut s.reflect),
    ("Typed", |s| &mut s.typed),
    ("Enum", |s| &mut s.enum_),
    ("Struct", |s| &mut s.struct_),
    ("Tuple", |s| &mut s.tuple),
    ("TypeDatabase", |s| &mut s.type_database),
];

impl TypeSkips {
    /// Applies `#[reflect(name = false)]`, if `name` is one of the opt-outs.
    ///
    /// Returns `false` when `name` is an ordinary override and the caller should
    /// keep looking.
    fn set(&mut self, name: &syn::Ident, expr: &Expr) -> syn::Result<bool> {
        let name_str = name.to_string();
        let Some((_, flag)) = SKIP_FLAGS.iter().find(|(n, _)| *n == name_str) else {
            return Ok(false);
        };

        let value = match expr {
            Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Bool(b),
                ..
            }) => b.value,
            _ => {
                let msg = format!(
                    "`{name}` is an opt-out; write `#[reflect({name} = false)]` to skip \
                     generating the implementation"
                );
                return Err(syn::Error::new(expr.span(), msg));
            }
        };

        let slot = flag(self);
        if *slot {
            return Err(duplicate_flag(&name.to_string()));
        }
        // `= true` is the default: the impl is generated.
        *slot = !value;

        Ok(true)
    }

    /// Combines two parsed attribute sets, rejecting a flag set twice.
    fn merge(&mut self, other: &TypeSkips, dup: fn(&str) -> syn::Error) -> syn::Result<()> {
        if self.reflect && other.reflect {
            return Err(dup("Reflect"));
        }
        if self.typed && other.typed {
            return Err(dup("Typed"));
        }
        if self.enum_ && other.enum_ {
            return Err(dup("Enum"));
        }
        if self.struct_ && other.struct_ {
            return Err(dup("Struct"));
        }
        if self.tuple && other.tuple {
            return Err(dup("Tuple"));
        }
        if self.type_database && other.type_database {
            return Err(dup("TypeDatabase"));
        }

        self.reflect |= other.reflect;
        self.typed |= other.typed;
        self.enum_ |= other.enum_;
        self.struct_ |= other.struct_;
        self.tuple |= other.tuple;
        self.type_database |= other.type_database;

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// FieldAttrs
// -----------------------------------------------------------------------------

/// Parsed `#[reflect(...)]` field-level attributes.
#[derive(Debug, Default)]
pub(crate) struct FieldAttrs {
    pub(crate) is_ignored: bool,
    pub(crate) has_default: bool,
    pub(crate) has_clone: bool,
    pub(crate) has_serialize: bool,
    pub(crate) has_deserialize: bool,
    pub(crate) custom_attrs: Vec<Expr>,
    /// The reflected wrapper of this field's type, from `#[reflect(remote = ...)]`.
    pub(crate) remote: Option<Expr>,
}

impl FieldAttrs {
    /// Parse all `#[reflect(...)]` attributes from a field's attribute list.
    pub(crate) fn parse(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut result = FieldAttrs::default();

        for attr in find_reflect_attrs(attrs) {
            let content: FieldAttrsContent = attr.parse_args()?;
            result.merge(content.attrs)?;
        }

        Ok(result)
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
        if other.has_serialize && self.has_serialize {
            return Err(duplicate_flag("serialize"));
        }
        if other.has_deserialize && self.has_deserialize {
            return Err(duplicate_flag("deserialize"));
        }

        self.is_ignored |= other.is_ignored;
        self.has_default |= other.has_default;
        self.has_clone |= other.has_clone;
        self.has_serialize |= other.has_serialize;
        self.has_deserialize |= other.has_deserialize;
        self.custom_attrs.extend(other.custom_attrs);

        if let Some(v) = other.remote {
            if self.remote.is_some() {
                return Err(duplicate_flag("remote"));
            }
            self.remote = Some(v);
        }

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// find_reflect_attrs
// -----------------------------------------------------------------------------

fn find_reflect_attrs(attrs: &[Attribute]) -> impl Iterator<Item = &'_ Attribute> {
    attrs.iter().filter(|a| a.path().is_ident("reflect"))
}

// -----------------------------------------------------------------------------
// Error
// -----------------------------------------------------------------------------

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

// -----------------------------------------------------------------------------
// TypeMetaItem
// -----------------------------------------------------------------------------

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

// -----------------------------------------------------------------------------
// TypeAttrsContent
// -----------------------------------------------------------------------------

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

const VALID_OVERRIDES: &str = "from_reflect, reflect_apply, on_register";
const VALID_SKIPS: &str = "Reflect, Typed, Enum, Struct, Tuple, TypeDatabase";

fn set_expr(attrs: &mut TypeAttrs, name: &syn::Ident, expr: Expr) -> syn::Result<()> {
    if attrs.skips.set(name, &expr)? {
        return Ok(());
    }

    let slot = match name.to_string().as_str() {
        "from_reflect" => &mut attrs.override_from_reflect,
        "reflect_apply" => &mut attrs.override_reflect_apply,
        "on_register" => &mut attrs.addtional_on_register,
        _ => {
            let msg = format!(
                "unknown override `{name}`; valid overrides are: {VALID_OVERRIDES}. \
                 Valid opt-outs are: {VALID_SKIPS}. \
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

// -----------------------------------------------------------------------------
// FieldMetaItem
// -----------------------------------------------------------------------------

enum FieldMetaItem {
    CustomAttr(Expr),
    Flag {
        name: syn::Ident,
        span: proc_macro2::Span,
    },
    /// `name = expr`, for the options that carry a value rather than being a flag.
    Value {
        name: syn::Ident,
        value: Expr,
    },
}

impl syn::parse::Parse for FieldMetaItem {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Token![@]) {
            input.parse::<Token![@]>()?;
            return Ok(Self::CustomAttr(input.parse()?));
        }
        let ident: syn::Ident = input.parse()?;
        if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            return Ok(Self::Value {
                name: ident,
                value: input.parse()?,
            });
        }
        Ok(Self::Flag {
            span: ident.span(),
            name: ident,
        })
    }
}

// -----------------------------------------------------------------------------
// FieldAttrsContent
// -----------------------------------------------------------------------------

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

const VALID_FIELD_FLAGS: &str = "ignore, default, clone, serialize, deserialize, remote";

fn apply_field_item(attrs: &mut FieldAttrs, item: FieldMetaItem) -> syn::Result<()> {
    match item {
        FieldMetaItem::CustomAttr(expr) => attrs.custom_attrs.push(expr),
        FieldMetaItem::Flag { name, span } => set_field_flag(attrs, &name, span)?,
        FieldMetaItem::Value { name, value } => set_field_value(attrs, &name, value)?,
    }
    Ok(())
}

fn set_field_value(attrs: &mut FieldAttrs, name: &syn::Ident, value: Expr) -> syn::Result<()> {
    let slot = match name.to_string().as_str() {
        "remote" => &mut attrs.remote,
        _ => {
            let msg = format!(
                "unknown field option `{name}`; the field attributes that take a value are: remote"
            );
            return Err(syn::Error::new(name.span(), msg));
        }
    };
    if slot.is_some() {
        let msg = format!("duplicate `{name}`; each option can only be set once");
        return Err(syn::Error::new(name.span(), msg));
    }
    *slot = Some(value);

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
        "serialize" => &mut attrs.has_serialize,
        "deserialize" => &mut attrs.has_deserialize,
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
