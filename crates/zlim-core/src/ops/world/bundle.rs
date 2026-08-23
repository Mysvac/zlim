//! Bundle registration methods implemented on `World`.

use core::any::TypeId;

use crate::bundle::{Bundle, BundleId};
use crate::component::{ComponentCollector, ComponentId};
use crate::world::World;

impl World {
    /// Registers the bundle type `B` and returns its [`BundleId`].
    ///
    /// If the bundle is already registered, the existing id is returned.
    ///
    /// The registered component set includes every component the bundle
    /// provides **plus** all required components (recursively), so the
    /// target table has storage for them.
    #[inline]
    pub fn register_required_bundle<B: Bundle>(&mut self) -> BundleId {
        #[cold]
        #[inline(never)]
        fn register_cold(
            world: &mut World,
            type_id: TypeId,
            collect: fn(&mut ComponentCollector),
        ) -> BundleId {
            let mut collector = ComponentCollector::new(Some(&world.components));

            collect(&mut collector);

            let components = collector.finish();
            debug_assert!(components.is_sorted());
            world.bundles.register_required(type_id, components)
        }

        if let Some(id) = self.bundles.get_required(TypeId::of::<B>()) {
            return id;
        }

        register_cold(self, TypeId::of::<B>(), B::collect_required)
    }

    /// Registers the bundle type `B` and returns its [`BundleId`].
    ///
    /// If the bundle is already registered, the existing id is returned.
    ///
    /// The registered component set includes every component the bundle
    /// provides **plus** all required components (recursively), so the
    /// target table has storage for them.
    #[inline]
    pub fn register_explicit_bundle<B: Bundle>(&mut self) -> BundleId {
        #[cold]
        #[inline(never)]
        fn register_cold(
            world: &mut World,
            type_id: TypeId,
            collect: fn(&mut ComponentCollector),
        ) -> BundleId {
            let mut collector = ComponentCollector::new(Some(&world.components));

            collect(&mut collector);

            let components = collector.finish();
            debug_assert!(components.is_sorted());
            world.bundles.register_explicit(type_id, components)
        }

        if let Some(id) = self.bundles.get_explicit(TypeId::of::<B>()) {
            return id;
        }

        register_cold(self, TypeId::of::<B>(), B::collect_explicit)
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
