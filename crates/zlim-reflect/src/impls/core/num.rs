use core::fmt::Debug;
use core::hash::Hash;
use core::num::*;
use core::str::FromStr;

use crate::impls::impl_simple_type_path;
use crate::ops::Opaque;

impl_simple_type_path!(@Wrapping<T>: "core", "num", "Wrapping");
impl_simple_type_path!(@Saturating<T>: "core", "num", "Saturating");

macro_rules! impl_zon_zero {
    ($ty:ty, $path:literal) => {
        impl_simple_type_path!($ty: "core", "num", $path);

        zlim_reflect_derive::impl_reflect! {
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

impl_zon_zero!(NonZeroU8, "NonZeroU8");
impl_zon_zero!(NonZeroU16, "NonZeroU16");
impl_zon_zero!(NonZeroU32, "NonZeroU32");
impl_zon_zero!(NonZeroU64, "NonZeroU64");
impl_zon_zero!(NonZeroU128, "NonZeroU128");
impl_zon_zero!(NonZeroUsize, "NonZeroUsize");
impl_zon_zero!(NonZeroI8, "NonZeroI8");
impl_zon_zero!(NonZeroI16, "NonZeroI16");
impl_zon_zero!(NonZeroI32, "NonZeroI32");
impl_zon_zero!(NonZeroI64, "NonZeroI64");
impl_zon_zero!(NonZeroI128, "NonZeroI128");
impl_zon_zero!(NonZeroIsize, "NonZeroIsize");

zlim_reflect_derive::impl_reflect! {
    #[reflect(Debug, Clone, Hash, Eq)]
    pub struct Wrapping<T: Copy + Send + Sync + Debug + Eq + Hash>(pub T);
}

zlim_reflect_derive::impl_reflect! {
    #[reflect(Debug, Clone, Hash, Eq)]
    pub struct Saturating<T: Copy + Send + Sync + Debug + Eq + Hash>(pub T);
}
