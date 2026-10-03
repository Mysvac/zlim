//! The `#[derive(IntoTemplate)]` macro.
//!
//! The derive generates the canonical template of a type: a companion type named
//! `<Type>Template`, whose fields are the templates of the fields of the original type, together
//! with the conversion between the two — `From<Type> for <Type>Template`, which describes an
//! existing value field by field, and the [`IntoTemplate`] impl that delegates to it.
//!
//! It also makes the type itself not `Unpin`, which is what keeps the generated `impl
//! IntoTemplate` from overlapping with the blanket implementation that every `Clone + Default`
//! type gets.
//!
//! # Layout
//!
//! A struct and an enum generate the same impls, but they get there differently: a struct's fields
//! are reached through `self` and its template has the shape of the struct, while an enum has to
//! match the variant first and its template has the variants of the enum. The two are therefore
//! separate entry points — [`template_for_struct`] and [`template_for_enum`] — and everything they
//! share is emitted by the helpers above them:
//!
//! - [`parts_of`] — reads every field once, into the [`TemplateParts`] the whole expansion works
//!   from. The caller adds the bounds those parts ask for to the deriving type before anything is
//!   emitted, so everything below reads a [`DeriveInput`] that is already constrained.
//! - [`trait_impls`] — the `Template` impl, which builds and clones the template.
//! - [`default_impl`] — the `Default` impl. Only its *body* comes from the caller, because an
//!   enum's default is a variant rather than a value built field by field.
//! - [`into_template_impls`] — the `From`/`IntoTemplate` pair, which converts a value.
//! - [`not_unpin_impl`] — the `Unpin` opt-out that makes the others legal.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{
    Data, DeriveInput, Fields, Generics, Ident, Path, Visibility, WhereClause, WherePredicate,
    parse_quote,
};

use crate::path;
use crate::utils::contains_any_idents;

// -----------------------------------------------------------------------------
// Attributes

/// The field attribute that picks the template of that field.
const TEMPLATE_ATTRIBUTE: &str = "template";

/// The variant attribute that picks the variant an enum template defaults to.
const DEFAULT_ATTRIBUTE: &str = "default";

/// The `#[template(built_in)]` option, which uses the built-in template of the field type.
const BUILT_IN_OPTION: &str = "built_in";

/// Which template a field is described by.
///
/// `#[template(...)]` selects this, and it decides two things at once: the type of the template
/// field, and how `From` converts the field into it.
#[derive(Clone)]
enum TemplateChoice {
    /// The field type's own `IntoTemplate`.
    Canonical,

    /// The field type's `BuiltInTemplate`, which is what maps a container to the template of its
    /// element.
    BuiltIn,

    /// The template named on the field, which the field converts into with `Into`.
    Named(Path),
}

impl TemplateChoice {
    /// The bound `field` needs for this choice to hold, if any.
    ///
    /// This is only about the fields that mention a type parameter: a field of a concrete type
    /// already satisfies whatever it needs. A field named explicitly carries no bound, because it
    /// is the deriving type's own business to convert into it.
    fn constraint(&self, field: &syn::Type, zlim_core: &Path) -> Option<WherePredicate> {
        match self {
            Self::Canonical => {
                let into_template_ = path::into_template_(zlim_core);
                Some(parse_quote! { #field: #into_template_ })
            }
            Self::BuiltIn => {
                // `BuiltInTemplate` is what says the field has a template of its own to fall back
                // to, and the trait is what pins that template's output, so the bound is the whole
                // requirement.
                let built_in_template_ = path::built_in_template_(zlim_core);
                Some(parse_quote! { #field: #built_in_template_ })
            }
            Self::Named(_) => None,
        }
    }
}

// -----------------------------------------------------------------------------
// Expand

/// Expands `#[derive(IntoTemplate)]` into the companion template type and its impls.
///
/// The bounds the fields need are added to the deriving type's generics here, before anything is
/// emitted, because every generated impl and the companion template's declaration are written from
/// them. Everything below therefore reads a [`DeriveInput`] that is already constrained.
pub(crate) fn expand(mut ast: DeriveInput) -> TokenStream {
    // Resolved once: it reads the crate's manifest, so every part of the expansion reuses it.
    let zlim_core = path::zlim_core_path();

    // The choices are read once here, since they say both what each field is bounded by and how it
    // converts.
    let parts = match parts_of(&ast, &zlim_core) {
        Ok(Some(parts)) => parts,
        Ok(None) => {
            const ERR: &str = "union types have no template";
            return syn::Error::new_spanned(&ast.ident, ERR).into_compile_error();
        }
        Err(error) => return error.into_compile_error(),
    };
    field_constraints(&mut ast, &parts);

    let context = ExpandContext {
        zlim_core: &zlim_core,
        type_ident: &ast.ident,
        template_ident: format_ident!("{}Template", ast.ident),
    };

    let expanded = match &ast.data {
        Data::Struct(_) => template_for_struct(&context, &ast, &parts),
        Data::Enum(_) => template_for_enum(&context, &ast, &parts),
        Data::Union(_) => unreachable!("the union was rejected above"),
    };
    match expanded {
        Ok(tokens) => tokens,
        Err(error) => error.into_compile_error(),
    }
}

/// The parts of the template of `ast`, or `None` for a union, which has none.
fn parts_of(ast: &DeriveInput, zlim_core: &Path) -> syn::Result<Option<TemplateParts>> {
    match &ast.data {
        Data::Struct(data) => TemplateParts::for_fields(&data.fields, zlim_core).map(Some),
        Data::Enum(data) => {
            let mut parts = TemplateParts::new(zlim_core);
            for variant in &data.variants {
                parts.absorb(TemplateParts::for_fields(&variant.fields, zlim_core)?);
            }
            Ok(Some(parts))
        }
        Data::Union(_) => Ok(None),
    }
}

/// What every part of the expansion needs to name things.
struct ExpandContext<'a> {
    /// The path of the crate the generated code refers to.
    zlim_core: &'a Path,

    /// The type being derived.
    type_ident: &'a Ident,

    /// The companion template type, `<Type>Template`.
    template_ident: Ident,
}

// -----------------------------------------------------------------------------
// The impls both shapes share

/// Emits the `Template` impl: the template builds the type it belongs to, and clones itself.
fn trait_impls(
    context: &ExpandContext<'_>,
    generics: &Generics,
    parts: &TemplateParts,
    build: TokenStream,
    clone: TokenStream,
) -> TokenStream {
    // `generics` is the constrained one: the bounds the fields need, plus what this impl adds.
    let ExpandContext {
        type_ident,
        template_ident,
        ..
    } = context;
    let (impl_generics, type_generics, _) = generics.split_for_impl();
    let template_ = path::template_(context.zlim_core);
    let template_context_ = path::template_context_(context.zlim_core);
    let zlim_result_ = path::zlim_result_(context.zlim_core);

    // Building the template builds each of its fields, so every field template has to be a
    // `Template`. That cannot be deduced from the bounds on the field types, so the predicate goes
    // on this impl rather than on the deriving type.
    let where_clause = parts.bounded_by(&template_, generics);

    quote! {
        impl #impl_generics #template_ for #template_ident #type_generics #where_clause {
            type Output = #type_ident #type_generics;

            fn build_template(
                &self,
                context: &mut #template_context_,
            ) -> #zlim_result_<Self::Output> {
                ::core::result::Result::Ok(#build)
            }

            fn clone_template(&self) -> Self {
                #clone
            }
        }
    }
}

/// Emits the `Default` impl of the template.
///
/// Only `body` comes from the caller, because an enum's default is a variant rather than a value
/// built field by field.
fn default_impl(
    context: &ExpandContext<'_>,
    generics: &Generics,
    parts: &TemplateParts,
    body: TokenStream,
) -> TokenStream {
    let template_ident = &context.template_ident;
    let (impl_generics, type_generics, _) = generics.split_for_impl();
    let default = parse_quote!(::core::default::Default);
    let where_clause = parts.bounded_by(&default, generics);

    quote! {
        impl #impl_generics ::core::default::Default for #template_ident #type_generics
            #where_clause
        {
            fn default() -> Self { #body }
        }
    }
}

/// Emits the pair that describes an existing value: `From<Type> for Template`, which converts the
/// value field by field, and the `IntoTemplate` impl that delegates to it.
fn into_template_impls(
    context: &ExpandContext<'_>,
    generics: &Generics,
    body: TokenStream,
) -> TokenStream {
    let ExpandContext {
        type_ident,
        template_ident,
        ..
    } = context;
    let into_template_ = path::into_template_(context.zlim_core);
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics ::core::convert::From<#type_ident #type_generics>
            for #template_ident #type_generics #where_clause
        {
            fn from(value: #type_ident #type_generics) -> Self { #body }
        }

        impl #impl_generics #into_template_ for #type_ident #type_generics #where_clause {
            type Template = #template_ident #type_generics;

            #[inline]
            fn into_template(self) -> Self::Template {
                ::core::convert::From::from(self)
            }
        }
    }
}

/// The generic arguments of a struct literal or a variant path, in turbofish form.
///
/// A path that carries generic arguments needs the turbofish to be read as an expression rather
/// than as a pattern, so `Holder::<T> { … }` is what the generated code writes. A type with no
/// generics gets nothing, since `Holder::<>` is not a path either.
fn generic_args(generics: &Generics) -> TokenStream {
    match generics.params.is_empty() {
        true => quote!(),
        false => {
            let (_, type_generics, _) = generics.split_for_impl();
            quote!(:: #type_generics)
        }
    }
}

/// The path of the type being derived, as a struct literal or a variant path.
fn value_path(context: &ExpandContext<'_>, ast: &DeriveInput) -> TokenStream {
    let type_ident = context.type_ident;
    let args = generic_args(&ast.generics);
    quote!(#type_ident #args)
}

/// The path of the companion template, as a struct literal or a variant path.
fn template_path(context: &ExpandContext<'_>, ast: &DeriveInput) -> TokenStream {
    let template_ident = &context.template_ident;
    let args = generic_args(&ast.generics);
    quote!(#template_ident #args)
}

/// The `Unpin` opt-out: the condition never holds, which is what keeps the generated
/// `IntoTemplate` impl out of the blanket implementation for `Clone + Default` types.
///
/// This adds a predicate the other impls must not see — they would have to prove a condition that
/// is false by construction — so it works on a copy rather than on the deriving type's generics.
fn not_unpin_impl(generics: &Generics, type_ident: &Ident, zlim_core: &Path) -> TokenStream {
    let specialize_from_template_ = path::specialize_from_template_(zlim_core);

    // The predicate goes on a copy of the generics rather than on a `where` clause of its own, so
    // that `split_for_impl` writes the comma-separated list the impl needs.
    let mut generics = generics.clone();
    generics
        .make_where_clause()
        .predicates
        .push(parse_quote! { for<'a> [()]: #specialize_from_template_ });
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics ::core::marker::Unpin for #type_ident #type_generics #where_clause {}
    }
}

/// Adds to `ast`'s generics the bounds its fields need.
///
/// Only a field whose type mentions a type parameter is constrained: one of a concrete type already
/// satisfies whatever it needs. A field that names its template carries no bound at all — converting
/// into a template the user named is the user's business.
///
/// The deriving type is modified in place rather than cloned, because the expansion owns it and
/// everything it emits is written from these generics: the bounds belong on the companion template's
/// declaration, which needs `<T as IntoTemplate>::Template` to be a well-formed type, and on the
/// impls, which is what lets the deriving type be written without repeating them.
fn field_constraints(ast: &mut DeriveInput, parts: &TemplateParts) {
    let type_params: Vec<Ident> = ast
        .generics
        .type_params()
        .map(|param| param.ident.clone())
        .collect();

    for choice in &parts.choices {
        if !contains_any_idents(&choice.field_type, &type_params) {
            continue;
        }
        let Some(constraint) = choice
            .choice
            .constraint(&choice.field_type, &parts.zlim_core)
        else {
            continue;
        };
        ast.generics.make_where_clause().predicates.push(constraint);
    }
}

// -----------------------------------------------------------------------------
// Structs

/// Emits everything a struct derives: the companion struct, and its impls.
///
/// `parts` and `ast` are the ones the caller already resolved and constrained, so nothing here has
/// to read the fields or the bounds again.
fn template_for_struct(
    context: &ExpandContext<'_>,
    ast: &DeriveInput,
    parts: &TemplateParts,
) -> syn::Result<TokenStream> {
    let Data::Struct(data) = &ast.data else {
        unreachable!("the caller dispatches on the data");
    };
    let fields = &data.fields;
    let value = value_path(context, ast);
    let template_qualified = template_path(context, ast);

    let declaration = template_declaration(
        &context.template_ident,
        &ast.vis,
        &ast.generics,
        fields,
        parts.declarations(),
    );

    // A struct reaches its fields through `self` — `self.name`, or `self.0` — and the emitters
    // borrow it where they need a reference.
    let access = |position: usize| -> TokenStream {
        match &parts.choices[position].name {
            Some(name) => quote!(self.#name),
            None => {
                let index = syn::Index::from(position);
                quote!(self.#index)
            }
        }
    };
    let borrow = |position: usize| -> TokenStream {
        let field = access(position);
        quote!(&#field)
    };

    // `From` consumes the value, and a field that names its template explicitly is converted with
    // `Into` — which needs the field by value. Destructuring moves every field out at once, which
    // is what makes all three conversions work.
    let bindings = parts.bindings();
    let binding = |position: usize| -> TokenStream {
        let name = &bindings[position];
        quote!(#name)
    };

    let build = struct_construct(
        &value,
        fields,
        &parts.values(Value::Built, &borrow, &binding),
    );
    let clone = struct_construct(
        &template_qualified,
        fields,
        &parts.values(Value::Cloned, &borrow, &binding),
    );
    let default = struct_construct(
        &template_qualified,
        fields,
        &parts.values(Value::Defaulted, &borrow, &binding),
    );
    let converted = struct_construct(
        &template_qualified,
        fields,
        &parts.values(Value::Converted, &borrow, &binding),
    );
    let pattern = destructure(context.type_ident, fields, &bindings);
    let conversion = quote! { let #pattern = value; #converted };

    let trait_impls = trait_impls(context, &ast.generics, parts, build, clone);
    let default_impl = default_impl(context, &ast.generics, parts, default);
    let into_template_impls = into_template_impls(context, &ast.generics, conversion);
    let not_unpin = not_unpin_impl(&ast.generics, context.type_ident, context.zlim_core);

    Ok(quote! {
        #declaration
        #trait_impls
        #default_impl
        #into_template_impls
        #not_unpin
    })
}

/// Emits a construction of a struct's shape, using the ident it is given.
fn struct_construct(path: &TokenStream, fields: &Fields, values: &[TokenStream]) -> TokenStream {
    match fields {
        Fields::Unit => quote!(#path),
        Fields::Named(_) => quote!(#path { #(#values,)* }),
        Fields::Unnamed(_) => quote!(#path ( #(#values,)* )),
    }
}

/// Emits the pattern that moves every field of a struct out of the value being converted.
///
/// The pattern names the type being derived rather than `Self`, because inside `From` the `Self`
/// type is the template — what the fields are converted into, not what they are moved out of. It
/// is written as a bare ident, since a pattern cannot carry generic arguments.
fn destructure(type_ident: &Ident, fields: &Fields, bindings: &[Ident]) -> TokenStream {
    match fields {
        Fields::Unit => quote!(#type_ident),
        Fields::Named(_) => quote!(#type_ident { #(#bindings,)* }),
        Fields::Unnamed(_) => quote!(#type_ident ( #(#bindings,)* )),
    }
}

// -----------------------------------------------------------------------------
// Enums

/// Emits everything an enum derives: the companion enum, and its impls.
///
/// `parts` and `ast` are the ones the caller already resolved and constrained; the per-variant
/// parts are rebuilt here, because each of them drives its own arm.
fn template_for_enum(
    context: &ExpandContext<'_>,
    ast: &DeriveInput,
    parts: &TemplateParts,
) -> syn::Result<TokenStream> {
    let Data::Enum(data) = &ast.data else {
        unreachable!("the caller dispatches on the data");
    };
    let type_ident = context.type_ident;
    let template_ident = &context.template_ident;
    let template_qualified = template_path(context, ast);

    let mut declaration = Vec::with_capacity(data.variants.len());
    let mut builds = Vec::with_capacity(data.variants.len());
    let mut clones = Vec::with_capacity(data.variants.len());
    let mut conversions = Vec::with_capacity(data.variants.len());
    let mut default = None;

    for variant in &data.variants {
        let variant_parts = TemplateParts::for_fields(&variant.fields, context.zlim_core)?;
        let variant_ident = &variant.ident;

        let is_default = variant
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident(DEFAULT_ATTRIBUTE));
        if is_default && default.is_some() {
            return Err(syn::Error::new(
                variant.span(),
                "an enum template can only have one `#[default]` variant",
            ));
        }

        // An enum reaches a field through the binding its arm introduces, and the arm is the same
        // for both sides of the conversion: matching the template borrows each binding, matching
        // the type being converted moves it. `build_template` and `clone_template` work on the
        // template, `From` on the type.
        let bindings = variant_parts.bindings();
        let access = |position: usize| -> TokenStream {
            let name = &bindings[position];
            quote!(#name)
        };
        // The arm matches the template and produces `Self::Output`: the type being derived for the
        // build, and the template itself for the clone, which is what `clone_template` returns.
        let built = variant_construct(
            type_ident,
            variant_ident,
            &variant.fields,
            &variant_parts.values(Value::Built, &access, &access),
        );
        let cloned = variant_construct(
            template_ident,
            variant_ident,
            &variant.fields,
            &variant_parts.values(Value::Cloned, &access, &access),
        );
        // The conversion produces the template, which outside the template's own impls has to be
        // named with its generics.
        let converted = variant_construct_qualified(
            &template_qualified,
            variant_ident,
            &variant.fields,
            &variant_parts.values(Value::Converted, &access, &access),
        );

        let pattern = variant_pattern(variant);
        declaration.push(variant_declaration(
            variant_ident,
            &variant.fields,
            &variant_parts,
        ));
        // The patterns are qualified by the type they match, and the arms construct the type each
        // impl produces.
        builds.push(quote! { #template_ident::#pattern => #built });
        clones.push(quote! { #template_ident::#pattern => #cloned });
        conversions.push(quote! { #type_ident::#pattern => #converted });

        if is_default {
            let defaults = variant_parts.values(Value::Defaulted, &access, &access);
            default = Some(variant_construct(
                template_ident,
                variant_ident,
                &variant.fields,
                &defaults,
            ));
        }
    }

    let Some(default) = default else {
        return Err(syn::Error::new_spanned(
            type_ident,
            "an enum template needs a variant marked with `#[default]`",
        ));
    };

    let build = quote! {
        match self {
            #(#builds,)*
        }
    };
    let clone = quote! {
        match self {
            #(#clones,)*
        }
    };
    let conversion = quote! {
        match value {
            #(#conversions,)*
        }
    };

    let trait_impls = trait_impls(context, &ast.generics, parts, build, clone);
    let default_impl = default_impl(context, &ast.generics, parts, default);
    let into_template_impls = into_template_impls(context, &ast.generics, conversion);
    let not_unpin = not_unpin_impl(&ast.generics, context.type_ident, context.zlim_core);
    let declaration = template_enum_declaration(
        &context.template_ident,
        &ast.vis,
        &ast.generics,
        &declaration,
    );

    Ok(quote! {
        #declaration

        #trait_impls
        #default_impl
        #into_template_impls
        #not_unpin
    })
}

/// Emits one variant of the companion enum.
fn variant_declaration(
    variant_ident: &Ident,
    fields: &Fields,
    parts: &TemplateParts,
) -> TokenStream {
    let declarations = parts.declarations();
    match fields {
        Fields::Unit => quote!(#variant_ident),
        Fields::Named(_) => quote!(#variant_ident { #(#declarations,)* }),
        Fields::Unnamed(_) => quote!(#variant_ident ( #(#declarations,)* )),
    }
}

/// Emits one construction of an enum variant.
///
/// `which` is the ident of the enum the construction names: the template for the impls that build
/// and clone it, and for the conversion — which produces the template — while the patterns match
/// the type each impl works on.
fn variant_construct(
    which: &Ident,
    variant_ident: &Ident,
    fields: &Fields,
    values: &[TokenStream],
) -> TokenStream {
    match fields {
        Fields::Unit => quote!(#which::#variant_ident),
        Fields::Named(_) => quote!(#which::#variant_ident { #(#values,)* }),
        Fields::Unnamed(_) => quote!(#which::#variant_ident ( #(#values,)* )),
    }
}

/// Same as [`variant_construct`], for a construction that has to name the template with its
/// generics, which is what the conversion needs outside the template's own impls.
fn variant_construct_qualified(
    which: &TokenStream,
    variant_ident: &Ident,
    fields: &Fields,
    values: &[TokenStream],
) -> TokenStream {
    match fields {
        Fields::Unit => quote!(#which::#variant_ident),
        Fields::Named(_) => quote!(#which::#variant_ident { #(#values,)* }),
        Fields::Unnamed(_) => quote!(#which::#variant_ident ( #(#values,)* )),
    }
}

/// The pattern matching `variant`, binding each of its fields.
fn variant_pattern(variant: &syn::Variant) -> Pattern {
    let variant_ident = &variant.ident;
    let fields: Vec<Ident> = variant
        .fields
        .iter()
        .enumerate()
        .map(|(position, field)| field_binding(field, position))
        .collect();

    match &variant.fields {
        Fields::Unit => Pattern::Unit(quote!(#variant_ident)),
        Fields::Named(_) => Pattern::Struct(quote!(#variant_ident { #(#fields,)* })),
        Fields::Unnamed(_) => Pattern::Tuple(quote!(#variant_ident(#(#fields,)*))),
    }
}

/// The pattern that matches one variant of an enum.
enum Pattern {
    /// `Variant`
    Unit(TokenStream),

    /// `Variant(t0, t1)`
    Tuple(TokenStream),

    /// `Variant { a, b }`
    Struct(TokenStream),
}

impl quote::ToTokens for Pattern {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Unit(pattern) | Self::Tuple(pattern) | Self::Struct(pattern) => {
                pattern.to_tokens(tokens)
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Fields

/// Everything the generated impls need to know about the fields of the template.
///
/// Built once per struct, and once per enum variant, and then read three ways: the template's
/// declarations, the values its impls are constructed from, and the bounds those impls need.
struct TemplateParts {
    /// The path of the crate the generated code refers to, resolved once by the caller.
    zlim_core: Path,

    /// One per field.
    choices: Vec<FieldChoice>,

    /// One per field: the declaration of that field in the template.
    declarations: Vec<TokenStream>,

    /// One per field: the type of that field in the template.
    template_types: Vec<TokenStream>,
}

/// What one field contributes, without borrowing the field it came from.
struct FieldChoice {
    /// The field's name, when it has one.
    name: Option<Ident>,

    /// The type of the field in the type being derived, which is what its bound is written on.
    field_type: syn::Type,

    /// Which template describes it.
    choice: TemplateChoice,
}

impl TemplateParts {
    /// An empty set of parts, reading the crate path from `zlim_core`.
    fn new(zlim_core: &Path) -> Self {
        Self {
            zlim_core: zlim_core.clone(),
            choices: Vec::new(),
            declarations: Vec::new(),
            template_types: Vec::new(),
        }
    }

    /// Reads the fields of a struct, or of one enum variant.
    fn for_fields(fields: &Fields, zlim_core: &Path) -> syn::Result<Self> {
        let mut parts = Self::new(zlim_core);

        for field in fields {
            let choice = field_choice(field)?;
            let visibility = match field.vis {
                Visibility::Public(_) => quote!(pub),
                _ => quote!(),
            };

            let template_type = field_template_type(&field.ty, &choice, &parts.zlim_core);
            let declaration = match &field.ident {
                Some(name) => quote!(#visibility #name: #template_type),
                None => quote!(#visibility #template_type),
            };

            parts.choices.push(FieldChoice {
                name: field.ident.clone(),
                field_type: field.ty.clone(),
                choice,
            });
            parts.declarations.push(declaration);
            parts.template_types.push(template_type);
        }

        Ok(parts)
    }

    /// Takes the parts of one variant into the parts of the whole enum.
    fn absorb(&mut self, other: Self) {
        self.choices.extend(other.choices);
        self.declarations.extend(other.declarations);
        self.template_types.extend(other.template_types);
    }

    /// The declarations of every template field.
    fn declarations(&self) -> &[TokenStream] {
        &self.declarations
    }

    /// The name each field is bound by, in the pattern that matches the type.
    fn bindings(&self) -> Vec<Ident> {
        self.choices
            .iter()
            .enumerate()
            .map(|(position, choice)| match &choice.name {
                Some(name) => name.clone(),
                None => field_binding_name(position),
            })
            .collect()
    }

    /// The `where` clause `generics` has, extended so that every template field satisfies `bound`.
    ///
    /// The impls that treat the template as a value — it is built and defaulted — need the
    /// *template* field types to have the corresponding trait, which cannot be deduced from the
    /// bounds on the field types themselves. Writing the predicates on those impls is what keeps
    /// the deriving type from having to declare them.
    fn bounded_by(&self, bound: &TokenStream, generics: &Generics) -> WhereClause {
        // `generics` already carries the bounds the deriving type's fields need, so this only adds
        // what the impls that treat the template as a value need on top of them.
        let mut generics = generics.clone();
        let where_clause = generics.make_where_clause();
        for template in &self.template_types {
            where_clause
                .predicates
                .push(parse_quote! { #template: #bound });
        }
        where_clause.clone()
    }

    /// The value every construction puts in each field.
    ///
    /// `borrow` reaches a field as a reference, for the impls that read the template; `move_` moves
    /// it out, for the conversion.
    fn values(&self, kind: Value, borrow: Accessor<'_>, move_: Accessor<'_>) -> Vec<TokenStream> {
        self.choices
            .iter()
            .enumerate()
            .map(|(position, choice)| choice.value(kind, borrow, move_, position, &self.zlim_core))
            .collect()
    }
}

/// Emits the declaration of the companion template: the same shape as the type, with a field per
/// field, and the parameters and constraints the type has.
///
/// The generics are split rather than interpolated whole, because `Generics` renders its
/// parameters but not its `where` clause.
fn template_declaration(
    template_ident: &Ident,
    visibility: &Visibility,
    generics: &Generics,
    fields: &Fields,
    declarations: &[TokenStream],
) -> TokenStream {
    let (params, _, where_clause) = generics.split_for_impl();
    match fields {
        Fields::Unit => quote!(#visibility struct #template_ident;),
        Fields::Named(_) => quote! {
            #visibility struct #template_ident #params #where_clause {
                #(#declarations,)*
            }
        },
        Fields::Unnamed(_) => quote! {
            #visibility struct #template_ident #params (
                #(#declarations,)*
            ) #where_clause;
        },
    }
}

/// Emits the declaration of the companion enum, with the variants of the type.
fn template_enum_declaration(
    template_ident: &Ident,
    visibility: &Visibility,
    generics: &Generics,
    variants: &[TokenStream],
) -> TokenStream {
    let (params, _, where_clause) = generics.split_for_impl();
    quote! {
        #visibility enum #template_ident #params #where_clause {
            #(#variants,)*
        }
    }
}

/// Which value one construction in the emitted code puts in a field.
#[derive(Clone, Copy)]
enum Value {
    /// The field of the type being derived, built out of its template.
    Built,

    /// The field of the template, cloned.
    Cloned,

    /// The field of the template, converted from the field of the type.
    Converted,

    /// The field of the template, defaulted.
    Defaulted,
}

/// How a field is reached, which is where the two shapes differ.
///
/// A struct reaches a field through `self` — `self.name` or `self.0` — while an enum variant binds
/// its fields directly, so the accessor is the binding that arm introduced. Both are a function of
/// the field's position, which is all the emitters need to know.
type Accessor<'a> = &'a dyn Fn(usize) -> TokenStream;

impl FieldChoice {
    /// The value one construction puts in this field, and the name it is written under.
    ///
    /// `borrow` reaches the field as a reference and `move` moves it out, because the two callers
    /// see different things: the impls that read the template hold a field, while the conversion
    /// owns the value. A struct needs the two to differ — `self.field` against `self.field`, which
    /// the emitters write — while an enum's arm binds the same name for both.
    fn value(
        &self,
        kind: Value,
        borrow: Accessor<'_>,
        move_: Accessor<'_>,
        position: usize,
        zlim_core: &Path,
    ) -> TokenStream {
        let value = match kind {
            Value::Built => {
                // Called through the trait rather than as a method, because a method call would
                // auto-deref an enum arm's binding and pass the template by value instead of by
                // reference.
                let field = borrow(position);
                let template_ = path::template_(zlim_core);
                quote!(#template_::build_template(#field, context)?)
            }
            Value::Cloned => {
                let field = borrow(position);
                let template_ = path::template_(zlim_core);
                quote!(#template_::clone_template(#field))
            }
            Value::Converted => self.conversion(&move_(position), zlim_core),
            Value::Defaulted => quote!(::core::default::Default::default()),
        };

        match &self.name {
            Some(name) => quote!(#name: #value),
            None => value,
        }
    }

    /// The expression that describes this field with a template.
    fn conversion(&self, access: &TokenStream, zlim_core: &Path) -> TokenStream {
        match &self.choice {
            TemplateChoice::Canonical => {
                let into_template_ = path::into_template_(zlim_core);
                quote!(#into_template_::into_template(#access))
            }
            TemplateChoice::BuiltIn => {
                let built_in_template_ = path::built_in_template_(zlim_core);
                quote!(#built_in_template_::built_in_template(#access))
            }
            TemplateChoice::Named(_) => quote!(::core::convert::Into::into(#access)),
        }
    }
}

/// The type of one template field.
fn field_template_type(
    field_type: &syn::Type,
    choice: &TemplateChoice,
    zlim_core: &Path,
) -> TokenStream {
    match choice {
        TemplateChoice::Canonical => {
            let into_template_ = path::into_template_(zlim_core);
            quote!(<#field_type as #into_template_>::Template)
        }
        TemplateChoice::BuiltIn => {
            let built_in_template_ = path::built_in_template_(zlim_core);
            quote!(<#field_type as #built_in_template_>::Template)
        }
        TemplateChoice::Named(named) => quote!(#named),
    }
}

/// Reads the `#[template(...)]` attribute of one field.
fn field_choice(field: &syn::Field) -> syn::Result<TemplateChoice> {
    let mut choice = TemplateChoice::Canonical;

    for attr in &field.attrs {
        if !attr.path().is_ident(TEMPLATE_ATTRIBUTE) {
            continue;
        }
        attr.parse_args_with(|stream: ParseStream| {
            let forked = stream.fork();
            if let Ok(option) = forked.parse::<Ident>()
                && option == BUILT_IN_OPTION
            {
                stream.parse::<Ident>()?;
                choice = TemplateChoice::BuiltIn;
                return Ok(());
            }

            match stream.parse::<Path>() {
                Ok(named) => {
                    choice = TemplateChoice::Named(named);
                    Ok(())
                }
                Err(_) => Err(syn::Error::new(
                    attr.span(),
                    "expected `built_in` or the path of a template type",
                )),
            }
        })?;
    }

    Ok(choice)
}

/// The name one field is bound by: its own name when it has one, a positional name otherwise.
fn field_binding(field: &syn::Field, position: usize) -> Ident {
    match &field.ident {
        Some(name) => name.clone(),
        None => field_binding_name(position),
    }
}

/// The positional name a tuple field is bound by.
fn field_binding_name(position: usize) -> Ident {
    format_ident!("t{position}")
}
