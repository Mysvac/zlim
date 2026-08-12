use core::any::TypeId;

use crate::bundle::{Bundle, BundleId, ComponentCollector};
use crate::component::ComponentId;
use crate::world::World;

impl World {
    #[inline]
    pub fn register_bundle<B: Bundle>(&mut self) -> BundleId {
        #[cold]
        #[inline(never)]
        fn register_cold(
            world: &mut World,
            type_id: TypeId,
            collect: fn(&mut ComponentCollector),
        ) -> BundleId {
            let mut collector = ComponentCollector::new(Some(&mut world.components));

            collect(&mut collector);

            let components = collector.finish();
            debug_assert!(components.is_sorted());
            world.bundles.register(type_id, components)
        }

        if let Some(id) = self.bundles.get_by_type(TypeId::of::<B>()) {
            return id;
        }

        register_cold(self, TypeId::of::<B>(), B::collect)
    }

    /// Registers a bundle from given `ComponentIds` and returns its [`BundleId`].
    ///
    /// If the target `BundleInfo` already exists, returns it directly.
    ///
    /// This function can be used for runtime dynamic operation.
    ///
    /// # Panics
    ///
    /// Panics if any provided component id is not registered in this world.
    pub fn register_dynamic_bundle(&mut self, idents: &[ComponentId]) -> BundleId {
        #[cold]
        #[inline(never)]
        fn register_cold(world: &mut World, idents: &[ComponentId]) -> BundleId {
            let mut idents = idents.to_vec();

            if !idents.is_sorted() {
                idents.sort();
            }
            idents.dedup();

            let components = crate::utils::SlicePool::component(&idents);

            world.bundles.register_dynamic(components)
        }

        if let Some(id) = self.bundles.get_by_arch(idents) {
            return id;
        }

        register_cold(self, idents)
    }
}
