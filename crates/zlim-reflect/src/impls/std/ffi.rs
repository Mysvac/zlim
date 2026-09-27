use crate::ops::Opaque;
use std::ffi::{OsStr, OsString};

use crate::impls::impl_simple_type_path;

// Both live in `std::ffi`. `OsString` is reflected below; `OsStr` is not — the unsized one can
// only be borrowed, which reflection has no way to hand out — so it is named here and nothing else.
impl_simple_type_path!(OsString: "std", "ffi", "OsString");
impl_simple_type_path!(OsStr: "std", "ffi", "OsStr");

zlim_reflect_derive::impl_reflect! {
    #[reflect(Opaque, Default, Debug, Clone, Hash, Eq)]
    pub struct OsString;
}

impl Opaque for OsString {
    fn apply_str(&mut self, v: &str) -> Result<(), String> {
        *self = OsString::from(v);
        Ok(())
    }

    fn stringify(&self) -> String {
        self.to_string_lossy().into_owned()
    }
}
