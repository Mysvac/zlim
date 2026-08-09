//! Centralized path management for derive macros.

use proc_macro2::TokenStream;
use quote::quote;
use syn::Path;

// -----------------------------------------------------------------------------
// zlim_reflect_path

#[inline]
pub(crate) fn zlim_reflect_path() -> Path {
    zlim_derive_utils::crate_path("zlim_reflect")
}

// -----------------------------------------------------------------------------
// Macros

macro_rules! def_path_fn {
    ($name:ident, $($seg:ident)::+) => {
        #[inline]
        pub(crate) fn $name(zr: &Path) -> TokenStream {
            quote!( #zr :: $($seg)::+ )
        }
    };
}

// -----------------------------------------------------------------------------
// Path

def_path_fn!(type_path_trait, path::TypePath);
def_path_fn!(path_cell, path::PathCell);
def_path_fn!(concat_fn, path::concat);

// -----------------------------------------------------------------------------
// Info

def_path_fn!(attributes, info::Attributes);
def_path_fn!(typed_trait, info::Typed);
def_path_fn!(type_info, info::TypeInfo);
def_path_fn!(info_cell, info::InfoCell);
def_path_fn!(struct_info, info::StructInfo);
def_path_fn!(tuple_info, info::TupleInfo);
def_path_fn!(enum_info, info::EnumInfo);
def_path_fn!(opaque_info, info::OpaqueInfo);
def_path_fn!(variant_info, info::VariantInfo);
def_path_fn!(unit_variant_info, info::UnitVariantInfo);
def_path_fn!(struct_variant_info, info::StructVariantInfo);
def_path_fn!(tuple_variant_info, info::TupleVariantInfo);
def_path_fn!(named_field, info::NamedField);
def_path_fn!(unnamed_field, info::UnnamedField);
def_path_fn!(generics, info::Generics);
def_path_fn!(generic_info, info::GenericInfo);
def_path_fn!(type_param_info, info::TypeParamInfo);
def_path_fn!(const_param_info, info::ConstParamInfo);
def_path_fn!(const_param, info::ConstParam);
def_path_fn!(variant_kind, info::VariantKind);
def_path_fn!(reflect_kind, info::ReflectKind);

// -----------------------------------------------------------------------------
// Ops

def_path_fn!(reflect_trait, ops::Reflect);
def_path_fn!(tuple_trait, ops::Tuple);
def_path_fn!(struct_trait, ops::Struct);
def_path_fn!(opaque_trait, ops::Opaque);
def_path_fn!(enum_trait, ops::Enum);
def_path_fn!(tuple_field_iter, ops::TupleFieldIter);
def_path_fn!(struct_field_iter, ops::StructFieldIter);
def_path_fn!(variant_field_iter, ops::VariantFieldIter);
def_path_fn!(reflect_ref, ops::ReflectRef);
def_path_fn!(reflect_mut, ops::ReflectMut);
def_path_fn!(reflect_owned, ops::ReflectOwned);
def_path_fn!(clone_error, ops::CloneError);
def_path_fn!(apply_error, ops::ApplyError);

// -----------------------------------------------------------------------------
// DB

def_path_fn!(type_db, db::TypeDB);
def_path_fn!(type_database_trait, db::TypeDatabase);

// -----------------------------------------------------------------------------
// impls

def_path_fn!(reflect_hasher, impls::reflect_hasher);
def_path_fn!(reflect_clone_field, impls::reflect_clone_field);
def_path_fn!(opaque_apply, impls::opaque_apply);
def_path_fn!(struct_apply, impls::struct_apply);
def_path_fn!(tuple_apply, impls::tuple_apply);
def_path_fn!(enum_try_apply, impls::enum_try_apply);
def_path_fn!(is_convertable, impls::is_convertable);

// -----------------------------------------------------------------------------
