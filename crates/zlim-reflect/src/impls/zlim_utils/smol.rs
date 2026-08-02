use core::any::TypeId;

use zlim_utils::str::SmolStr;

use crate::Reflect;
use crate::db::TypeDB;
use crate::impls::CONVERT_TYPE_ERROR;
use crate::info::ReflectKind;
use crate::ops::Opaque;

zlim_reflect_derive::impl_reflect! {
    #[type_path = "zlim_utils::str::SmolStr"]
    #[reflect(Opaque, Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    #[reflect(on_register = on_register, from_reflect = from_reflect)]
    pub struct SmolStr;
}

fn from_reflect(value: Box<dyn Reflect>) -> Result<Box<SmolStr>, Box<dyn Reflect>> {
    let mut value = match value.downcast::<SmolStr>() {
        Ok(ret) => return Ok(ret),
        Err(e) => e,
    };

    if let Some(db) = TypeDB::get_by_type(value.type_id()) {
        match db.convert(value, TypeId::of::<SmolStr>()) {
            Ok(ret) => {
                let converted = ret.downcast::<SmolStr>();
                return Ok(converted.expect(CONVERT_TYPE_ERROR));
            }
            Err(v) => value = v,
        }
    }

    if value.reflect_kind() != ReflectKind::Opaque {
        return Err(value);
    }

    let value = value.reflect_owned().into_opaque().unwrap();

    Ok(Box::new(SmolStr::from_str(&value.stringify())))
}

fn on_register(db: &'static TypeDB) {
    db.insert_convertor(<String as Into<SmolStr>>::into);
    db.insert_convertor(<SmolStr as Into<String>>::into);
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
