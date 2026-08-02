//! Intermediate representation (IR) for the Reflect derive macro.

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Comma;
use syn::{DeriveInput, Field, Fields, Ident, Variant};

use super::attrs::{FieldAttrs, TypeAttrs};
use super::meta::ReflectMeta;

// ----------------------------------------------------------------------------
// StructField

pub(crate) struct StructField<'a> {
    pub data: &'a Field,
    pub attrs: FieldAttrs,
    pub field_index: usize,
    pub reflect_index: usize,
}

impl<'a> StructField<'a> {
    #[inline]
    pub fn is_ignore(&self) -> bool {
        self.attrs.is_ignored
    }

    #[inline]
    pub fn cloneable(&self) -> bool {
        self.attrs.has_clone
    }

    #[inline]
    pub fn defaultable(&self) -> bool {
        self.attrs.has_default
    }

    pub fn ty(&self) -> &syn::Type {
        &self.data.ty
    }

    /// Get the field name for the `field` function of `Struct/TupleStruct`.
    ///
    /// - Named fields return values similar to `"name"`.
    /// - Unnamedfields return values similar to `2`.
    pub fn reflect_accessor(&self) -> TokenStream {
        match &self.data.ident {
            Some(ident) => ident.to_string().to_token_stream(),
            None => self.reflect_index.to_token_stream(),
        }
    }

    /// Generates a [`syn::Member`] based on this field.
    ///
    /// If the field is unnamed, the declaration index is used.
    /// This allows this member to be used for both active and ignored fields.
    pub fn to_member(&self) -> syn::Member {
        match &self.data.ident {
            Some(ident) => syn::Member::Named(ident.clone()),
            None => syn::Member::Unnamed(self.field_index.into()),
        }
    }

    /// Generates a `TokenStream` for `NamedField` or `UnnamedField` construction.
    ///
    /// This function is only allowed to be called for active fields(self.reflection_index is some).
    pub fn to_info_tokens(&self, zlim_reflect_path: &syn::Path) -> TokenStream {
        let field_info = if self.data.ident.is_some() {
            crate::path::named_field(zlim_reflect_path) // String Literal
        } else {
            crate::path::unnamed_field(zlim_reflect_path) // Num Literal
        };

        let name: TokenStream = self.reflect_accessor();

        let ty = &self.data.ty;

        let with_attributes = self.with_attributes_expression(zlim_reflect_path);
        let with_docs = self.with_docs_expression();

        quote! {
            #field_info::new::<#ty>(#name)
                #with_attributes
                #with_docs
        }
    }

    /// Generate docs codes
    pub fn with_docs_expression(&self) -> TokenStream {
        match self.attrs.doc_string() {
            Some(docs) => quote!( .with_docs(::core::option::Option::Some( #docs )) ),
            None => TokenStream::new(),
        }
    }

    /// Generate attributes codes
    pub fn with_attributes_expression(&self, zlim_reflect_path: &syn::Path) -> TokenStream {
        if self.attrs.custom_attrs.is_empty() {
            return TokenStream::new();
        }

        let attrs = &self.attrs.custom_attrs;
        let with_attrs = attrs.iter().map(|value| quote!(.with(#value)));
        let attributes = crate::path::attributes(zlim_reflect_path);

        quote! {
            .with_attributes(#attributes::builder() #(#with_attrs)* .finish())
        }
    }
}

// ----------------------------------------------------------------------------
// ReflectStruct

pub(crate) struct StructFieldAccessors {
    /// The referenced field accessors, such as `&self.foo`.
    pub fields_ref: Vec<TokenStream>,
    /// The mutably referenced field accessors, such as `&mut self.foo`.
    pub fields_mut: Vec<TokenStream>,
    /// The ordered set of field indices (basically just the range of [0, `field_count`).
    pub field_indices: Vec<usize>,
    /// The number of fields in the reflected struct.
    pub field_count: usize,
}

pub(crate) struct ReflectStruct<'a> {
    meta: ReflectMeta<'a>,
    fields: Vec<StructField<'a>>,
}

impl<'a> ReflectStruct<'a> {
    fn new(mut meta: ReflectMeta<'a>, fields: Vec<StructField<'a>>) -> Self {
        meta.active_types.reserve(fields.len() << 1);
        for field in fields.iter() {
            if !field.attrs.is_ignored {
                meta.active_types.insert(&field.data.ty);
            }
        }
        Self { meta, fields }
    }

    pub fn meta(&self) -> &ReflectMeta<'a> {
        &self.meta
    }

    pub fn fields(&self) -> &[StructField<'a>] {
        &self.fields
    }

    pub fn active_fields(&self) -> impl Iterator<Item = &StructField<'a>> {
        self.fields.iter().filter(|f| !f.attrs.is_ignored)
    }

    pub fn field_accessors(&self) -> StructFieldAccessors {
        let (fields_ref, fields_mut): (Vec<_>, Vec<_>) = self
            .active_fields()
            .map(|field| {
                let member = field.to_member();
                (quote!(&self.#member), quote!(&mut self.#member))
            })
            .unzip();

        let field_count = fields_ref.len();
        let field_indices = (0..field_count).collect();

        StructFieldAccessors {
            fields_ref,
            fields_mut,
            field_indices,
            field_count,
        }
    }

    pub fn type_info_tokens(&self, is_tuple: bool) -> TokenStream {
        let zlim_reflect_path = self.meta.zlim_reflect();

        let type_info_path = crate::path::type_info(zlim_reflect_path);

        let type_info_kind = if is_tuple {
            Ident::new("Tuple", Span::call_site())
        } else {
            Ident::new("Struct", Span::call_site())
        };

        let info_struct_path = if is_tuple {
            crate::path::tuple_info(zlim_reflect_path)
        } else {
            crate::path::struct_info(zlim_reflect_path)
        };

        let field_infos = self
            .active_fields()
            .map(|field| field.to_info_tokens(zlim_reflect_path));

        let with_attributes = self.meta.with_attributes_expression();
        let with_generics = self.meta.with_generics_expression();
        let with_docs = self.meta.with_docs_expression();

        quote! {
            #type_info_path::#type_info_kind(
                #info_struct_path::new::<Self>(&[ #(#field_infos),* ])
                    #with_attributes
                    #with_generics
                    #with_docs
            )
        }
    }
}

// ----------------------------------------------------------------------------
// EnumVariantFields & EnumVariant

pub(crate) enum EnumVariantFields<'a> {
    Unit,
    Named(Vec<StructField<'a>>),
    Unnamed(Vec<StructField<'a>>),
}

pub(crate) struct EnumVariant<'a> {
    pub data: &'a Variant,
    pub fields: EnumVariantFields<'a>,
    pub attrs: FieldAttrs,
}

impl<'a> EnumVariant<'a> {
    /// The complete set of fields in this variant.
    pub fn fields(&self) -> &[StructField<'a>] {
        use EnumVariantFields::{Named, Unnamed};
        match &self.fields {
            Named(fields) | Unnamed(fields) => fields,
            EnumVariantFields::Unit => &[],
        }
    }

    /// Get an iterator of fields which are exposed to the reflection API
    pub fn active_fields(&self) -> impl Iterator<Item = &StructField<'a>> {
        self.fields().iter().filter(|f| !f.attrs.is_ignored)
    }

    /// Generates a `TokenStream` for `VariantInfo` construction.
    pub fn variant_info_tokens(&self, zlim_reflect_path: &syn::Path) -> TokenStream {
        let variant_info_path = crate::path::variant_info(zlim_reflect_path);

        let variant_info_kind = match &self.fields {
            EnumVariantFields::Named(_) => Ident::new("Struct", Span::call_site()),
            EnumVariantFields::Unnamed(_) => Ident::new("Tuple", Span::call_site()),
            EnumVariantFields::Unit => Ident::new("Unit", Span::call_site()),
        };

        let info_struct_path = match &self.fields {
            EnumVariantFields::Named(_) => crate::path::struct_variant_info(zlim_reflect_path),
            EnumVariantFields::Unnamed(_) => crate::path::tuple_variant_info(zlim_reflect_path),
            EnumVariantFields::Unit => crate::path::unit_variant_info(zlim_reflect_path),
        };

        let fields = self
            .active_fields()
            .map(|field| field.to_info_tokens(zlim_reflect_path));

        let variant_name = &self.data.ident.to_string();
        let args = match &self.fields {
            EnumVariantFields::Unit => quote!( #variant_name ),
            _ => quote!( #variant_name , &[ #(#fields),* ] ),
        };

        let with_attributes = self.with_attributes_expression(zlim_reflect_path);
        let with_docs = self.with_docs_expression();

        quote! {
            #variant_info_path::#variant_info_kind(
                #info_struct_path::new( #args )
                    #with_attributes
                    #with_docs
            )
        }
    }

    /// Generate docs codes
    ///
    /// If `docs` is empty, this function will return an empty token stream.
    ///
    /// Otherwise, it will return content similar to this:
    ///
    /// ```ignore
    /// .with_docs(::core::option::Option::Some("......"))
    /// ```
    pub fn with_docs_expression(&self) -> TokenStream {
        match self.attrs.doc_string() {
            Some(docs) => quote!( .with_docs(::core::option::Option::Some( #docs )) ),
            None => TokenStream::new(),
        }
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
    pub fn with_attributes_expression(&self, zlim_reflect_path: &syn::Path) -> TokenStream {
        if self.attrs.custom_attrs.is_empty() {
            return TokenStream::new();
        }

        let attrs = &self.attrs.custom_attrs;
        let with_attrs = attrs.iter().map(|value| quote!(.with(#value)));
        let attributes = crate::path::attributes(zlim_reflect_path);

        quote! {
            .with_attributes(#attributes::builder() #(#with_attrs)* .finish())
        }
    }
}

// ----------------------------------------------------------------------------
// ReflectEnum

pub(crate) struct ReflectEnum<'a> {
    pub meta: ReflectMeta<'a>,
    pub variants: Vec<EnumVariant<'a>>,
}

impl<'a> ReflectEnum<'a> {
    fn new(mut meta: ReflectMeta<'a>, variants: Vec<EnumVariant<'a>>) -> Self {
        meta.active_types.reserve(variants.len() << 1);

        for variant in variants.iter() {
            for field in variant.fields() {
                if !field.attrs.is_ignored {
                    meta.active_types.insert(&field.data.ty);
                }
            }
        }

        Self { meta, variants }
    }

    pub fn meta(&self) -> &ReflectMeta<'a> {
        &self.meta
    }

    pub fn variants(&self) -> &[EnumVariant<'a>] {
        &self.variants
    }

    pub fn active_fields(&self) -> impl Iterator<Item = &StructField<'a>> {
        self.variants.iter().flat_map(EnumVariant::active_fields)
    }

    pub fn type_info_tokens(&self) -> TokenStream {
        let zlim_reflect_path = self.meta.zlim_reflect();

        let type_info_path = crate::path::type_info(zlim_reflect_path);
        let info_struct_path = crate::path::enum_info(zlim_reflect_path);

        let variant_infos = self
            .variants
            .iter()
            .map(|variant| variant.variant_info_tokens(zlim_reflect_path));

        let with_attributes = self.meta.with_attributes_expression();
        let with_docs = self.meta.with_docs_expression();
        let with_generics = self.meta.with_generics_expression();

        quote! {
            #type_info_path::Enum(
                #info_struct_path::new::<Self>(&[ #(#variant_infos),* ])
                    #with_attributes
                    #with_generics
                    #with_docs
            )
        }
    }
}

// ----------------------------------------------------------------------------
// ReflectDerive

pub(crate) enum ReflectDerive<'a> {
    Struct(ReflectStruct<'a>),
    Tuple(ReflectStruct<'a>),
    UnitStruct(ReflectMeta<'a>),
    Enum(ReflectEnum<'a>),
    Opaque(ReflectMeta<'a>),
}

impl<'a> ReflectDerive<'a> {
    /// Classifies a `DeriveInput` into the appropriate variant.
    ///
    /// # Errors
    ///
    /// Returns an error for unions (unsupported) or empty enums.
    pub fn from_input(input: &'a DeriveInput, zlim_reflect: syn::Path) -> syn::Result<Self> {
        let attrs = TypeAttrs::parse(&input.attrs)?;

        let meta = ReflectMeta::new(&input.ident, &input.generics, attrs, zlim_reflect);

        if meta.attrs().is_opaque {
            return Ok(Self::Opaque(meta));
        }

        match &input.data {
            syn::Data::Struct(data) => {
                let fields = collect_struct_fields(&data.fields)?;
                match data.fields {
                    Fields::Named(_) => Ok(Self::Struct(ReflectStruct::new(meta, fields))),
                    Fields::Unnamed(_) => Ok(Self::Tuple(ReflectStruct::new(meta, fields))),
                    Fields::Unit => Ok(Self::UnitStruct(meta)),
                }
            }
            syn::Data::Enum(data) => {
                if data.variants.is_empty() {
                    ::core::hint::cold_path();
                    let ident = &input.ident;
                    let msg = format!(
                        "`{ident}` is an empty enum; reflection requires at least one variant."
                    );
                    return Err(syn::Error::new_spanned(input, msg));
                }

                let variants = collect_enum_variants(&data.variants)?;
                Ok(Self::Enum(ReflectEnum::new(meta, variants)))
            }
            syn::Data::Union(_) => {
                let ident = &input.ident;
                let msg = format!("`{ident}` is a union; reflection does not support unions.");
                Err(syn::Error::new_spanned(input, msg))
            }
        }
    }

    pub fn meta(&self) -> &ReflectMeta<'a> {
        match self {
            Self::Struct(s) => &s.meta,
            Self::Tuple(s) => &s.meta,
            Self::UnitStruct(m) => m,
            Self::Enum(e) => &e.meta,
            Self::Opaque(m) => m,
        }
    }
}

// ----------------------------------------------------------------------------
// Field / variant collection

const MAX_COUNT: usize = u8::MAX as usize;

fn collect_struct_fields(fields: &Fields) -> syn::Result<Vec<StructField<'_>>> {
    let fields = match fields {
        Fields::Named(f) => &f.named,
        Fields::Unnamed(f) => &f.unnamed,
        Fields::Unit => return Ok(vec![]),
    };

    let len = fields.len();
    if len > MAX_COUNT {
        ::core::hint::cold_path();
        let msg = format!(
            "too many fields ({len}); reflection supports at most {MAX_COUNT} fields per type"
        );
        return Err(syn::Error::new_spanned(fields, msg));
    }

    let mut result = Vec::with_capacity(len);
    let mut reflect_index = 0usize;

    for (field_index, field) in fields.iter().enumerate() {
        let attrs = FieldAttrs::parse(&field.attrs)?;

        let is_ignored = attrs.is_ignored;

        result.push(StructField {
            data: field,
            attrs,
            field_index,
            reflect_index,
        });

        if !is_ignored {
            reflect_index += 1;
        }
    }

    Ok(result)
}

fn collect_enum_variants(
    variants: &Punctuated<Variant, Comma>,
) -> syn::Result<Vec<EnumVariant<'_>>> {
    let len = variants.len();

    if variants.len() > MAX_COUNT {
        ::core::hint::cold_path();
        let msg = format!(
            "too many variants ({len}); reflection supports at most {MAX_COUNT} variants per enum"
        );
        return Err(syn::Error::new(variants.span(), msg));
    }

    let mut result = Vec::with_capacity(len);

    for variant in variants.iter() {
        let fields = collect_struct_fields(&variant.fields)?;

        let variant_fields = match variant.fields {
            Fields::Named(_) => EnumVariantFields::Named(fields),
            Fields::Unnamed(_) => EnumVariantFields::Unnamed(fields),
            Fields::Unit => EnumVariantFields::Unit,
        };

        let attrs = FieldAttrs::parse(&variant.attrs)?;

        if attrs.is_ignored {
            ::core::hint::cold_path();
            let msg = "#[reflect(ignore)] cannot be used for enum variant";
            return Err(syn::Error::new(variant.span(), msg));
        }

        if attrs.has_clone {
            ::core::hint::cold_path();
            let msg = "#[reflect(clone)] cannot be used for enum variant";
            return Err(syn::Error::new(variant.span(), msg));
        }

        if attrs.has_default {
            ::core::hint::cold_path();
            let msg = "#[reflect(default)] cannot be used for enum variant";
            return Err(syn::Error::new(variant.span(), msg));
        }

        result.push(EnumVariant {
            data: variant,
            fields: variant_fields,
            attrs,
        });
    }

    Ok(result)
}
