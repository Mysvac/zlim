use core::fmt::Debug;
use core::hash::Hash;
use core::num::*;
use core::str::FromStr;

use serde_core::{Deserialize, Serialize};

use crate::ops::Opaque;

macro_rules! impl_zon_zero {
    ($ty:ty, $path:literal) => {
        zlim_reflect_derive::impl_reflect! {
            #[type_path = $path]
            #[reflect(Opaque, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
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

impl_zon_zero!(NonZeroU8, "core::num::NonZeroU8");
impl_zon_zero!(NonZeroU16, "core::num::NonZeroU16");
impl_zon_zero!(NonZeroU32, "core::num::NonZeroU32");
impl_zon_zero!(NonZeroU64, "core::num::NonZeroU64");
impl_zon_zero!(NonZeroU128, "core::num::NonZeroU128");
impl_zon_zero!(NonZeroUsize, "core::num::NonZeroUsize");
impl_zon_zero!(NonZeroI8, "core::num::NonZeroI8");
impl_zon_zero!(NonZeroI16, "core::num::NonZeroI16");
impl_zon_zero!(NonZeroI32, "core::num::NonZeroI32");
impl_zon_zero!(NonZeroI64, "core::num::NonZeroI64");
impl_zon_zero!(NonZeroI128, "core::num::NonZeroI128");
impl_zon_zero!(NonZeroIsize, "core::num::NonZeroIsize");

zlim_reflect_derive::impl_reflect! {
    #[type_path = "core::num::Wrapping"]
    #[reflect(Debug, Clone, Hash, Eq, Serialize, Deserialize)]
    pub struct Wrapping<T: Copy + Send + Sync + Debug + Eq + Hash + Serialize + for<'de> Deserialize<'de>>(pub T);
}

zlim_reflect_derive::impl_reflect! {
    #[type_path = "core::num::Saturating"]
    #[reflect(Debug, Clone, Hash, Eq /*, Serialize, Deserialize  */)] // `serde` dose not implement ser/de for `Saturating`. 2026-08.
    pub struct Saturating<T: Copy + Send + Sync + Debug + Eq + Hash /* + Serialize + for<'de> Deserialize<'de>  */>(pub T);
}
