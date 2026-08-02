use std::ffi::OsString;
use std::path::PathBuf;

use crate::db::TypeDB;
use crate::ops::Opaque;

zlim_reflect_derive::impl_reflect! {
    #[type_path = "std::ffi::OsString"]
    #[reflect(Opaque, Debug, Clone, Hash, Eq)]
    #[reflect(on_register = on_register)]
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

fn on_register(db: &'static TypeDB) {
    db.insert_convertor(<OsString as Into<PathBuf>>::into);
}
