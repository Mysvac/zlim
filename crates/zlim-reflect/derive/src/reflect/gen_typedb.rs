use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::quote;

use super::ReflectDerive;
use super::data::{ReflectEnum, ReflectStruct, StructField};

pub(crate) fn gen_typedb(derive: &ReflectDerive) -> TokenStream {
    let meta = derive.meta();
    let zlim_reflect = meta.zlim_reflect();
    let type_database = crate::path::type_database_trait(zlim_reflect);
    let type_db = crate::path::type_db(zlim_reflect);

    let real_ident = meta.ident();
    let (impl_generics, ty_generics, where_clause) = meta.split_generics();

    let attrs = meta.attrs();

    let defaultor = attrs.has_default.then(|| {
        quote! { __db_x_.insert_defaultor::<Self>(); }
    });
    let serializer = attrs.has_serialize.then(|| {
        quote! { __db_x_.insert_serializer::<Self>(); }
    });
    let deserializer = attrs.has_deserialize.then(|| {
        quote! { __db_x_.insert_deserializer::<Self>(); }
    });
    let addtional = attrs.addtional_on_register.as_ref().map(|expr| {
        quote! { #expr(__db_x_); }
    });

    let dependencies = match derive {
        ReflectDerive::Struct(r) => struct_deps(r),
        ReflectDerive::Tuple(r) => struct_deps(r),
        ReflectDerive::Enum(r) => enum_deps(r),
        _ => TokenStream::new(),
    };

    let need_inline = dependencies.is_empty().then(|| quote! { #[inline] });

    quote! {
        #[automatically_derived]
        impl #impl_generics #type_database for #real_ident #ty_generics #where_clause {
            fn on_register(__db_x_: &'static #type_db) {
                #defaultor
                #serializer
                #deserializer
                #addtional
            }

            #need_inline
            fn register_dependencies() {
                #dependencies
            }
        }
    }
}

fn enum_deps(derive: &ReflectEnum) -> TokenStream {
    let meta = derive.meta();
    let zlim_reflect = meta.zlim_reflect();
    let type_db = crate::path::type_db(zlim_reflect);

    let mut type_set: BTreeSet<String> = BTreeSet::new();

    // A remote field is registered as its wrapper: that is the type reflection sees for it, and
    // the remote type itself is not reflected at all.
    let dependencies = derive
        .variants
        .iter()
        .flat_map(|variant| variant.active_fields())
        .flat_map(|field| field_deps(field, &type_db, &mut type_set));

    quote! { #(#dependencies)* }
}

fn struct_deps(derive: &ReflectStruct) -> TokenStream {
    let meta = derive.meta();
    let zlim_reflect = meta.zlim_reflect();
    let type_db = crate::path::type_db(zlim_reflect);

    let mut type_set: BTreeSet<String> = BTreeSet::new();

    // A remote field is registered as its wrapper, as in `enum_deps`.
    let dependencies = derive
        .active_fields()
        .flat_map(|field| field_deps(field, &type_db, &mut type_set));

    quote! { #(#dependencies)* }
}

fn field_deps(
    field: &StructField<'_>,
    type_db: &TokenStream,
    set: &mut BTreeSet<String>,
) -> Vec<TokenStream> {
    let ty = field.reflected_ty();

    if !set.insert(ty.to_string()) {
        // already registered, skip if it does not contains attributes.
        if !field.defaultable() && !field.serializable() && !field.deserializable() {
            return Vec::new();
        }
    }

    let mut deps = vec![quote! { let __db_deps_ = #type_db::register::<#ty>(); }];

    if field.defaultable() {
        deps.push(quote! {
            __db_deps_.insert_defaultor::<#ty>();
        });
    }
    if field.serializable() {
        deps.push(quote! {
            __db_deps_.insert_serializer::<#ty>();
        });
    }
    if field.deserializable() {
        deps.push(quote! {
            __db_deps_.insert_deserializer::<#ty>();
        });
    }

    deps
}
