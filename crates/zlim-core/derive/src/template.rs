//! The `#[derive(FromTemplate)]` macro.
//!
//! The derive generates the canonical template of a type: a companion type named
//! `<Type>Template`, whose fields are the templates of the fields of the original type, together
//! with the association between the two.
//!
//! It also makes the type itself not `Unpin`, which is what keeps the generated `impl
//! FromTemplate` from overlapping with the blanket implementation that every `Clone + Default`
//! type gets.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::ParseStream;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
    Data, DeriveInput, Fields, FieldsUnnamed, Generics, Ident, Index, Path, Token, Visibility,
    WhereClause, parse_quote,
};

use crate::path;

// -----------------------------------------------------------------------------
// Attributes

/// The field attribute that picks the template of that field.
const TEMPLATE_ATTRIBUTE: &str = "template";

/// The variant attribute that picks the variant an enum template defaults to.
const DEFAULT_ATTRIBUTE: &str = "default";

/// The `#[template(built_in)]` option, which uses the built-in template of the field type.
const BUILT_IN_OPTION: &str = "built_in";

// -----------------------------------------------------------------------------
// Expand

/// Expands `#[derive(FromTemplate)]` into the companion template type and its impls.
pub(crate) fn expand(ast: DeriveInput) -> TokenStream {
    let zlim_core = path::zlim_core_path();
    let from_template_ = path::from_template_(&zlim_core);
    let specialize_from_template_ = path::specialize_from_template_(&zlim_core);

    let type_ident = &ast.ident;
    let (impl_generics, type_generics, where_clause) = ast.generics.split_for_impl();
    let template_ident = format_ident!("{type_ident}Template");

    let template = match &ast.data {
        Data::Struct(data) => {
            let fields = match struct_impl(&data.fields, &zlim_core, false) {
                Ok(fields) => fields,
                Err(error) => return error.into_compile_error(),
            };
            template_for_struct(
                &data.fields,
                &fields,
                &template_ident,
                type_ident,
                &ast.vis,
                &ast.generics,
            )
        }
        Data::Enum(data) => {
            let mut variants = Vec::with_capacity(data.variants.len());
            let mut builds = Vec::with_capacity(data.variants.len());
            let mut clones = Vec::with_capacity(data.variants.len());
            let mut default = None;

            for variant in &data.variants {
                let fields = match struct_impl(&variant.fields, &zlim_core, true) {
                    Ok(fields) => fields,
                    Err(error) => return error.into_compile_error(),
                };
                let is_default = variant
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident(DEFAULT_ATTRIBUTE));
                if is_default && default.is_some() {
                    return syn::Error::new(
                        variant.span(),
                        "an enum template can only have one `#[default]` variant",
                    )
                    .into_compile_error();
                }

                let FieldsImpl {
                    template_fields,
                    builds: field_builds,
                    defaults: field_defaults,
                    clones: field_clones,
                } = fields;
                let variant_ident = &variant.ident;

                match &variant.fields {
                    Fields::Named(named) => {
                        variants.push(quote! {
                            #variant_ident { #(#template_fields,)* }
                        });
                        let idents = named.named.iter().map(|field| &field.ident);
                        builds.push(quote! {
                            #template_ident::#variant_ident { #(#idents,)* } => {
                                #type_ident::#variant_ident { #(#field_builds,)* }
                            }
                        });
                        let idents = named.named.iter().map(|field| &field.ident);
                        clones.push(quote! {
                            #template_ident::#variant_ident { #(#idents,)* } => {
                                #template_ident::#variant_ident { #(#field_clones,)* }
                            }
                        });
                        if is_default {
                            default = Some(quote! {
                                Self::#variant_ident { #(#field_defaults,)* }
                            });
                        }
                    }
                    Fields::Unnamed(FieldsUnnamed { unnamed, .. }) => {
                        let idents: Vec<Ident> = (0..unnamed.len())
                            .map(|position| format_ident!("t{position}"))
                            .collect();
                        variants.push(quote! {
                            #variant_ident(#(#template_fields,)*)
                        });
                        builds.push(quote! {
                            #template_ident::#variant_ident(#(#idents,)*) => {
                                #type_ident::#variant_ident(#(#field_builds,)*)
                            }
                        });
                        clones.push(quote! {
                            #template_ident::#variant_ident(#(#idents,)*) => {
                                #template_ident::#variant_ident(#(#field_clones,)*)
                            }
                        });
                        if is_default {
                            default = Some(quote! {
                                Self::#variant_ident(#(#field_defaults,)*)
                            });
                        }
                    }
                    Fields::Unit => {
                        variants.push(quote! { #variant_ident });
                        builds.push(quote! {
                            #template_ident::#variant_ident => #type_ident::#variant_ident
                        });
                        clones.push(quote! {
                            #template_ident::#variant_ident => #template_ident::#variant_ident
                        });
                        if is_default {
                            default = Some(quote! { Self::#variant_ident });
                        }
                    }
                }
            }

            let Some(default) = default else {
                return syn::Error::new_spanned(
                    type_ident,
                    "an enum template needs a variant marked with `#[default]`",
                )
                .into_compile_error();
            };

            let template_ = path::template_(&zlim_core);
            let template_context_ = path::template_context_(&zlim_core);
            let zlim_result_ = path::zlim_result_(&zlim_core);
            let visibility = &ast.vis;

            quote! {
                #visibility enum #template_ident #impl_generics #where_clause {
                    #(#variants,)*
                }

                impl #impl_generics #template_ for #template_ident #type_generics #where_clause {
                    type Output = #type_ident #type_generics;

                    fn build_template(
                        &self,
                        context: &mut #template_context_,
                    ) -> #zlim_result_<Self::Output> {
                        ::core::result::Result::Ok(match self {
                            #(#builds,)*
                        })
                    }

                    fn clone_template(&self) -> Self {
                        match self {
                            #(#clones,)*
                        }
                    }
                }

                impl #impl_generics ::core::default::Default
                    for #template_ident #type_generics #where_clause
                {
                    fn default() -> Self {
                        #default
                    }
                }
            }
        }
        Data::Union(_) => {
            return syn::Error::new_spanned(&ast.ident, "union types have no template")
                .into_compile_error();
        }
    };

    // The type itself is made not `Unpin`: the condition never holds, which is what keeps the
    // `FromTemplate` impl below out of the blanket implementation for `Clone + Default` types.
    let mut unpin_where_clause = where_clause.cloned().unwrap_or_else(|| WhereClause {
        where_token: <Token![where]>::default(),
        predicates: Punctuated::new(),
    });
    unpin_where_clause
        .predicates
        .push(parse_quote! { for<'a> [()]: #specialize_from_template_ });

    quote! {
        impl #impl_generics #from_template_ for #type_ident #type_generics #where_clause {
            type Template = #template_ident #type_generics;
        }

        impl #impl_generics ::core::marker::Unpin for #type_ident #type_generics #unpin_where_clause {}

        #template
    }
}

// -----------------------------------------------------------------------------
// Structs

/// Emits the template of a struct, in the shape the struct itself has.
fn template_for_struct(
    shape: &Fields,
    fields: &FieldsImpl,
    template_ident: &Ident,
    type_ident: &Ident,
    visibility: &Visibility,
    generics: &Generics,
) -> TokenStream {
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let zlim_core = path::zlim_core_path();
    let template_ = path::template_(&zlim_core);
    let template_context_ = path::template_context_(&zlim_core);
    let zlim_result_ = path::zlim_result_(&zlim_core);

    let FieldsImpl {
        template_fields,
        builds,
        defaults,
        clones,
    } = fields;

    let (definition, build, clone, default) = match shape {
        Fields::Named(_) => (
            quote! {
                #visibility struct #template_ident #impl_generics #where_clause {
                    #(#template_fields,)*
                }
            },
            quote! { #type_ident { #(#builds,)* } },
            quote! { Self { #(#clones,)* } },
            quote! { Self { #(#defaults,)* } },
        ),
        Fields::Unnamed(_) => (
            quote! {
                #visibility struct #template_ident #impl_generics (
                    #(#template_fields,)*
                ) #where_clause;
            },
            quote! { #type_ident (#(#builds,)*) },
            quote! { Self (#(#clones,)*) },
            quote! { Self (#(#defaults,)*) },
        ),
        Fields::Unit => (
            quote! {
                #visibility struct #template_ident;
            },
            quote! { #type_ident },
            quote! { Self },
            quote! { Self },
        ),
    };

    quote! {
        #definition

        impl #impl_generics #template_ for #template_ident #type_generics #where_clause {
            type Output = #type_ident #type_generics;

            fn build_template(&self, context: &mut #template_context_) -> #zlim_result_<Self::Output> {
                ::core::result::Result::Ok(#build)
            }

            fn clone_template(&self) -> Self {
                #clone
            }
        }

        impl #impl_generics ::core::default::Default for #template_ident #type_generics #where_clause {
            fn default() -> Self {
                #default
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Fields

/// What the generated template of a struct, or of one enum variant, needs.
struct FieldsImpl {
    /// The declaration of each template field.
    template_fields: Vec<TokenStream>,
    /// The expression that builds each field out of its template.
    builds: Vec<TokenStream>,
    /// The expression that defaults each template field.
    defaults: Vec<TokenStream>,
    /// The expression that clones each template field.
    clones: Vec<TokenStream>,
}

/// Picks the template type of each field, and emits what the template needs for it.
///
/// `is_enum` only changes how the fields are reached: the fields of an enum variant are bound by
/// the pattern that matches it, while the fields of a struct are reached through `self`.
fn struct_impl(fields: &Fields, zlim_core: &Path, is_enum: bool) -> syn::Result<FieldsImpl> {
    let built_in_template_ = path::built_in_template_(zlim_core);
    let from_template_ = path::from_template_(zlim_core);
    let template_ = path::template_(zlim_core);

    let named = matches!(fields, Fields::Named(_));
    let mut fields_impl = FieldsImpl {
        template_fields: Vec::with_capacity(fields.len()),
        builds: Vec::with_capacity(fields.len()),
        defaults: Vec::with_capacity(fields.len()),
        clones: Vec::with_capacity(fields.len()),
    };

    for (position, field) in fields.iter().enumerate() {
        let visibility = match field.vis {
            Visibility::Public(_) => quote!(pub),
            _ => quote!(),
        };
        let ident = &field.ident;
        let ty = &field.ty;
        let index = Index::from(position);

        // The template of a field is the template of its type, unless `#[template(...)]` says
        // otherwise.
        let mut template_type = None;
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
                    template_type = Some(quote!(<#ty as #built_in_template_>::Template));
                    return Ok(());
                }

                match stream.parse::<Path>() {
                    Ok(path) => {
                        template_type = Some(quote!(#path));
                        Ok(())
                    }
                    Err(_) => Err(syn::Error::new(
                        attr.span(),
                        "expected `built_in` or the path of a template type",
                    )),
                }
            })?;
        }
        let template_type =
            template_type.unwrap_or_else(|| quote!(<#ty as #from_template_>::Template));

        if named {
            let ident_name = ident.as_ref().expect("a named field has a name");
            fields_impl.template_fields.push(quote! {
                #visibility #ident_name: #template_type
            });
            if is_enum {
                fields_impl
                    .builds
                    .push(quote! { #ident_name: #ident_name.build_template(context)? });
                fields_impl
                    .clones
                    .push(quote! { #ident_name: #template_::clone_template(#ident_name) });
            } else {
                fields_impl
                    .builds
                    .push(quote! { #ident_name: self.#ident_name.build_template(context)? });
                fields_impl
                    .clones
                    .push(quote! { #ident_name: #template_::clone_template(&self.#ident_name) });
            }
            fields_impl
                .defaults
                .push(quote! { #ident_name: ::core::default::Default::default() });
        } else {
            fields_impl
                .template_fields
                .push(quote! { #visibility #template_type });
            if is_enum {
                let binding = format_ident!("t{position}");
                fields_impl
                    .builds
                    .push(quote! { #binding.build_template(context)? });
                fields_impl
                    .clones
                    .push(quote! { #template_::clone_template(#binding) });
            } else {
                fields_impl
                    .builds
                    .push(quote! { self.#index.build_template(context)? });
                fields_impl
                    .clones
                    .push(quote! { #template_::clone_template(&self.#index) });
            }
            fields_impl
                .defaults
                .push(quote! { ::core::default::Default::default() });
        }
    }

    Ok(fields_impl)
}
