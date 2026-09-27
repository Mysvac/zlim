use core::str::FromStr;

use zlim_utils::num::*;

use crate::impls::impl_simple_type_path;
use crate::ops::Opaque;

macro_rules! impl_non_max {
    ($ty:ty, $path:literal) => {
        impl_simple_type_path!($ty: "zlim_utils", "num", $path);

        zlim_reflect_derive::impl_reflect! {
            #[reflect(Opaque, Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
            pub struct $ty;
        }

        impl Opaque for $ty {
            fn apply_str(&mut self, v: &str) -> Result<(), String> {
                match <Self as FromStr>::from_str(v) {
                    Ok(v) => {
                        *self = v;
                        Ok(())
                    }
                    Err(e) => Err(e.to_string()),
                }
            }

            fn stringify(&self) -> String {
                let mut buf = core::fmt::NumBuffer::new();
                ToOwned::to_owned(self.get().format_into(&mut buf))
            }
        }
    };
}

impl_non_max!(NonMaxU8, "NonMaxU8");
impl_non_max!(NonMaxU16, "NonMaxU16");
impl_non_max!(NonMaxU32, "NonMaxU32");
impl_non_max!(NonMaxU64, "NonMaxU64");
impl_non_max!(NonMaxU128, "NonMaxU128");
impl_non_max!(NonMaxUsize, "NonMaxUsize");
impl_non_max!(NonMaxI8, "NonMaxI8");
impl_non_max!(NonMaxI16, "NonMaxI16");
impl_non_max!(NonMaxI32, "NonMaxI32");
impl_non_max!(NonMaxI64, "NonMaxI64");
impl_non_max!(NonMaxI128, "NonMaxI128");
impl_non_max!(NonMaxIsize, "NonMaxIsize");
