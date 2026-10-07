use zlim_reflect::Reflect;
use zlim_reflect::ops::ApplyError;

use crate::borrow::{Mut, Ref};
use crate::component::Component;
use crate::entity::{EntityError, EntityMapper};
use crate::error::ZlimError;
use crate::ops::{EntityMut, EntityOwned, EntityRef};

/// The function pointers for [reflected] [component].
///
/// [reflected]: Reflect
/// [component]: Component
pub struct ReflectComponent {
    /// Returns a shared reference to the component, if present.
    pub reflect: fn(EntityRef<'_>) -> Option<&'_ dyn Reflect>,

    /// Returns a `Ref` guard to the component, if present.
    pub reflect_ref: fn(EntityRef<'_>) -> Option<Ref<'_, dyn Reflect>>,

    /// Returns a `Mut` guard to the component, if present.
    pub reflect_mut: fn(EntityMut<'_>) -> Option<Mut<'_, dyn Reflect>>,

    /// Maps all entity references contained in the component through the mapper.
    pub map_entities: fn(&mut dyn Reflect, &mut dyn EntityMapper),

    /// Removes the component from the entity.
    pub remove: fn(&mut EntityOwned) -> Result<(), EntityError>,

    /// Inserts the component into the entity.
    pub insert: fn(&mut EntityOwned, Box<dyn Reflect>) -> Result<(), EntityError>,

    /// Modifies the component in place, inserting it if it does not exist.
    pub modify: fn(&mut EntityOwned, Box<dyn Reflect>) -> Result<(), EntityError>,

    /// Modifies the component in place, returning `Ok(false)` if it does not exist.
    pub try_modify: fn(&mut EntityOwned, &dyn Reflect) -> Result<bool, ZlimError>,
}

impl ReflectComponent {
    /// Creates a [`ReflectComponent`] for given component type.
    pub const fn new<C: Component + Reflect>() -> Self {
        #[cold]
        #[inline(never)]
        fn apply_error(e: ApplyError) -> ZlimError {
            ZlimError::error(format!("Failed to apply a reflect component: {e}"))
        }

        Self {
            reflect: |x| x.get::<C>().map(|y| y as &dyn Reflect),
            reflect_ref: |x| x.into_ref::<C>().map(|y| y.map_type(|z| z as _)),
            reflect_mut: |x| x.into_mut::<C>().map(|y| y.map_type(|z| z as _)),
            map_entities: |x, mut m| x.downcast_mut::<C>().unwrap().map_entities(&mut m),
            remove: |x| x.remove::<C>().map(|_| ()),
            insert: |x, y| x.insert(y.take::<C>().unwrap()).map(|_| ()),
            modify: |x, y| {
                let z = y.downcast::<C>().unwrap();
                if let Some(c) = x.get_mut::<C>() {
                    *c.into_inner() = *z;
                } else {
                    x.insert(*z)?;
                }
                Ok(())
            },
            try_modify: |x, y| {
                let z = y.downcast_ref::<C>().unwrap();
                x.validate()?;
                if let Some(c) = x.get_mut::<C>() {
                    c.into_inner().reflect_apply(z).map_err(apply_error)?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            },
        }
    }
}
