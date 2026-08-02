use core::marker::PhantomData;

pub struct Entity<'w> {
    // world: UnsafeWorld<'w>,
    // entity: EntityId,
    // info: Option<&'w EntityInfo>,
    // model: Option<&'w Model>,
    _marker: PhantomData<&'w ()>,
}

pub struct EntityRef<'w> {
    // world: &'w World,
    // entity: EntityId,
    // location: EntityLocation,
    _marker: PhantomData<&'w ()>,
}

pub struct EntityMut<'w> {
    // world: &'w mut World,
    // entity: EntityId,
    // location: EntityLocation,
    _marker: PhantomData<&'w ()>,
}
