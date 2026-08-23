use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_quote};

pub(crate) fn expand(mut item_fn: ItemFn) -> TokenStream {
    assert_eq!(
        item_fn.sig.ident, "main",
        "`zlim_main` can only be used on a function called 'main'."
    );

    let zlim_app = zlim_derive_utils::crate_path("zlim_app");
    let zlim_task = zlim_derive_utils::crate_path("zlim_task");

    // Android APP
    let android_app = quote! { #zlim_app::sys::AndroidApp };
    let static_android_app = quote! { #zlim_app::sys::ANDROID_APP };

    // zlim-task main thread marker
    item_fn
        .block
        .stmts
        .insert(0, parse_quote! { #zlim_task::set_main_thread(); });

    quote! {
        #[unsafe(no_mangle)]
        #[cfg(target_os = "android")]
        fn android_main(android_app: #android_app) {
            let _ = #static_android_app.set(android_app);
            main();
        }

        #item_fn
    }
}
