//! Tuple implementations of [`Scene`] and [`SceneList`].

use zlim_core::error::ZlimResult;

use crate::dependency::SceneDependencies;
use crate::resolved::ResolvedScene;
use crate::scene::{ResolveContext, Scene};
use crate::scene_list::SceneList;

/// Generates [`Scene`] and [`SceneList`] implementations for tuples.
///
/// A tuple is composed of its elements in declaration order, which is the order in which they are
/// resolved: a tuple of scenes describes one entity, and a tuple of scene lists describes one entity
/// per entry, in order.
///
/// [`resolve`]: Scene::resolve
/// [`resolve_list`]: SceneList::resolve_list
macro_rules! impl_scene_for_tuple {
    (0: []) => {
        impl Scene for () {
            #[inline(always)]
            fn resolve(
                self,
                _context: &mut ResolveContext,
                _scene: &mut ResolvedScene,
            ) -> ZlimResult<()> {
                Ok(())
            }

            #[inline(always)]
            fn register_dependencies(&self, _dependencies: &mut SceneDependencies) {}
        }

        impl SceneList for () {
            #[inline(always)]
            fn resolve_list(
                self,
                _context: &mut ResolveContext,
                _scenes: &mut Vec<ResolvedScene>,
            ) -> ZlimResult<()> {
                Ok(())
            }

            #[inline(always)]
            fn register_dependencies(&self, _dependencies: &mut SceneDependencies) {}
        }
    };
    (1 : [ $index:tt : $scene:ident ]) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 scenes long.")]
        impl<$scene: Scene> Scene for ($scene,) {
            #[inline]
            fn resolve(
                self,
                context: &mut ResolveContext,
                scene: &mut ResolvedScene,
            ) -> ZlimResult<()> {
                self.$index.resolve(context, scene)
            }

            #[inline]
            fn register_dependencies(&self, dependencies: &mut SceneDependencies) {
                self.$index.register_dependencies(dependencies);
            }
        }

        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 12 scenes long.")]
        impl<$scene: SceneList> SceneList for ($scene,) {
            #[inline]
            fn resolve_list(
                self,
                context: &mut ResolveContext,
                scenes: &mut Vec<ResolvedScene>,
            ) -> ZlimResult<()> {
                self.$index.resolve_list(context, scenes)
            }

            #[inline]
            fn register_dependencies(&self, dependencies: &mut SceneDependencies) {
                self.$index.register_dependencies(dependencies);
            }
        }
    };
    ($num:literal : [$($index:tt : $scene:ident),*]) => {
        #[cfg_attr(docsrs, doc(hidden))]
        impl<$($scene: Scene),*> Scene for ($($scene,)*) {
            fn resolve(
                self,
                context: &mut ResolveContext,
                scene: &mut ResolvedScene,
            ) -> ZlimResult<()> {
                $( self.$index.resolve(context, scene)?; )*
                Ok(())
            }

            fn register_dependencies(&self, dependencies: &mut SceneDependencies) {
                $( self.$index.register_dependencies(dependencies); )*
            }
        }

        #[cfg_attr(docsrs, doc(hidden))]
        impl<$($scene: SceneList),*> SceneList for ($($scene,)*) {
            fn resolve_list(
                self,
                context: &mut ResolveContext,
                scenes: &mut Vec<ResolvedScene>,
            ) -> ZlimResult<()> {
                $( self.$index.resolve_list(context, scenes)?; )*
                Ok(())
            }

            fn register_dependencies(&self, dependencies: &mut SceneDependencies) {
                $( self.$index.register_dependencies(dependencies); )*
            }
        }
    };
}

zlim_utils::range_invoke!(impl_scene_for_tuple, 12);
