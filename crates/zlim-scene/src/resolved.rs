//! [`ResolvedScene`] — what a [`Scene`] resolves to.
//!
//! [`Scene`]: crate::Scene

use core::any::{Any, TypeId};
use core::fmt::{Debug, Formatter};
use std::sync::Arc;

use zlim_asset::assets::Assets;
use zlim_asset::handle::Handle;
use zlim_utils::ext::TypeMap;
use zlim_utils::hash::HashSet;

use zlim_core::entity::EntityId;
use zlim_core::error::ZlimError;
use zlim_core::template::{EntityReference, EntityTemplate};
use zlim_core::template::{ErasedTemplate, Template, TemplateEffect};

use crate::patch::ScenePatch;

// -----------------------------------------------------------------------------
// ResolvedScene

/// A resolved scene: the templates to write to one entity, and the entities that belong with it.
///
/// A [`Scene`] is a description; resolving it fills this type in. What is left is to
/// apply it: spawn the entity, write the templates into it, spawn its children with it as their
/// parent, and connect the parent edge it carries.
///
/// # Templates
///
/// Templates are stored erased, in the order they were added, and are applied in that order:
///
/// - [`push_template`](Self::push_template) appends a template for the entity's components. The
///   same type may be pushed more than once, and every copy is applied.
/// - [`insert_template`](Self::insert_template) stores a template under its own [`TypeId`], so that
///   a later part of the composition — or a patch — can replace it instead of adding another copy.
///   This is the canonical slot that [`get_or_insert_template`](Self::get_or_insert_template)
///   hands out.
///
/// A template's output decides how much it writes: a [`Component`] becomes one column of the row, a
/// [`Bundle`] hands over every component it carries — required components included, which the writer
/// initialises. Either way the write goes through the same [`BundleWriter`], so a bundle is only a
/// template that produces more than one value.
///
/// [`Component`]: zlim_core::component::Component
/// [`BundleWriter`]: zlim_core::bundle::BundleWriter
///
/// # Hierarchy
///
/// - `entity_references` are the names this scene declares. Applying the scene binds each of them
///   to the entity it produced, which is what a `#Name` mentioned anywhere in the scene resolves to.
/// - `children` are the scenes to spawn *under* this entity, in list order: the order of this list
///   is the order of [`Children`], because spawning with a parent appends to it.
/// - `parent` is an explicit parent edge for the entity itself, which is what a scene applied to an
///   existing entity needs. It is applied after the entities exist, so it may name an entity that is
///   declared later in the same scene.
///
/// # Caching
///
/// A scene may be built *on top of* a cached one: [`include_cached`](Self::include_cached) names the
/// patch, and applying the scene then applies the cached scene's templates first, skips the
/// templates it replaced (those were cloned out of the cached scene when they were asked for), and
/// spawns the cached scene's children before its own. This is what makes a patch small: it stores
/// only what it changes, while the asset it builds on is resolved once and shared.
///
/// [`Scene`]: crate::Scene
/// [`Bundle`]: zlim_core::bundle::Bundle
/// [`Children`]: zlim_core::query::Children
pub struct ResolvedScene {
    /// The templates, in the order they were added.
    component_templates: Vec<Box<dyn ErasedTemplate>>,

    /// The index of each canonical template in `component_templates`.
    template_indices: TypeMap<usize>,

    /// The names this scene declares.
    entity_references: Vec<EntityReference>,

    /// The scenes to spawn as this entity's children, in order.
    children: Vec<ResolvedScene>,

    /// The explicit parent edge of this entity.
    parent: Option<EntityTemplate>,

    /// The id this entity carries in the document it was read from.
    ///
    /// A scene read from a document has one, and applying it binds that id to the entity it spawns,
    /// which is what makes the ids the document's components mention resolve. A scene built in Rust
    /// has none, and the entities its templates name are already the ones they mean.
    document_id: Option<EntityId>,

    /// The cached scene this one builds on, if any.
    cached: Option<CachedSceneInfo>,
}

impl Default for ResolvedScene {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for ResolvedScene {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ResolvedScene")
            .field("component_templates", &self.component_templates.len())
            .field("entity_references", &self.entity_references)
            .field("children", &self.children)
            .field("parent", &self.parent)
            .field("document_id", &self.document_id)
            .field("cached", &self.cached)
            .finish()
    }
}

// -----------------------------------------------------------------------------
// Construction

impl ResolvedScene {
    /// Creates a scene that describes nothing.
    #[inline]
    pub const fn new() -> Self {
        Self {
            component_templates: Vec::new(),
            template_indices: TypeMap::new(),
            entity_references: Vec::new(),
            children: Vec::new(),
            parent: None,
            document_id: None,
            cached: None,
        }
    }

    /// Returns the id this entity carries in the document it was read from, if it came from one.
    #[inline]
    pub fn document_id(&self) -> Option<EntityId> {
        self.document_id
    }

    /// Marks this entity as the one a document describes with `id`.
    ///
    /// This is what the scene loader calls for every entity of a document, and what makes the
    /// document's ids — the ones its components carry — resolve to the entities this scene spawns.
    #[inline]
    pub fn set_document_id(&mut self, id: EntityId) {
        self.document_id = Some(id);
    }
}

// -----------------------------------------------------------------------------
// Cached scenes

/// The cached scene a [`ResolvedScene`] builds on.
#[derive(Debug)]
pub(crate) struct CachedSceneInfo {
    /// The patch the cached scene lives in.
    handle: Handle<ScenePatch>,

    /// The resolved form of that patch.
    ///
    /// Holding the resolved scene keeps the layers of an application independent of the asset
    /// collection: by the time a scene is applied, everything it builds on is already in hand.
    resolved: Arc<ResolvedScene>,

    /// The template types this scene replaced, which the cached scene therefore does not apply.
    ///
    /// A template that both scenes describe is cloned out of the cached scene when it is asked for,
    /// so that a patch can edit a field of it instead of restating the whole description. Applying
    /// the cached copy as well would write the unpatched value first and the patched one second —
    /// which works, but costs a build and a write per replaced template, so it is skipped.
    duplicate_templates: HashSet<TypeId>,
}

impl CachedSceneInfo {
    /// Returns the resolved scene of the patch.
    #[inline]
    pub(crate) fn resolved(&self) -> &Arc<ResolvedScene> {
        &self.resolved
    }

    /// Returns the template types this scene replaced.
    #[inline]
    pub(crate) fn duplicate_templates(&self) -> &HashSet<TypeId> {
        &self.duplicate_templates
    }
}

impl ResolvedScene {
    /// Returns the cached scene this one builds on, if any.
    #[inline]
    pub(crate) fn cached_info(&self) -> Option<&CachedSceneInfo> {
        self.cached.as_ref()
    }

    /// Returns whether this scene builds on a cached one.
    #[inline]
    pub fn is_cached(&self) -> bool {
        self.cached.is_some()
    }

    /// Returns the patch this scene builds on, if any.
    #[inline]
    pub fn cached_patch(&self) -> Option<&Handle<ScenePatch>> {
        self.cached.as_ref().map(|cached| &cached.handle)
    }

    /// Makes this scene build on the patch at `handle`.
    ///
    /// The cached scene is applied *first*, so a template the cached scene describes can be edited in
    /// place through [`get_or_insert_template`](Self::get_or_insert_template) instead of being
    /// restated: the template is cloned out of the cached scene the first time it is asked for, and
    /// the cached copy is then skipped when the scene is applied.
    ///
    /// `patches` is the collection the patch lives in, which is where its resolved form is read from;
    /// a resolution that has none cannot include a cached scene. The patch has to be resolved
    /// already. A patch that includes one lists it as a dependency (see
    /// [`Scene::register_dependencies`]), so resolving in load order is enough.
    ///
    /// # Errors
    ///
    /// Returns an error if a cached scene was already included, if this scene already describes
    /// something — a cached scene has to be included first, or the templates it contributes could
    /// end up applied after the ones that build on them — or if the patch has not been resolved yet.
    ///
    /// [`Scene::register_dependencies`]: crate::Scene::register_dependencies
    pub fn include_cached(
        &mut self,
        patches: Option<&Assets<ScenePatch>>,
        handle: Handle<ScenePatch>,
    ) -> Result<(), ZlimError> {
        if let Some(cached) = &self.cached {
            return Err(ZlimError::error(format!(
                "the scene already includes the cached patch {:?}; a scene can only build on one",
                cached.handle
            )));
        }

        if !(self.component_templates.is_empty() && self.children.is_empty()) {
            return Err(ZlimError::error(
                "the scene already describes components or children, so a cached scene cannot be \
                 included first",
            ));
        }

        let resolved = patches
            .and_then(|patches| patches.get(&handle))
            .and_then(|patch| patch.resolved.clone())
            .ok_or_else(|| {
                ZlimError::error(format!(
                    "the cached scene patch {handle:?} has not been resolved yet, so a scene cannot \
                     build on it"
                ))
            })?;

        self.cached = Some(CachedSceneInfo {
            handle,
            resolved,
            duplicate_templates: HashSet::default(),
        });

        Ok(())
    }

    /// Records that `type_id` was cloned out of the cached scene, so the cached copy is skipped when
    /// this scene is applied.
    #[inline]
    pub(crate) fn mark_duplicated(&mut self, type_id: TypeId) {
        if let Some(cached) = &mut self.cached {
            cached.duplicate_templates.insert(type_id);
        }
    }
}

// -----------------------------------------------------------------------------
// Templates

impl ResolvedScene {
    /// Returns the templates, in the order they were added.
    #[inline]
    pub fn component_templates(&self) -> &[Box<dyn ErasedTemplate>] {
        &self.component_templates
    }

    /// Appends `template` to the back of the templates.
    ///
    /// This does not take the canonical slot of its type: the template is applied in addition to
    /// whatever else describes the entity, and [`insert_template`](Self::insert_template) cannot
    /// replace it.
    #[inline]
    pub fn push_template<T>(&mut self, template: T)
    where
        T: Template<Output: TemplateEffect> + Send + Sync + 'static,
    {
        self.push_template_erased(Box::new(template));
    }

    /// Appends an erased `template` to the back of the templates.
    #[inline]
    pub fn push_template_erased(&mut self, template: Box<dyn ErasedTemplate>) {
        self.component_templates.push(template);
    }

    /// Stores `template` as the canonical template of its type, replacing the previous one.
    #[inline]
    pub fn insert_template<T>(&mut self, template: T)
    where
        T: Template<Output: TemplateEffect> + Send + Sync + 'static,
    {
        self.insert_erased_template(TypeId::of::<T>(), Box::new(template));
    }

    /// Stores an erased `template` as the canonical template of `type_id`, replacing the previous
    /// one.
    ///
    /// If no template of that type was stored yet, the template is appended to the back, so that the
    /// application order still follows the composition order.
    pub fn insert_erased_template(&mut self, type_id: TypeId, template: Box<dyn ErasedTemplate>) {
        match self.template_indices.get(type_id).copied() {
            Some(index) => self.component_templates[index] = template,
            None => {
                self.template_indices
                    .insert(type_id, self.component_templates.len());
                self.component_templates.push(template);
            }
        }
    }

    /// Returns the canonical template of `type_id`, if there is one.
    #[inline]
    pub fn get_direct_erased_template(&self, type_id: TypeId) -> Option<&dyn ErasedTemplate> {
        let index = *self.template_indices.get(type_id)?;
        Some(&*self.component_templates[index])
    }

    /// Returns the canonical template of `type_id`, storing `default()` as that template if there is
    /// none yet.
    ///
    /// If the scene builds on a cached one ([`include_cached`](Self::include_cached)) and the cached
    /// scene describes that type, the cached template is cloned and becomes this scene's own — the
    /// copy-on-write that lets a patch edit a field of a template it does not own. Remembering it is
    /// what lets the application skip the cached copy later.
    pub fn get_or_insert_erased_template(
        &mut self,
        type_id: TypeId,
        default: impl FnOnce() -> Box<dyn ErasedTemplate>,
    ) -> &mut dyn ErasedTemplate {
        if let Some(&index) = self.template_indices.get(type_id) {
            return &mut *self.component_templates[index];
        }

        // The cached scene's copy wins over `default`: it is the description this scene builds on.
        let cloned = self
            .cached
            .as_ref()
            .and_then(|cached| cached.resolved.get_direct_erased_template(type_id))
            .map(ErasedTemplate::clone_template);

        let index = self.component_templates.len();
        self.template_indices.insert(type_id, index);
        match cloned {
            Some(template) => {
                self.component_templates.push(template);
                self.mark_duplicated(type_id);
            }
            None => self.component_templates.push(default()),
        }

        &mut *self.component_templates[index]
    }

    /// Returns the canonical template of `T`, storing `T::default()` as that template if there is
    /// none yet.
    ///
    /// If the scene builds on a cached one and the cached scene describes `T`, the cached template is
    /// cloned and becomes this scene's own; see
    /// [`get_or_insert_erased_template`](Self::get_or_insert_erased_template).
    ///
    /// # Panics
    ///
    /// Panics if the slot of `T`'s [`TypeId`] holds a template of another type, which cannot happen
    /// unless [`insert_erased_template`](Self::insert_erased_template) was called with a
    /// `type_id` that does not belong to the template.
    pub fn get_or_insert_template<T>(&mut self) -> &mut T
    where
        T: Template<Output: TemplateEffect> + Default + Send + Sync + 'static,
    {
        let erased =
            self.get_or_insert_erased_template(TypeId::of::<T>(), || Box::new(T::default()));

        (erased as &mut dyn Any)
            .downcast_mut::<T>()
            .expect("a template is stored under the `TypeId` of its own type")
    }
}

// -----------------------------------------------------------------------------
// References

impl ResolvedScene {
    /// Returns the names this scene declares.
    #[inline]
    pub fn entity_references(&self) -> &[EntityReference] {
        &self.entity_references
    }

    /// Declares `reference` as a name of this scene.
    ///
    /// Applying the scene binds the name to the entity it produced, so a template that mentions the
    /// name resolves to that entity. Declaring the same name twice is allowed; both resolve to the
    /// same entity.
    #[inline]
    pub fn add_entity_reference(&mut self, reference: EntityReference) {
        if !self.entity_references.contains(&reference) {
            self.entity_references.push(reference);
        }
    }
}

// -----------------------------------------------------------------------------
// Hierarchy

impl ResolvedScene {
    /// Returns the scenes that are spawned under this entity, in the order they were added.
    #[inline]
    pub fn children(&self) -> &[ResolvedScene] {
        &self.children
    }

    /// Returns the underlying child list.
    #[inline]
    pub(crate) fn children_mut(&mut self) -> &mut Vec<ResolvedScene> {
        &mut self.children
    }

    /// Appends `child` to the back of the children, which is where it will end up in [`Children`]
    /// as well.
    ///
    /// [`Children`]: zlim_core::query::Children
    #[inline]
    pub fn push_child(&mut self, child: ResolvedScene) {
        self.children.push(child);
    }

    /// Returns the explicit parent edge of this entity, if there is one.
    #[inline]
    pub fn parent(&self) -> Option<EntityTemplate> {
        self.parent
    }

    /// Sets the explicit parent edge of this entity, replacing the previous one.
    ///
    /// This is the parent the entity is *moved* under; the children of a scene are spawned with
    /// their parent already in place, so they do not need it.
    ///
    /// Applying the edge moves the entity with [`reparent_without_signal`] when the scene created
    /// it, and with [`reparent`] when the scene was applied to an entity that already existed: only
    /// the second case changes a place in the tree that the rest of the world has already seen.
    ///
    /// [`reparent_without_signal`]: zlim_core::ops::EntityOwned::reparent_without_signal
    /// [`reparent`]: zlim_core::ops::EntityOwned::reparent
    #[inline]
    pub fn set_parent(&mut self, parent: EntityTemplate) {
        self.parent = Some(parent);
    }
}
