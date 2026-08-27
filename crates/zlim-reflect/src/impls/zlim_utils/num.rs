use core::str::FromStr;

use zlim_utils::num::*;

use crate::ops::Opaque;

macro_rules! impl_zon_zero {
    ($ty:ty, $path:literal) => {
        zlim_reflect_derive::impl_reflect! {
            #[type_path = $path]
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

impl_zon_zero!(NonMaxU8, "zlim_utils::num::NonMaxU8");
impl_zon_zero!(NonMaxU16, "zlim_utils::num::NonMaxU16");
impl_zon_zero!(NonMaxU32, "zlim_utils::num::NonMaxU32");
impl_zon_zero!(NonMaxU64, "zlim_utils::num::NonMaxU64");
impl_zon_zero!(NonMaxU128, "zlim_utils::num::NonMaxU128");
impl_zon_zero!(NonMaxUsize, "zlim_utils::num::NonMaxUsize");
impl_zon_zero!(NonMaxI8, "zlim_utils::num::NonMaxI8");
impl_zon_zero!(NonMaxI16, "zlim_utils::num::NonMaxI16");
impl_zon_zero!(NonMaxI32, "zlim_utils::num::NonMaxI32");
impl_zon_zero!(NonMaxI64, "zlim_utils::num::NonMaxI64");
impl_zon_zero!(NonMaxI128, "zlim_utils::num::NonMaxI128");
impl_zon_zero!(NonMaxIsize, "zlim_utils::num::NonMaxIsize");
