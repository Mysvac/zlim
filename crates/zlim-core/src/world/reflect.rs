use core::any::TypeId;

use zlim_ptr::Ptr;
use zlim_reflect::serde::ReflectContext;

use crate::entity::Entities;
use crate::world::World;

unsafe impl ReflectContext for World {
    fn get_ptr(&self, ty: TypeId) -> Option<Ptr<'_>> {
        if ty == TypeId::of::<Self>() {
            return Some(Ptr::from_ref(self));
        }
        if let Some(cell) = self.resources.get(ty) {
            return cell.get_data();
        }
        if ty == TypeId::of::<Entities>() {
            return Some(Ptr::from_ref(&self.entities));
        }
        // Do we need to export more fields?
        None
    }
}
