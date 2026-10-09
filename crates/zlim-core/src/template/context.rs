use core::fmt::{Debug, Display, Formatter};
use core::hash::{Hash, Hasher};

use zlim_error::{ZlimError, ZlimResult};
use zlim_utils::hash::FixedState;
use zlim_utils::hash::HashMap;
use zlim_utils::hash::NoopState;

use crate::borrow::{Res, ResMut};
use crate::component::Components;
use crate::entity::{Entities, EntityId, EntityMap, EntityMapper};
use crate::ops::EntityOwned;
use crate::resource::Resource;
use crate::world::World;

// -----------------------------------------------------------------------------
// EntityReference

#[derive(Clone, Copy)]
#[repr(transparent)]
struct FileStr(&'static str);

impl PartialEq for FileStr {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        if core::ptr::eq::<str>(self.0, other.0) {
            true
        } else {
            self.0 == other.0
        }
    }
}

impl Eq for FileStr {}

/// A unique reference to a named entity of a scene.
///
/// A reference is what a `#Name` in a scene expands to. Its identity is made of
///
/// - the location of the macro invocation that produced it: file, line and column,
/// - the ordinal of the name within that invocation,
/// - a counter that is bumped for every run of the invocation.
///
/// so that two entities of the same name in different scenes — or in different runs of the same
/// scene — never resolve to each other. The hash of that identity is computed once, when the
/// reference is created, because [`EntityReferences`] looks references up through
/// [`NoopState`], which forwards the value a key hashes to instead of hashing it again.
#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct EntityReference {
    // SIMD comparison
    meta: [u64; 4],
    file: FileStr,
}

impl EntityReference {
    /// Creates a reference from the location of the invocation that produced it, the ordinal of the
    /// name within that invocation, and the counter of the current run.
    ///
    /// The location is what `file!()`, `line!()` and `column!()` expand to at the invocation.
    pub fn new(file: &'static str, line: u32, column: u32, name_id: usize, runtime: u64) -> Self {
        let mut hasher = FixedState::HASHER;
        file.hash(&mut hasher);
        hasher.write_u32(line);
        hasher.write_u32(column);
        hasher.write_u64(runtime);
        hasher.write_usize(name_id);
        Self {
            meta: [
                hasher.finish(),
                ((line as u64) << 32) + (column as u64),
                name_id as u64,
                runtime,
            ],
            file: FileStr(file),
        }
    }
}

impl Display for EntityReference {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{}:{}:{} #{} of run {}",
            self.file.0,
            self.meta[1] >> 32,
            self.meta[1] & (u32::MAX as u64),
            self.meta[2],
            self.meta[3],
        )
    }
}

impl Hash for EntityReference {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        // The identity is already hashed, and `NoopState` expects exactly one write.
        // See the type documentation for why the map is looked up this way.
        state.write_u64(self.meta[0]);
    }
}

impl Debug for EntityReference {
    /// Describes the identity of the reference, which is what a reader needs to
    /// tell two of them apart; the precomputed hash is not part of that identity.
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self, f)
    }
}

// -----------------------------------------------------------------------------
// EntityReferences

/// The named entities of a scene, and the entity each of them stands for.
///
/// A name is *declared* by whoever owns the entity it stands for — a scene binds its own names while
/// it is applied — and every template that mentions the same name then refers to the same entity.
/// Looking a name up before it is bound is an error rather than a new entity: an entity invented on
/// the spot would be one the scene never describes, and a component pointing at it would be pointing
/// at nothing.
///
/// A scene applied on its own owns its scope and drops it with the application; a list of roots
/// shares one scope, which is what lets one root name an entity another declares.
#[derive(Default)]
pub struct EntityReferences(HashMap<EntityReference, EntityId, NoopState>);

impl EntityReferences {
    /// Creates an empty set of references.
    #[inline]
    pub const fn new() -> Self {
        Self(HashMap::with_hasher(NoopState))
    }

    /// Returns the entity of the given reference, if it has been bound already.
    #[inline]
    pub fn get(&self, reference: EntityReference) -> Option<EntityId> {
        self.0.get(&reference).copied()
    }

    /// Binds the given reference to `entity`.
    #[inline]
    pub fn set(&mut self, reference: EntityReference, entity: EntityId) {
        self.0.insert(reference, entity);
    }

    /// Returns the number of references that have been bound.
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether no reference has been bound.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Debug for EntityReferences {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

// -----------------------------------------------------------------------------
// TemplateContext

/// A mapper for template entities that additionally checks whether the mapped
/// entity actually exists.
///
/// Unlike a plain entity map, [`TemplateEntityMapper::get_mapped`] verifies the
/// mapped entity against the world: if it does not exist, [`EntityId::PLACEHOLDER`]
/// is returned instead.
///
/// [`TemplateEntityMapper::get_mapped`]: EntityMapper::get_mapped
pub struct TemplateEntityMapper<'a> {
    entities: &'a Entities,
    raw_mapper: &'a mut EntityMap<EntityId>,
}

impl EntityMapper for TemplateEntityMapper<'_> {
    fn get_mapped(&mut self, source: EntityId) -> EntityId {
        let r = self.raw_mapper.get_mapped(source);
        if self.entities.contains(r) {
            r
        } else {
            EntityId::PLACEHOLDER
        }
    }

    fn set_mapped(&mut self, source: EntityId, target: EntityId) {
        self.raw_mapper.set_mapped(source, target);
    }
}

// -----------------------------------------------------------------------------
// TemplateContext

/// The context a [`Template`](super::Template) is built with.
///
/// It holds the entity the template is being applied to, the entity references of the scene it
/// belongs to, and the entities that scene describes.
///
/// The last two are the two ways a template can point at an entity, and they are different in kind:
///
/// - a *name* ([`EntityReferences`]) is written as a `#Name` in a scene, and an unknown one is an
///   error, because an entity invented on the spot would be one nothing describes;
/// - an *id* ([`EntityMap`]) is what a component carries after it was serialized by reflection — a
///   document id, meaningful only within the scene it came from. An id the scene does not declare is
///   kept when it names an entity that still exists, which is what lets a template that was never
///   part of a document (everything built in Rust) work unchanged; one that names nothing at all
///   becomes [`EntityId::PLACEHOLDER`] rather than a dangling reference.
pub struct TemplateContext<'a, 'w> {
    /// The entity the template is being applied to.
    pub entity: &'a mut EntityOwned<'w>,

    /// The entity references of the scene, used to resolve its named entities.
    pub references: &'a mut EntityReferences,

    /// The entities the scene describes, keyed by the id they carry in its document.
    pub raw_mapper: &'a mut EntityMap<EntityId>,
}

impl<'a, 'w> TemplateContext<'a, 'w> {
    /// Creates a context for the given entity, references and entities.
    #[inline]
    pub fn new(
        entity: &'a mut EntityOwned<'w>,
        references: &'a mut EntityReferences,
        raw_mapper: &'a mut EntityMap<EntityId>,
    ) -> Self {
        Self {
            entity,
            references,
            raw_mapper,
        }
    }

    /// Returns the entity the given reference stands for.
    ///
    /// A name belongs to the entity that declared it (see [`EntityReferences`]), so this is a lookup,
    /// not a way to make an entity: an unknown name is an error, because an entity invented here
    /// would be one nothing in the scene describes.
    ///
    /// The scene runtime binds every name of a scene — and of the entities it brings with it —
    /// before any template is built, so a template may point at a name that is declared later in the
    /// same scene.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn resolve_entity(&mut self, reference: EntityReference) -> ZlimResult<EntityId> {
        match self.references.get(reference) {
            Some(x) => Ok(x),
            None => {
                ::core::hint::cold_path();
                let msg = format!(
                    "no entity is named `{reference}`: a `#Name` has to be \
                    declared by the scene that owns the entity it stands for"
                );
                Err(ZlimError::error(msg))
            }
        }
    }

    /// Returns the [`TemplateEntityMapper`].
    ///
    /// An id that has not been registered is returned unchanged, so a template that
    /// was built in Rust rather than read from a document keeps naming whatever entity
    /// its id already names.
    ///
    /// After mapping, the resulting entity is checked for existence in the scene's
    /// world: if it does not exist, it is replaced with [`EntityId::PLACEHOLDER`].
    #[inline]
    pub fn entity_mapper(&mut self) -> TemplateEntityMapper<'_> {
        TemplateEntityMapper {
            entities: &self.entity.world().entities,
            raw_mapper: self.raw_mapper,
        }
    }

    /// Gets read-only access to the world that the current template belongs to.
    #[inline]
    pub fn world(&self) -> &World {
        self.entity.world()
    }

    /// Returns the cached Components information in this world.
    #[inline]
    pub fn components(&self) -> &Components {
        &self.entity.world().components
    }

    /// Returns `true` if the entity is currently spawned.
    #[inline]
    pub fn contains_entity(&self, entity: EntityId) -> bool {
        self.entity.world().contains_entity(entity)
    }

    /// Gets a reference to the resource of the given type if it exists
    #[inline]
    pub fn get_resource<R: Resource + Sync>(&self) -> Option<&R> {
        self.entity.get_resource()
    }

    /// Gets a reference with change detections to the resource of the given type if it exists
    #[inline]
    pub fn get_resource_ref<R: Resource + Sync>(&self) -> Option<Res<'_, R>> {
        self.entity.get_resource_ref()
    }

    /// Gets a mutable reference with change detections to the resource of the given type
    #[inline]
    pub fn get_resource_mut<R: Resource + Send>(&mut self) -> Option<ResMut<'_, R>> {
        self.entity.get_resource_mut()
    }

    /// Returns the resource of type `R`.
    ///
    /// # Panics
    ///
    /// Panics if the world does not have the resource.
    #[inline]
    pub fn resource<R: Resource + Sync>(&self) -> &R {
        self.entity.resource()
    }

    /// Returns a mutable reference to the resource of type `R`.
    ///
    /// # Panics
    ///
    /// Panics if the world does not have the resource.
    #[inline]
    pub fn resource_ref<R: Resource + Sync>(&mut self) -> Res<'_, R> {
        self.entity.resource_ref()
    }

    /// Returns a mutable reference to the resource of type `R`.
    ///
    /// # Panics
    ///
    /// Panics if the world does not have the resource.
    #[inline]
    pub fn resource_mut<R: Resource + Send>(&mut self) -> ResMut<'_, R> {
        self.entity.resource_mut()
    }
}
