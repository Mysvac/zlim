use uuid::{NonNilUuid, Uuid};

use crate::impls::impl_simple_type_path;
use crate::ops::Opaque;

impl_simple_type_path!(Uuid: "uuid", "Uuid");
impl_simple_type_path!(NonNilUuid: "uuid", "NonNilUuid");

zlim_reflect_derive::impl_reflect! {
    #[reflect(Opaque, Default, Debug, Clone, Hash, Eq, Serialize, Deserialize)]
    pub struct Uuid;
}

zlim_reflect_derive::impl_reflect! {
    #[reflect(Opaque, Debug, Clone, Hash, Eq, Serialize, Deserialize)]
    pub struct NonNilUuid;
}

impl Opaque for Uuid {
    fn apply_str(&mut self, v: &str) -> Result<(), String> {
        match Uuid::parse_str(v) {
            Ok(parsed) => {
                *self = parsed;
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        }
    }

    fn stringify(&self) -> String {
        self.to_string()
    }
}

impl Opaque for NonNilUuid {
    fn apply_str(&mut self, v: &str) -> Result<(), String> {
        let parsed = Uuid::parse_str(v).map_err(|e| e.to_string())?;

        match NonNilUuid::new(parsed) {
            Some(non_nil) => {
                *self = non_nil;
                Ok(())
            }
            None => Err("NonNilUuid cannot be nil".into()),
        }
    }

    fn stringify(&self) -> String {
        self.to_string()
    }
}
