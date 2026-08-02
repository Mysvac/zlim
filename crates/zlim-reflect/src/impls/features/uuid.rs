use uuid::{NonNilUuid, Uuid};

use crate::ops::Opaque;

zlim_reflect_derive::impl_reflect! {
    #[type_path = "uuid::Uuid"]
    #[reflect(Opaque, Default, Debug, Clone, Hash, Eq, Serialize, Deserialize)]
    pub struct Uuid;
}

zlim_reflect_derive::impl_reflect! {
    #[type_path = "uuid::Uuid"]
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
            None => Err("NonNilUuid cannot be nil".to_string()),
        }
    }

    fn stringify(&self) -> String {
        self.to_string()
    }
}
