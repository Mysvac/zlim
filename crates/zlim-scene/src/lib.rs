#![doc = include_str!("../README.md")]
//!
//! # Layout
//!
//! The crate is layered, and a module only reaches downwards:
//!
//! | Layer | Module | What it holds |
//! |-------|--------|---------------|
//! | 0 | `__macro_exports__`, `dependency` | the `scn!` call counter, and what a description needs loaded |
//! | 1 | `resolved`, `scene`, `scene_list` | the resolved form of one entity, and the two traits a description implements |
//! | 2 | `compose`, `children`, `tuple` | the pieces a description is composed out of |
//! | 3 | `apply`, `world` | applying a resolved scene, and the `World` shorthand for it |
//! | 4 | `patch` | the scene assets: `ScenePatch`, `SceneListPatch`, and the cached-scene half of resolving |
//! | 5 | `spawn`, `plugin` | queueing a scene, and the job that builds what was queued |
//!
//! `patch` is the one module that reaches back up: a patch *is* a description plus the asset system,
//! so `patch` and `scene` reference each other (resolving reads the patches a scene may build on),
//! and `patch` and `resolved` reference each other (a patch holds the resolved form of its
//! description, and a scene that builds on a cached one holds the patch's handle). Both are what
//! "a scene asset" means; splitting them would take a type-erased handle, which buys the module graph
//! nothing but costs the type.

// -----------------------------------------------------------------------------
// Modules

// Layer 0 — the leaves: nothing here uses the rest of the crate.
mod dependency;

// Layer 1 — the resolved form of an entity, and the traits a description implements.
mod resolved;
mod scene;
mod scene_list;

// Layer 2 — the pieces a description is composed out of.
mod children;
mod compose;
mod tuple;

// Layer 3 — applying what was resolved.
mod apply;
mod world;

// Layer 4 — the asset side: the patches, and the cached-scene half of resolving.
mod patch;

// Layer 4 — the document half: a scene read from a document, arranged into the resolved form.
mod dynamic;

// Layer 5 — queueing, and the job that builds what was queued.
mod plugin;
mod spawn;

// -----------------------------------------------------------------------------
// Exports

pub use crate::children::{SceneChildren, SceneParent};
pub use crate::compose::{InitTemplate, InsertTemplate, PatchIntoTemplate, PatchTemplate};
pub use crate::compose::{SceneFunction, SceneListScope, SceneScope, TemplatePatch};
pub use crate::dependency::{SceneDependencies, SceneDependency};
pub use crate::derive::{scn, scn_list};
pub use crate::patch::{CachedSceneAsset, SceneListPatch, SceneListPatchHandle};
pub use crate::patch::{ScenePatch, ScenePatchHandle};
pub use crate::plugin::ScenePlugin;
pub use crate::resolved::ResolvedScene;
pub use crate::scene::{ResolveContext, Scene, SceneBox};
pub use crate::scene_list::{EntityScene, SceneList, SceneListBox};
pub use crate::spawn::{SceneListPatchInstance, ScenePatchInstance, SceneQueue};
pub use crate::world::{CommandsSceneExt, EntityCommandsExt, WorldSceneExt};

/// The scene jobs.
pub mod jobs {
    #[doc(inline)]
    pub use crate::spawn::HandleSceneSpawn;
}

/// The scene macros.
pub mod derive {
    #[doc(inline)]
    pub use zlim_scene_derive::{scn, scn_list};
}

/// The scene plugins.
pub mod plugins {
    #[doc(no_inline)]
    pub use crate::ScenePlugin;
}

/// The scene preludes.
pub mod prelude {
    #[doc(no_inline)]
    pub use crate::derive::{scn, scn_list};
    #[doc(no_inline)]
    pub use crate::patch::{SceneListPatch, ScenePatch};
    #[doc(no_inline)]
    pub use crate::resolved::ResolvedScene;
    #[doc(no_inline)]
    pub use crate::scene::Scene;
    #[doc(no_inline)]
    pub use crate::scene_list::SceneList;
    #[doc(no_inline)]
    pub use crate::world::{CommandsSceneExt, EntityCommandsExt, WorldSceneExt};
}

// -----------------------------------------------------------------------------
// Macro Exports

/// Internal module, public for derive macros.
#[doc(hidden)]
pub mod __macro_exports__ {
    use core::sync::atomic::{AtomicU64, Ordering};

    /// Counts how many times a macro invocation has run.
    ///
    /// A `#Name` is identified by where it was written and by its ordinal within the invocation,
    /// which is enough to tell two *different* invocations apart. It is not enough for two runs of
    /// the *same* invocation — a function that builds a scene and is called twice — because both
    /// runs would produce the same references, and the second run would resolve the first run's
    /// entities. The counter is what tells those apart: one static per invocation, bumped per run.
    pub struct CallCounter(AtomicU64);

    impl CallCounter {
        /// Creates a counter that starts at zero.
        #[expect(clippy::new_without_default, reason = "need const")]
        pub const fn new() -> Self {
            Self(AtomicU64::new(0))
        }

        /// Returns the run this is, and counts it.
        #[inline]
        pub fn increment(&self) -> u64 {
            self.0.fetch_add(1, Ordering::Relaxed)
        }
    }

    pub use zlim_core::template::EntityReference;
    pub use zlim_core::template::EntityTemplate;
    pub use zlim_core::template::IntoTemplate;
}

// -----------------------------------------------------------------------------
