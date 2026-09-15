use core::result::Result;

zlim_reflect_derive::impl_reflect! {
    #[type_path = "core::result::Result"]
    pub enum Result<T, E> {
        /// Contains the success value
        Ok(T),
        /// Contains the error value
        Err(E),
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::Reflect;
    use crate::dynamic::DynamicEnum;
    use crate::dynamic::DynamicTuple;
    use crate::dynamic::DynamicVariant;
    use crate::ops::Enum;
    use std::string::String;

    /// A hand-built `Ok` variant is accepted by `from_reflect`, and its tuple
    /// payload becomes the success value.
    #[test]
    fn dynamic_enum_to_result_ok() {
        let mut t = DynamicTuple::new();
        t.push(Box::new(2026_i32));

        let dyn_e = DynamicEnum::new(0, "Ok", DynamicVariant::Tuple(t));
        let boxed: Box<dyn Reflect> = Box::new(dyn_e);

        let result: Box<Result<i32, String>> = Result::<i32, String>::from_reflect(boxed).unwrap();

        assert_eq!(*result, Ok(2026_i32));
    }

    /// The same conversion for the error arm: the payload has to land in `E` and
    /// survive the trip as a `String`.
    #[test]
    fn dynamic_enum_to_result_err() {
        let mut t = DynamicTuple::new();
        t.push(Box::new(String::from("oops")));

        let dyn_e = DynamicEnum::new(1, "Err", DynamicVariant::Tuple(t));
        let boxed: Box<dyn Reflect> = Box::new(dyn_e);

        let result: Box<Result<i32, String>> = Result::<i32, String>::from_reflect(boxed).unwrap();

        assert_eq!(*result, Err(String::from("oops")));
    }

    /// The reverse direction, turning a concrete `Ok` into a `DynamicEnum` that
    /// keeps the variant name and exposes the payload as field 0.
    #[test]
    fn result_to_dynamic_enum_ok() {
        let val: Result<i32, String> = Ok(7);
        let e: &dyn Enum = &val;
        let dyn_e = DynamicEnum::from_ref(e).unwrap();

        assert_eq!(dyn_e.variant_name(), "Ok");
        assert_eq!(dyn_e.field_len(), 1);
        let f: &i32 = dyn_e.field_at(0).unwrap().downcast_ref().unwrap();
        assert_eq!(*f, 7_i32);
    }

    /// The reverse direction for `Err`, whose payload is the error type rather
    /// than the success type.
    #[test]
    fn result_to_dynamic_enum_err() {
        let val: Result<i32, String> = Err(String::from("boom"));
        let e: &dyn Enum = &val;
        let dyn_e = DynamicEnum::from_ref(e).unwrap();

        assert_eq!(dyn_e.variant_name(), "Err");
        assert_eq!(dyn_e.field_len(), 1);
        let f: &String = dyn_e.field_at(0).unwrap().downcast_ref().unwrap();
        assert_eq!(f, "boom");
    }

    /// Chains both directions, so a value that goes through the dynamic form has
    /// to come back equal to the original.
    #[test]
    fn result_roundtrip_via_dynamic() {
        let original: Result<i32, String> = Err(String::from("boom"));
        let e: &dyn Enum = &original;

        let dyn_e = DynamicEnum::from_ref(e).unwrap();
        let boxed: Box<dyn Reflect> = Box::new(dyn_e);

        let restored: Box<Result<i32, String>> =
            Result::<i32, String>::from_reflect(boxed).unwrap();

        assert_eq!(*restored, Err(String::from("boom")));
    }
}
