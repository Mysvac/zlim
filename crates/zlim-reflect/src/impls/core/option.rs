use core::option::Option;

zlim_reflect_derive::impl_reflect! {
    #[type_path = "core::option::Option"]
    #[reflect(Default)]
    pub enum Option<T> {
        /// No value.
        None,
        /// Some value of type `T`.
        Some(T),
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::Reflect;
    use crate::dynamic::{DynamicEnum, DynamicTuple, DynamicVariant};
    use crate::ops::Enum;

    #[test]
    fn dynamic_enum_to_option_some() {
        let mut t = DynamicTuple::new();
        t.push(Box::new(42i32));
        let dyn_e = DynamicEnum::new(1, "Some", DynamicVariant::Tuple(t));

        let boxed: Box<dyn Reflect> = Box::new(dyn_e);
        let opt: Box<Option<i32>> = Option::<i32>::from_reflect(boxed).unwrap();
        assert_eq!(*opt, Some(42));
    }

    #[test]
    fn dynamic_enum_to_option_none() {
        let dyn_e = DynamicEnum::new(0, "None", DynamicVariant::Unit);

        let boxed: Box<dyn Reflect> = Box::new(dyn_e);
        let opt: Box<Option<i32>> = Option::<i32>::from_reflect(boxed).unwrap();
        assert_eq!(*opt, None);
    }

    #[test]
    fn option_to_dynamic_enum_some() {
        let val: Option<i32> = Some(99);
        let e: &dyn Enum = &val;
        let dyn_e = DynamicEnum::from_ref(e).unwrap();

        assert_eq!(dyn_e.variant_name(), "Some");
        assert_eq!(dyn_e.field_len(), 1);
        let f: &i32 = dyn_e.field_at(0).unwrap().downcast_ref().unwrap();
        assert_eq!(*f, 99);
    }

    #[test]
    fn option_to_dynamic_enum_none() {
        let val: Option<i32> = None;
        let e: &dyn Enum = &val;
        let dyn_e = DynamicEnum::from_ref(e).unwrap();

        assert_eq!(dyn_e.variant_name(), "None");
        assert_eq!(dyn_e.field_len(), 0);
    }

    #[test]
    fn option_roundtrip_via_dynamic() {
        let original: Option<i32> = Some(42);
        let e: &dyn Enum = &original;
        let dyn_e = DynamicEnum::from_ref(e).unwrap();

        let boxed: Box<dyn Reflect> = Box::new(dyn_e);
        let restored: Box<Option<i32>> = Option::<i32>::from_reflect(boxed).unwrap();
        assert_eq!(*restored, Some(42));
    }
}
