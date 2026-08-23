//! Implementation for the `job_group!` function-like macro.
//!
//! The macro generates a marker type implementing `JobGroupLabel`:
//!
//! - Job slots accept either string literals (used as-is) or types
//!   implementing `JobLabel` (resolved through
//!   `<Type as JobLabel>::name()`).
//! - `type`, `name`, and `jobs` are required; `condition`, `order`,
//!   and `weak_order` are optional and default to `None` / `&[]`.
//! - The generated `register()` registers the group's type-based jobs and
//!   condition (in list order) before registering the group itself.
//!   String slots carry no type and are skipped.
//! - Generic markers require every type parameter to implement `TypePath`,
//!   and `name()` is derived from the marker type itself.
//! - Non-generic markers register themselves at program startup through
//!   `zlim_reg::submit!` (visible after `JobGroup::collect`); generic
//!   markers must be registered manually per instantiation.

use proc_macro2::TokenStream;
use quote::quote;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Token, parse_quote};

use super::job::parse_type_spec;

// -----------------------------------------------------------------------------
// Inputs

/// One entry of a job slot list: a string literal or a `JobLabel`
/// type.
pub(crate) enum Slot {
    Str(LitStr),
    Ty(Box<syn::Type>),
}

impl Parse for Slot {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(LitStr) {
            Ok(Slot::Str(input.parse()?))
        } else {
            Ok(Slot::Ty(Box::new(input.parse()?)))
        }
    }
}

/// Parses a bracketed, comma-separated slot list: `[slot, slot, ...]`.
fn parse_slot_list(input: ParseStream) -> syn::Result<Vec<Slot>> {
    let content;
    syn::bracketed!(content in input);
    let slots = content.parse_terminated(Slot::parse, Token![,])?;
    Ok(slots.into_iter().collect())
}

/// Parses a bracketed list of slot lists: `[[...], [...], ...]`.
fn parse_slot_list_list(input: ParseStream) -> syn::Result<Vec<Vec<Slot>>> {
    let content;
    syn::bracketed!(content in input);
    let lists = content.parse_terminated(parse_slot_list, Token![,])?;
    Ok(lists.into_iter().collect())
}

/// Parsed `job_group! { type: ..., name: ..., jobs: [...] }` input.
pub(crate) struct JobGroupInput {
    pub type_ident: Ident,
    pub generics: syn::Generics,
    pub name: LitStr,
    pub jobs: Vec<Slot>,
    pub condition: Option<Slot>,
    pub order: Vec<Vec<Slot>>,
    pub weak_order: Vec<Vec<Slot>>,
    pub relaxed_order: Vec<Vec<Slot>>,
}

impl Parse for JobGroupInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut type_spec = None;
        let mut name = None;
        let mut jobs = None;
        let mut condition = None;
        let mut order = None;
        let mut weak_order = None;
        let mut relaxed_order = None;

        while !input.is_empty() {
            let key = input.call(Ident::parse_any)?;
            input.parse::<Token![:]>()?;

            match key.to_string().as_str() {
                "type" => type_spec = Some(parse_type_spec(input)?),
                "name" => name = Some(input.parse()?),
                "jobs" => jobs = Some(parse_slot_list(input)?),
                "condition" => condition = Some(Some(input.parse()?)),
                "order" => order = Some(parse_slot_list_list(input)?),
                "weak_order" => weak_order = Some(parse_slot_list_list(input)?),
                "relaxed_order" => relaxed_order = Some(parse_slot_list_list(input)?),
                _ => {
                    return Err(syn::Error::new_spanned(
                        key,
                        "expected `type`, `name`, `jobs`, `condition`, `order`, \
                         `weak_order`, or `relaxed_order`",
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        let (type_ident, generics) =
            type_spec.ok_or_else(|| syn::Error::new(input.span(), "missing `type` argument"))?;
        let name = name.ok_or_else(|| syn::Error::new(input.span(), "missing `name` argument"))?;
        let jobs = jobs.ok_or_else(|| syn::Error::new(input.span(), "missing `jobs` argument"))?;

        Ok(Self {
            type_ident,
            generics,
            name,
            jobs,
            condition: condition.unwrap_or_default(),
            order: order.unwrap_or_default(),
            weak_order: weak_order.unwrap_or_default(),
            relaxed_order: relaxed_order.unwrap_or_default(),
        })
    }
}

// -----------------------------------------------------------------------------
// Expansion

/// Generates the expression resolving one slot to `&'static str`.
fn slot_expr(slot: &Slot, job_label_: &TokenStream) -> TokenStream {
    match slot {
        Slot::Str(lit) => quote! { #lit },
        Slot::Ty(ty) => quote! { <#ty as #job_label_>::name() },
    }
}

/// Generates the marker struct and its `JobGroupLabel` implementation.
pub(crate) fn expand(input: JobGroupInput) -> TokenStream {
    let zlim_core = crate::path::zlim_core_path();
    let job_label_ = crate::path::job_label_(&zlim_core);
    let job_group_ = crate::path::job_group_(&zlim_core);
    let job_group_label_ = crate::path::job_group_label_(&zlim_core);
    let job_group_reg_ = crate::path::job_group_reg_(&zlim_core);
    let submit_ = crate::path::submit_(&zlim_core);
    let type_path_trait_ = crate::path::type_path_trait_(&zlim_core);
    let type_path_derive_ = crate::path::type_path_derive_(&zlim_core);

    let JobGroupInput {
        type_ident,
        mut generics,
        name,
        jobs,
        condition,
        order,
        weak_order,
        relaxed_order,
    } = input;

    let type_params: Vec<Ident> = generics
        .type_params()
        .map(|param| param.ident.clone())
        .collect();
    let generic = !generics.params.is_empty();

    // Generic markers need `T: TypePath` so the group name can be derived
    // from the marker type itself.
    if generic {
        let where_clause = generics.make_where_clause();
        for param in &type_params {
            where_clause
                .predicates
                .push(parse_quote!(#param: #type_path_trait_));
        }
    }

    let (impl_g, ty_g, where_g) = generics.split_for_impl();

    let struct_item = if generic {
        let phantom = if type_params.is_empty() {
            quote! { ::core::marker::PhantomData<()> }
        } else {
            quote! { ::core::marker::PhantomData<( #(#type_params,)* )> }
        };

        quote! {
            #[derive(#type_path_derive_)]
            #[type_path = #name]
            pub struct #type_ident #ty_g (#phantom) #where_g;
        }
    } else {
        quote! { pub struct #type_ident (()); }
    };

    let name_expr = if generic {
        quote! { <Self as #type_path_trait_>::type_path() }
    } else {
        quote! { #name }
    };

    let job_exprs: Vec<TokenStream> = jobs
        .iter()
        .map(|slot| slot_expr(slot, &job_label_))
        .collect();

    // `register()` calls `JobLabel::register()` on every type-based slot
    // (jobs in list order, then `condition`) before registering the group.
    // String slots cannot be registered.
    let job_registers: Vec<TokenStream> = jobs
        .iter()
        .filter_map(|slot| match slot {
            Slot::Ty(ty) => Some(quote! { <#ty as #job_label_>::register(); }),
            Slot::Str(_) => None,
        })
        .collect();

    let condition_register = match &condition {
        Some(Slot::Ty(ty)) => quote! { <#ty as #job_label_>::register(); },
        _ => TokenStream::new(),
    };

    let condition_expr = match &condition {
        Some(slot) => {
            let expr = slot_expr(slot, &job_label_);
            quote! { ::core::option::Option::Some(#expr) }
        }
        None => quote! { ::core::option::Option::None },
    };

    let chain_exprs = |chains: &[Vec<Slot>]| -> Vec<TokenStream> {
        chains
            .iter()
            .map(|chain| {
                let items: Vec<TokenStream> = chain
                    .iter()
                    .map(|slot| slot_expr(slot, &job_label_))
                    .collect();
                quote! { &[ #(#items),* ] }
            })
            .collect()
    };

    let order_exprs = chain_exprs(&order);
    let weak_order_exprs = chain_exprs(&weak_order);
    let relaxed_order_exprs = chain_exprs(&relaxed_order);

    // Non-generic markers register themselves at program startup.  Generic
    // markers cannot — a CTOR static may not reference generic parameters
    // (E0401) — so they must be registered manually per instantiation.
    let registration = if generic {
        TokenStream::new()
    } else {
        quote! {
            #submit_!(#job_group_reg_::of::<#type_ident>() => #job_group_reg_);
        }
    };

    quote! {
        #struct_item

        const _: () = {
            impl #impl_g #job_group_label_ for #type_ident #ty_g #where_g {
                fn name() -> &'static str {
                    #name_expr
                }

                fn group() -> #job_group_ {
                    #job_group_::build(
                        <Self as #job_group_label_>::name(),
                        &[ #(#job_exprs),* ],
                        #condition_expr,
                        &[ #(#order_exprs),* ],
                        &[ #(#weak_order_exprs),* ],
                        &[ #(#relaxed_order_exprs),* ],
                    )
                }

                fn register() {
                    let name = Self::name();

                    if #job_group_::get(name).is_some() {
                        return;
                    }

                    ::core::hint::cold_path();

                    #(#job_registers)*
                    #condition_register

                    #job_group_::register(Self::group());
                }
            }

            #registration
        };
    }
}
