use zlim_reflect::Reflect;

use crate::borrow::{Mut, Ref};
use crate::resource::Resource;
use crate::world::World;

/// The function pointers for [reflected] [resource].
///
/// [reflected]: zlim_reflect::Reflect
/// [resource]: Resource
pub struct ReflectResource {
    pub reflect: fn(&World) -> Option<&'_ dyn Reflect>,
    pub reflect_ref: fn(&World) -> Option<Ref<'_, dyn Reflect>>,
    pub reflect_mut: fn(&mut World) -> Option<Mut<'_, dyn Reflect>>,
    pub remove: fn(&mut World) -> Option<Box<dyn Reflect>>,
    pub insert: fn(&mut World, Box<dyn Reflect>),
}

impl ReflectResource {
    /// Creates a [`ReflectResource`] for given Resource type.
    pub const fn new<R: Resource + Reflect>() -> Self {
        Self {
            reflect: |x| x.get_resource::<R>().map(|y| y as &dyn Reflect),
            reflect_ref: |x| {
                x.get_resource_ref::<R>()
                    .map(|y| Ref::from(y).map_type(|z| z as _))
            },
            reflect_mut: |x| {
                x.get_resource_mut::<R>()
                    .map(|y| Mut::from(y).map_type(|z| z as _))
            },
            remove: |w| {
                w.remove_resource::<R>()
                    .map(|x| Box::new(x) as Box<dyn Reflect>)
            },
            insert: |w, v| {
                let z = v.downcast::<R>().unwrap();
                w.insert_resource(*z);
            },
        }
    }
}
