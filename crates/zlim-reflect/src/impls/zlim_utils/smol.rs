use zlim_utils::str::SmolStr;

use crate::Reflect;
use crate::impls::impl_simple_type_path;
use crate::info::ReflectKind;
use crate::ops::Opaque;

impl_simple_type_path!(SmolStr: "zlim_utils", "str", "SmolStr");

zlim_reflect_derive::impl_reflect! {
    #[reflect(Opaque, Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    #[reflect(from_reflect = from_reflect)]
    pub struct SmolStr;
}

fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<SmolStr>, Box<dyn Reflect>> {
    let value = match value.downcast::<SmolStr>() {
        Ok(ret) => return Ok(ret),
        Err(e) => e,
    };

    if value.reflect_kind() != ReflectKind::Opaque {
        return Err(value);
    }

    let value = value.reflect_owned().into_opaque().unwrap();

    Ok(Box::new(SmolStr::from_str(&value.stringify())))
}

impl Opaque for SmolStr {
    fn apply_str(&mut self, v: &str) -> Result<(), String> {
        *self = Self::from_str(v);
        Ok(())
    }

    fn stringify(&self) -> String {
        String::from(self.as_str())
    }
}
