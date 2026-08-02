//! Derive macro for the [`Reflect`] trait.
//!
//! Architecture: attrs → data (parse) → gen_* (codegen).

mod attrs;
mod data;
mod meta;

mod gen_enum;
mod gen_opaque;
mod gen_reflect;
mod gen_register;
mod gen_struct;
mod gen_tuple;
mod gen_typed;
mod gen_typedb;

use data::ReflectDerive;

use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

/// Entry point for `#[derive(Reflect)]`.
pub(crate) fn expand_reflect(input: &DeriveInput) -> TokenStream {
    let zlim_reflect = crate::path::zlim_reflect_path();

    let type_path = crate::type_path::expand_type_path(input, &zlim_reflect);

    let derive = match ReflectDerive::from_input(input, zlim_reflect) {
        Ok(d) => d,
        Err(e) => return e.into_compile_error(),
    };

    let typed = gen_typed::gen_typed(&derive);
    let reflect = gen_reflect::gen_reflect(&derive);
    let ops_trait = match &derive {
        ReflectDerive::Struct(r) => gen_struct::gen_struct(r),
        ReflectDerive::Tuple(r) => gen_tuple::gen_tuple(r),
        ReflectDerive::UnitStruct(r) => gen_opaque::gen_opaque(r),
        ReflectDerive::Enum(r) => gen_enum::gen_enum(r),
        ReflectDerive::Opaque(_) => TokenStream::new(),
    };
    let typedb = gen_typedb::gen_typedb(&derive);
    let register = gen_register::gen_register(&derive);

    quote! {
        #type_path

        const _: () = {
            #typed
            #reflect
            #ops_trait
            #typedb
            #register
        };
    }
}
