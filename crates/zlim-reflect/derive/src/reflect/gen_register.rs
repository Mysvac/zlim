use proc_macro2::TokenStream;
use quote::quote;

use super::ReflectDerive;

pub(crate) fn gen_register(derive: &ReflectDerive) -> TokenStream {
    let meta = derive.meta();

    // Auto-register requires a concrete, non-generic type.
    // This must also exclude lifetime-only generics like `Foo<'a>`.
    if !meta.only_lifetime_generics() {
        return TokenStream::new();
    }

    let real_ident = meta.ident();
    let zlim_reflect = meta.zlim_reflect();

    // ↓ See [`zlim_reflect::register!`]'s implementation.
    quote! {
        #zlim_reflect::db::__internal__::submit!(
            #zlim_reflect::db::__internal__::__TypeReg__::of::<#real_ident>()
            => #zlim_reflect::db::__internal__::__TypeReg__
        );
    }
}
