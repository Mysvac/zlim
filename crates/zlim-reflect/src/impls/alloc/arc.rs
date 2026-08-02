use std::sync::Arc;

use crate::{TypePath, ops::Opaque};

zlim_reflect_derive::impl_reflect! {
    #[type_path = "alloc::sync::Arc"]
    #[reflect(Opaque, Clone)]
    pub struct Arc<T: Send + Sync + ?Sized>;
}

impl<T: TypePath + Send + Sync + ?Sized> Opaque for Arc<T> {
    fn apply_str(&mut self, _: &str) -> Result<(), String> {
        Err(String::from("`Arc` cannot apply_str"))
    }

    fn stringify(&self) -> String {
        format!(
            "{}({:?})",
            Self::type_path(),
            &**self as *const T as *const ()
        )
    }
}
