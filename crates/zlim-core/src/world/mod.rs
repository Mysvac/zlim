use zlim_utils::define_atomic_id;

define_atomic_id!(WorldId);

pub struct World {
    pub id: WorldId,
}
