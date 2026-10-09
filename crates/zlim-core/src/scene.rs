use core::fmt::{Debug, Formatter};
use std::collections::{BTreeMap, VecDeque};

use indexmap::IndexMap;
use serde::de::{DeserializeSeed, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserializer, Serialize, ser::SerializeMap};
use zlim_core::bundle::BundleScratch;
use zlim_core::component::ComponentDB;
use zlim_core::entity::{EntityError, EntityId, EntityMap, EntityMapper};
use zlim_core::template::{EntityReferences, ReflectTemplate, TemplateContext};
use zlim_core::world::World;
use zlim_error::{ZlimError, ZlimResult};
use zlim_reflect::Reflect;
use zlim_reflect::{TypeDB, serde::ReflectContext};
use zlim_utils::hash::SparseState;
use zlim_utils::serde::BorrowedStr;

// -----------------------------------------------------------------------------

/// A component type named by its type path, which is the key a document uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeIdent(pub &'static str);

impl Ord for TypeIdent {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0
            .len()
            .cmp(&other.0.len())
            .then_with(|| self.0.cmp(other.0))
    }
}

impl PartialOrd for TypeIdent {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// -----------------------------------------------------------------------------
// SceneEntityMapper

use std::hash::RandomState;

/// Allocates the entity ids a scene document leaves out, so one document can be loaded many times.
#[derive(Debug, Clone)]
pub struct SceneEntityMapper {
    allocator: u64,
    // `EntityMap` hashes sparsely: it keeps the low bits of the id as the slot
    // and does not usefully mix in the generation — `SparseHasher` says as much.
    // That assumes the index alone spreads entities out, which holds in a
    // `World` but not here: a document's ids are placeholders, and many of them
    // sharing one index would collide on every insert. A random state breaks
    // that up.
    mapper: std::collections::HashMap<EntityId, EntityId, RandomState>,
}

impl SceneEntityMapper {
    /// Creates a new empty SceneEntityMapper.
    pub fn new() -> Self {
        Self {
            allocator: 1, // NonZero
            mapper: std::collections::HashMap::new(),
        }
    }

    /// Reserves capacity for at least additional more elements.
    pub fn reserve(&mut self, additional: usize) {
        self.mapper.reserve(additional);
    }
}

impl Default for SceneEntityMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityMapper for SceneEntityMapper {
    fn get_mapped(&mut self, source: EntityId) -> EntityId {
        use std::collections::hash_map::Entry;
        match self.mapper.entry(source) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let id = EntityId::from_bits(self.allocator).unwrap();
                self.allocator += 1;
                *entry.insert(id)
            }
        }
    }

    fn set_mapped(&mut self, source: EntityId, target: EntityId) {
        self.mapper.insert(source, target);
    }
}

// -----------------------------------------------------------------------------
// DynamicScene & DynamicEntity & DynamicComponents

/// A scene read from a document, in the shape the document wrote it: every
/// entity it declared, flat.
///
/// This is what deserialization produces, and it is deliberately not a
/// description. The parent edges are [`EntityId`]s rather than an arrangement,
/// nothing has been grouped into trees, and the components are already
/// templates — reflection has turned each serialized value into a
/// [`ReflectTemplate`], so the type-level half of the work is done.
///
/// What remains is to arrange it. That is not this crate's step: a scene is
/// *resolved* by the crate that owns the resolved form, so a document and a
/// description written in code meet in one place. This type is the handover
/// between the two.
#[derive(Default, Debug, Clone)]
pub struct DynamicScene {
    pub entities: Vec<DynamicEntity>,
}

/// One entity of a [`DynamicScene`], with its components already templates.
pub struct DynamicEntity {
    pub id: EntityId,
    pub parent: Option<EntityId>,
    pub components: BTreeMap<TypeIdent, Box<dyn ReflectTemplate>>,
}

impl Debug for DynamicEntity {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        struct ComponentsDebug<'a>(&'a BTreeMap<TypeIdent, Box<dyn ReflectTemplate>>);

        impl Debug for ComponentsDebug<'_> {
            fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
                let mut debugger = f.debug_map();
                for (k, v) in self.0.iter() {
                    let reflect: &dyn Reflect = (**v).as_reflect();
                    debugger.entry(&k.0, &reflect);
                }
                debugger.finish()
            }
        }

        let mut debugger = f.debug_map();
        debugger.entry(&"id", &self.id);
        if let Some(parent) = self.parent {
            debugger.entry(&"parent", &parent);
        }
        debugger.entry(&"components", &ComponentsDebug(&self.components));
        debugger.finish()
    }
}

impl Clone for DynamicEntity {
    fn clone(&self) -> Self {
        let mut components = BTreeMap::new();
        for (k, v) in self.components.iter() {
            components.insert(*k, (**v).clone_reflect_template());
        }

        Self {
            id: self.id,
            parent: self.parent,
            components,
        }
    }
}

// -----------------------------------------------------------------------------
// DynamicScene Deserialize

// -------------------------------------------------------------
// DynamicComponents Deserialize

/// Reads the `components` map of one entity: each key names a component type.
pub struct ComponentsVisitor<'a> {
    pub mapper: &'a mut SceneEntityMapper,
    pub context: &'a dyn ReflectContext,
}

impl<'de> Visitor<'de> for ComponentsVisitor<'_> {
    type Value = BTreeMap<TypeIdent, Box<dyn ReflectTemplate>>;

    fn expecting(&self, f: &mut Formatter) -> core::fmt::Result {
        f.write_str("a map from type_path to reflect value")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut components = BTreeMap::new();

        while let Some(BorrowedStr(ty)) = map.next_key::<BorrowedStr>()? {
            let type_path: &str = &ty;
            let Some(component_db) = ComponentDB::get_by_path(type_path) else {
                ::core::hint::cold_path();
                return Err(serde::de::Error::custom(format!(
                    "missing component db for type `{type_path}`"
                )));
            };
            let Some(type_db) = component_db.type_db else {
                ::core::hint::cold_path();
                return Err(serde::de::Error::custom(format!(
                    "missing type db for component `{type_path}`, requires `#[component(reflect)]` attribute"
                )));
            };
            let Some(into_template) = component_db.into_template else {
                ::core::hint::cold_path();
                return Err(serde::de::Error::custom(format!(
                    "the component `{type_path}` cannot be deserialize, requires `#[component(serialize)]` attribute"
                )));
            };
            let seed = type_db.deserialize_seed(self.context);
            let mut value = map.next_value_seed(seed)?;

            if !component_db.no_entity {
                let Some(reflect) = component_db.reflect else {
                    ::core::hint::cold_path();
                    return Err(serde::de::Error::custom(format!(
                        "the component `{type_path}` is un-reflected, requires `#[component(reflect)]` attribute"
                    )));
                };
                (reflect.map_entities)(&mut *value, self.mapper);
            }

            let template = into_template(value);
            components.insert(TypeIdent(component_db.type_path), template);
        }

        Ok(components)
    }
}

impl<'de> DeserializeSeed<'de> for ComponentsVisitor<'_> {
    type Value = BTreeMap<TypeIdent, Box<dyn ReflectTemplate>>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

// -------------------------------------------------------------
// DynamicEntity Deserialize

/// Reads one entity of a scene document: its id, its parent, and its components.
pub struct EntityVisitor<'a> {
    pub mapper: &'a mut SceneEntityMapper,
    pub context: &'a dyn ReflectContext,
}

impl<'de> Visitor<'de> for EntityVisitor<'_> {
    type Value = DynamicEntity;

    fn expecting(&self, f: &mut Formatter) -> core::fmt::Result {
        f.write_str("a map with `id` and `components` fields")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut id: Option<EntityId> = None;
        let mut parent: Option<EntityId> = None;
        let mut components: Option<BTreeMap<TypeIdent, Box<dyn ReflectTemplate>>> = None;

        // Field keys. Adjust the strings to match your actual format.
        const ID: &str = "id";
        const PARENT: &str = "parent";
        const COMPONENTS: &str = "components";
        const EXPECTED: &[&str] = &[ID, PARENT, COMPONENTS];

        while let Some(BorrowedStr(key)) = map.next_key::<BorrowedStr>()? {
            match key.as_ref() {
                ID => {
                    if id.is_some() {
                        return Err(serde::de::Error::duplicate_field("id"));
                    }
                    id = Some(map.next_value::<EntityId>()?);
                }
                PARENT => {
                    if parent.is_some() {
                        return Err(serde::de::Error::duplicate_field("parent"));
                    }
                    parent = Some(map.next_value::<EntityId>()?);
                }
                COMPONENTS => {
                    if components.is_some() {
                        return Err(serde::de::Error::duplicate_field("components"));
                    }
                    let seed = ComponentsVisitor {
                        mapper: self.mapper,
                        context: self.context,
                    };
                    components = Some(map.next_value_seed(seed)?);
                }
                other => {
                    ::core::hint::cold_path();
                    return Err(serde::de::Error::unknown_field(other, EXPECTED));
                }
            }
        }

        let Some(id) = id else {
            return Err(serde::de::Error::missing_field(ID));
        };
        let Some(components) = components else {
            return Err(serde::de::Error::missing_field(COMPONENTS));
        };

        let id = self.mapper.get_mapped(id);
        if let Some(parent) = parent.as_mut() {
            *parent = self.mapper.get_mapped(*parent);
        };

        Ok(DynamicEntity {
            id,
            parent,
            components,
        })
    }
}

impl<'de> DeserializeSeed<'de> for EntityVisitor<'_> {
    type Value = DynamicEntity;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

// -------------------------------------------------------------
// DynamicEntities Deserialize

/// Reads the `entities` sequence of a scene document.
pub struct EntitiesVisitor<'a> {
    pub mapper: &'a mut SceneEntityMapper,
    pub context: &'a dyn ReflectContext,
}

impl<'de> Visitor<'de> for EntitiesVisitor<'_> {
    type Value = Vec<DynamicEntity>;

    fn expecting(&self, f: &mut Formatter) -> core::fmt::Result {
        f.write_str("a sequence of entities")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let hint = seq.size_hint().unwrap_or(0);
        let mut entities = Vec::with_capacity(hint);

        loop {
            let seed = EntityVisitor {
                mapper: self.mapper,
                context: self.context,
            };
            match seq.next_element_seed(seed)? {
                Some(item) => entities.push(item),
                None => break,
            }
        }

        Ok(entities)
    }
}

impl<'de> DeserializeSeed<'de> for EntitiesVisitor<'_> {
    type Value = Vec<DynamicEntity>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(self)
    }
}

// -------------------------------------------------------------
// DynamicScene Deserialize

/// Reads a whole scene document into a [`DynamicScene`].
pub struct SceneVisitor<'a> {
    pub mapper: &'a mut SceneEntityMapper,
    pub context: &'a dyn ReflectContext,
}

impl<'de> Visitor<'de> for SceneVisitor<'_> {
    type Value = DynamicScene;

    fn expecting(&self, f: &mut Formatter) -> core::fmt::Result {
        f.write_str("a map with an `entities` field")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        const ENTITIES: &str = "entities";
        const EXPECTED: &[&str] = &[ENTITIES];

        let mut entities: Option<Vec<DynamicEntity>> = None;

        while let Some(BorrowedStr(key)) = map.next_key::<BorrowedStr>()? {
            match key.as_ref() {
                ENTITIES => {
                    if entities.is_some() {
                        return Err(serde::de::Error::duplicate_field(ENTITIES));
                    }
                    let seed = EntitiesVisitor {
                        mapper: self.mapper,
                        context: self.context,
                    };
                    entities = Some(map.next_value_seed(seed)?);
                }
                other => {
                    ::core::hint::cold_path();
                    return Err(serde::de::Error::unknown_field(other, EXPECTED));
                }
            }
        }

        let Some(entities) = entities else {
            return Err(serde::de::Error::missing_field(ENTITIES));
        };

        Ok(DynamicScene { entities })
    }
}

impl<'de> DeserializeSeed<'de> for SceneVisitor<'_> {
    type Value = DynamicScene;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

// -----------------------------------------------------------------------------
// DynamicScene Serialize

// -------------------------------------------------------------
// DynamicComponents Serialize

/// Serializes the components of one [`DynamicEntity`].
pub struct ComponentsSerial<'a> {
    pub context: &'a dyn ReflectContext,
    pub components: &'a BTreeMap<TypeIdent, Box<dyn ReflectTemplate>>,
}

impl Serialize for ComponentsSerial<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.components.len()))?;

        for (k, v) in self.components.iter() {
            let reflect = ReflectTemplate::as_reflect(&**v);
            map.serialize_entry(k.0, &TypeDB::serialize_data(reflect, self.context))?;
        }

        map.end()
    }
}

// -------------------------------------------------------------
// DynamicEntity Serialize

/// Serializes one [`DynamicEntity`] of a [`DynamicScene`].
pub struct EntitySerial<'a> {
    pub context: &'a dyn ReflectContext,
    pub entity: &'a DynamicEntity,
}

impl Serialize for EntitySerial<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let hint = 2 + (self.entity.parent.is_some() as usize);
        let mut map = serializer.serialize_map(Some(hint))?;

        map.serialize_entry("id", &self.entity.id)?;

        if let Some(parent) = self.entity.parent {
            map.serialize_entry("parent", &parent)?;
        }

        let components = ComponentsSerial {
            context: self.context,
            components: &self.entity.components,
        };
        map.serialize_entry("components", &components)?;

        map.end()
    }
}

// -------------------------------------------------------------
// DynamicEntities Serialize

/// Serializes the entities of a [`DynamicScene`].
pub struct EntitiesSerial<'a> {
    pub context: &'a dyn ReflectContext,
    pub entities: &'a [DynamicEntity],
}

impl Serialize for EntitiesSerial<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let hint = self.entities.len();
        let mut seq = serializer.serialize_seq(Some(hint))?;

        for entity in self.entities {
            let driver = EntitySerial {
                context: self.context,
                entity,
            };
            seq.serialize_element(&driver)?;
        }

        seq.end()
    }
}

// -------------------------------------------------------------
// DynamicScene Serialize

/// The document-shaped serializer for a [`DynamicScene`].
pub struct SceneSerial<'a> {
    pub context: &'a dyn ReflectContext,
    pub scene: &'a DynamicScene,
}

impl Serialize for SceneSerial<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;

        let driver = EntitiesSerial {
            context: self.context,
            entities: &self.scene.entities,
        };

        map.serialize_entry("entities", &driver)?;

        map.end()
    }
}

// -----------------------------------------------------------------------------
// DynamicScene & DynamicEntity & DynamicComponents

/// A scene whose components are read straight out of a live [`World`], with no
/// copy.
///
/// This is the writing half of the document format, and the counterpart of
/// [`DynamicScene`]: where that one holds templates reflection built while
/// reading, this one holds shared references into the world that are still
/// there. Nothing is cloned and nothing is owned, which is what lets a large
/// world be written out without first being copied into a scene.
///
/// It borrows the world for as long as it lives, so it is built by
/// [`BorrowedSceneBuilder`] and serialized straight away; it is not a value to
/// keep. Because it only borrows, reading is done through the same reflection
/// hook a `DynamicScene` would have used — see the `Serialize` impl, which
/// hands each component to `TypeDB` as a `&dyn Reflect`.
///
/// [`World`]: crate::world::World
#[derive(Clone)]
pub struct BorrowedScene<'a> {
    pub world: &'a World,
    pub entities: Vec<BorrowedEntity<'a>>,
}

/// One entity of a [`BorrowedScene`]: its id, its parent edge, and the components it holds.
#[derive(Clone)]
pub struct BorrowedEntity<'a> {
    pub id: EntityId,
    pub parent: Option<EntityId>,
    pub components: BTreeMap<TypeIdent, &'a dyn Reflect>,
}

impl Debug for BorrowedScene<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BorrowedScene")
            .field("world_id", &self.world.id())
            .field("entities", &self.entities)
            .finish()
    }
}

impl Debug for BorrowedEntity<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        struct ComponentsDebug<'a>(&'a BTreeMap<TypeIdent, &'a dyn Reflect>);
        impl Debug for ComponentsDebug<'_> {
            fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
                let mut debugger = f.debug_map();
                for (k, v) in self.0.iter() {
                    debugger.entry(&k.0, v);
                }
                debugger.finish()
            }
        }

        let mut debugger = f.debug_map();
        debugger.entry(&"id", &self.id);
        if let Some(parent) = self.parent {
            debugger.entry(&"parent", &parent);
        }
        debugger.entry(&"components", &ComponentsDebug(&self.components));
        debugger.finish()
    }
}

// -----------------------------------------------------------------------------
// BorrowedScene Serialize

// -------------------------------------------------------------
// BorrowedComponents Serialize

/// Serializes the components of one entity of a [`BorrowedScene`].
pub struct BorrowedComponentsSerial<'a, 'w> {
    pub context: &'a dyn ReflectContext,
    pub components: &'a BTreeMap<TypeIdent, &'w dyn Reflect>,
}

impl Serialize for BorrowedComponentsSerial<'_, '_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.components.len()))?;

        for (k, v) in self.components.iter() {
            map.serialize_entry(k.0, &TypeDB::serialize_data(*v, self.context))?;
        }

        map.end()
    }
}

// -------------------------------------------------------------
// DynamicEntity Serialize

/// Serializes one entity of a [`BorrowedScene`].
pub struct BorrowedEntitySerial<'a, 'w> {
    pub context: &'a dyn ReflectContext,
    pub entity: &'a BorrowedEntity<'w>,
}

impl Serialize for BorrowedEntitySerial<'_, '_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let hint = 2 + (self.entity.parent.is_some() as usize);
        let mut map = serializer.serialize_map(Some(hint))?;

        map.serialize_entry("id", &self.entity.id)?;

        if let Some(parent) = self.entity.parent {
            map.serialize_entry("parent", &parent)?;
        }

        let components = BorrowedComponentsSerial {
            context: self.context,
            components: &self.entity.components,
        };
        map.serialize_entry("components", &components)?;

        map.end()
    }
}

// -------------------------------------------------------------
// DynamicEntities Serialize

/// Serializes the entities of a [`BorrowedScene`].
pub struct BorrowedEntitiesSerial<'a, 'w> {
    pub context: &'a dyn ReflectContext,
    pub entities: &'a [BorrowedEntity<'w>],
}

impl Serialize for BorrowedEntitiesSerial<'_, '_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let hint = self.entities.len();
        let mut seq = serializer.serialize_seq(Some(hint))?;

        for entity in self.entities {
            let driver = BorrowedEntitySerial {
                context: self.context,
                entity,
            };
            seq.serialize_element(&driver)?;
        }

        seq.end()
    }
}

// -------------------------------------------------------------
// DynamicScene Serialize

/// The document-shaped serializer for a [`BorrowedScene`].
pub struct BorrowedSceneSerial<'a, 'w> {
    pub context: &'a dyn ReflectContext,
    pub scene: &'a BorrowedScene<'w>,
}

impl Serialize for BorrowedSceneSerial<'_, '_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;

        let driver = BorrowedEntitiesSerial {
            context: self.context,
            entities: &self.scene.entities,
        };

        map.serialize_entry("entities", &driver)?;

        map.end()
    }
}

// -----------------------------------------------------------------------------
// BorrowedScene Builder

enum SceneMode {
    All,
    With(EntityId),
    WithRecursive(EntityId),
    Without(EntityId),
    WithoutRecursive(EntityId),
}

/// Collects which entities of a world to serialize, and in what shape.
///
/// Every method records a *request* — `with`, `without`, and their `_recursive`
/// forms — and [`finish`](Self::finish) applies them all.
///
/// # Requests are applied in order
///
/// The requests are not a set; they are a sequence, and a later one wins over an
/// earlier one:
///
/// ```rust, ignore
/// // `child` ends up included: the `with_recursive` came
/// // after the `without` and put the whole subtree back.
/// world.borrowed_scene_builder()
///     .without_recursive(parent)
///     .with_recursive(parent)
/// ```
///
/// The same holds the other way round — a `without` after a `with` takes the
/// entity back out — so a request never depends on how the earlier ones were
/// grouped, only on their order.
///
/// Being a sequence is what makes the builder enough on its own: there is no
/// separate "removals" pass that a later inclusion has to outrank.
///
/// [`with`]: Self::with
/// [`without`]: Self::without
pub struct BorrowedSceneBuilder<'w> {
    world: &'w World,
    entities: Vec<SceneMode>,
    skip_missing: bool,
}

impl World {
    /// Creates a empty [`BorrowedSceneBuilder`] that borrows data from this world.
    ///
    /// The builder assembles the scene to be serialized using only references
    /// into the `World`, avoiding any deep copy of the underlying data.
    #[inline]
    #[doc(alias = "scene_builder")]
    pub fn borrowed_scene_builder(&self) -> BorrowedSceneBuilder<'_> {
        BorrowedSceneBuilder::new(self)
    }
}

impl<'w> BorrowedSceneBuilder<'w> {
    /// Creates a builder that reads from `world`.
    #[inline]
    pub fn new(world: &'w World) -> Self {
        BorrowedSceneBuilder {
            world,
            entities: Vec::new(),
            skip_missing: false,
        }
    }

    /// Skips an entity that is gone instead of failing.
    #[inline]
    pub fn skip_missing(mut self) -> Self {
        self.skip_missing = true;
        self
    }

    #[inline]
    /// Requests that `id` be included, without its children.
    pub fn with_all(mut self) -> Self {
        self.entities.push(SceneMode::All);
        self
    }

    #[inline]
    /// Requests that `id` be included, without its children.
    pub fn with(mut self, id: EntityId) -> Self {
        self.entities.push(SceneMode::With(id));
        self
    }

    /// Requests that `id` and everything under it be included.
    #[inline]
    pub fn with_recursive(mut self, id: EntityId) -> Self {
        self.entities.push(SceneMode::WithRecursive(id));
        self
    }

    /// Requests that `id` be left out, leaving its children in place.
    #[inline]
    pub fn without(mut self, id: EntityId) -> Self {
        self.entities.push(SceneMode::Without(id));
        self
    }

    /// Requests that `id` and everything under it be left out.
    #[inline]
    pub fn without_recursive(mut self, id: EntityId) -> Self {
        self.entities.push(SceneMode::WithoutRecursive(id));
        self
    }

    /// Applies every request, in the order they were made, and returns the scene.
    #[inline(never)]
    pub fn finish(self) -> Result<BorrowedScene<'w>, EntityError> {
        let world = self.world;
        let entities = world.entities();
        let components = world.components();

        // Use `IndexMap` to maintain order stability.
        let mut graph =
            IndexMap::<EntityId, Option<EntityId>, SparseState>::with_hasher(SparseState);

        for mode in self.entities {
            match mode {
                SceneMode::All => {
                    let mut pending: VecDeque<EntityId> = VecDeque::new();
                    entities
                        .root_entities()
                        .for_each(|id| pending.push_back(id));
                    while let Some(child) = pending.pop_front() {
                        let sub_info = entities
                            .get(child)
                            .expect("the entity tree must be correct");
                        pending.extend(sub_info.children.as_slice());
                        graph.insert(child, sub_info.parent);
                    }
                }
                SceneMode::With(id) => match entities.get(id) {
                    Ok(info) => {
                        graph.insert(id, info.parent);
                    }
                    Err(e) if !self.skip_missing => return Err(e),
                    _ => {}
                },
                SceneMode::WithRecursive(id) => {
                    let info = match entities.get(id) {
                        Ok(info) => info,
                        Err(e) if !self.skip_missing => return Err(e),
                        _ => continue,
                    };
                    graph.insert(id, info.parent);
                    // VecDeque::from(Vec) is fast and no additional memory allocation required
                    let mut pending: VecDeque<EntityId> = VecDeque::from(info.children.clone());
                    while let Some(child) = pending.pop_front() {
                        let sub_info = entities
                            .get(child)
                            .expect("the entity tree must be correct");
                        pending.extend(sub_info.children.as_slice());
                        graph.insert(child, sub_info.parent);
                    }
                }
                SceneMode::Without(id) => {
                    graph.swap_remove(&id);
                }
                SceneMode::WithoutRecursive(id) => {
                    let info = match entities.get(id) {
                        Ok(info) => info,
                        Err(_) => continue,
                    };
                    graph.swap_remove(&id);
                    // VecDeque::from(Vec) is fast and no additional memory allocation required
                    let mut pending: VecDeque<EntityId> = VecDeque::from(info.children.clone());
                    while let Some(child) = pending.pop_front() {
                        let sub_info = entities
                            .get(child)
                            .expect("the entity tree must be correct");
                        pending.extend(sub_info.children.as_slice());
                        graph.swap_remove(&child);
                    }
                }
            }
        }

        let mut borrowed = BorrowedScene::<'w> {
            world,
            entities: Vec::with_capacity(graph.len()),
        };

        let mut roots = Vec::<EntityId>::with_capacity(graph.len() >> 1);

        graph.iter().for_each(|(key, val)| {
            if let Some(val) = val
                && graph.contains_key(val)
            {
                return;
            }
            roots.push(*key);
        });

        #[cold]
        #[inline(never)]
        fn unreachable_err(db: &ComponentDB) -> ! {
            panic!("The component `{db:?}` annotated `serialize` does not impl reflect")
        }

        roots.iter().for_each(|id| {
            let mut stack = Vec::<(EntityId, Option<EntityId>)>::new();
            stack.push((*id, None));

            while let Some((id, parent)) = stack.pop() {
                if graph.swap_remove(&id).is_none() {
                    continue;
                }

                let entity_ref = world.entity_ref(id);

                let mut borrowed_entity = BorrowedEntity::<'w> {
                    id,
                    parent,
                    components: BTreeMap::new(),
                };
                for &cid in entity_ref.components().iter() {
                    let db = components.get_by_id(cid);
                    if !db.serialize {
                        continue;
                    }
                    // #[component(serialize)] will register `into_template`.
                    debug_assert!(db.into_template.is_some());

                    let Some(type_db) = db.type_db else {
                        unreachable_err(db);
                    };
                    let ptr = entity_ref.get_by_id(cid).expect("should exists");

                    #[expect(unsafe_code, reason = "faster than EntityRef::get_reflect_by_id")]
                    let reflect = unsafe { type_db.reflect_from_ptr(ptr) };

                    borrowed_entity
                        .components
                        .insert(TypeIdent(db.type_path), reflect);
                }

                borrowed.entities.push(borrowed_entity);

                let node = entities.get(id).expect("checked above");

                // rev: Stack is tail first
                node.children.iter().rev().for_each(|&child| {
                    stack.push((child, Some(id)));
                });
            }
        });

        Ok(borrowed)
    }
}

// -----------------------------------------------------------------------------

impl BorrowedScene<'_> {
    /// Convert a [`BorrowedScene`] to a [`DynamicScene`] and remap the EntityId.
    ///
    /// This can make the serialized result more stable.
    pub fn into_dynamic(self) -> DynamicScene {
        let components = &self.world.components;
        let mut mapper = SceneEntityMapper::new();
        let mut dynamic = DynamicScene {
            entities: Vec::new(),
        };

        mapper.reserve(self.entities.len() + (self.entities.len() >> 1));
        // Collect entity IDs in advance to ensure overall stability.
        // (The order of entity mapping for certain components is unstable.)
        for entity in &self.entities {
            let _ = mapper.get_mapped(entity.id);
        }

        for entity in &self.entities {
            let id = mapper.get_mapped(entity.id);
            let parent = entity.parent.map(|p| mapper.get_mapped(p));
            let mut dynamic_entity = DynamicEntity {
                id,
                parent,
                components: BTreeMap::new(),
            };
            for (k, &v) in entity.components.iter() {
                let db = components
                    .get_by_type(v.type_id())
                    .or_else(|| components.get_by_path(k.0))
                    .expect("the component in BorrowedScene must be registered");
                let mut cloned = v
                    .reflect_clone()
                    .expect("the component in BorrowedScene must cloneable");
                if !db.no_entity {
                    let reflect = db
                        .reflect
                        .expect("the component in BorrowedScene must be reflected");
                    (reflect.map_entities)(&mut *cloned, &mut mapper);
                }
                let into_template = db.into_template.expect(
                    "the component in BorrowedScene must be serializable (#[component(serialize)])",
                );
                dynamic_entity.components.insert(*k, into_template(cloned));
            }
            dynamic.entities.push(dynamic_entity);
        }

        dynamic
    }
}

impl From<BorrowedScene<'_>> for DynamicScene {
    fn from(value: BorrowedScene<'_>) -> Self {
        value.into_dynamic()
    }
}

// -----------------------------------------------------------------------------

impl World {
    /// Spawns every entity of `scene` under `parent`, components and all.
    ///
    /// A [`DynamicScene`] needs no planning: each entity it describes is exactly one entity, in a
    /// shape already decided, so this is two passes and nothing more. The first spawns every empty
    /// entity and records which one each document id became; the second fills them in, moving each
    /// component out of its template straight into the entity.
    ///
    /// The order the document lists its entities in is not the order they are spawned in. A `parent`
    /// names an id, and the entity that id belongs to may be declared *after* the one naming it, so
    /// the roots are spawned first and each parent before its children. Without that, an empty entity
    /// could not be spawned with the parent it asks for.
    ///
    /// # Errors
    ///
    /// Returns [`EntityError`] if `parent` is not a live entity, and an error if a component cannot be
    /// applied.
    ///
    /// A failure leaves nothing behind. The entities spawned here are despawned again, deepest first,
    /// and a component that was built but never written is dropped: the caller only learns of the
    /// failure through the error, so it would have no way to name either.
    ///
    /// Only the first of those two errors is reachable today: a component is applied by moving it into
    /// the entity, and every step of that returns `Ok`. The second is kept because it is part of what
    /// the caller is promised, and a promise that lapses the first time a component learns to fail is
    /// not one worth making. Nothing has to change here when that happens.
    pub fn spawn_dynamic_scene(
        &mut self,
        scene: DynamicScene,
        parent: Option<EntityId>,
    ) -> ZlimResult<()> {
        /// Despawns `entities`, deepest first.
        #[cold]
        #[inline(never)]
        fn drop_spawned(world: &mut World, entities: &[EntityId]) {
            for &id in entities.iter().rev() {
                world.try_despawn(id);
            }
        }

        // A parent that does not exist is refused before anything is spawned, so a bad call changes
        // nothing at all.
        if let Some(parent) = parent {
            self.entities().get(parent).map_err(ZlimError::error)?;
        }

        let mut dynamic = scene.entities;

        // `EntityMap` hashes sparsely, which assumes the index alone spreads the
        // ids out — the reason `SceneEntityMapper` cannot use it. Here it holds:
        // reading a document runs every id through `SceneEntityMapper`, so the
        // ids in memory come from its counter and are uniform, rather than the
        // placeholders a document may hold.
        let mut id_map = EntityMap::with_capacity(dynamic.len());
        let mut spawned: Vec<EntityId> = Vec::with_capacity(dynamic.len());
        let mut world_of: Vec<Option<EntityId>> = vec![None; dynamic.len()];

        // Where each document id sits in the list, which is how a `parent` naming an id is turned
        // into the entity that id became.
        let mut index_of: EntityMap<usize> = EntityMap::with_capacity(dynamic.len());
        for (index, entity) in dynamic.iter().enumerate() {
            index_of.insert(entity.id, index);
        }

        // Depth from the nearest root, walked from the roots down. A parent always sits at a smaller
        // depth than its children, so sorting by depth spawns a parent before anything hanging off it.
        //
        // An id the document does not declare, and an entity that names itself, are both treated as
        // "no parent here" and make the entity a root. A document read back from a file cannot
        // produce either, so they only matter for a hand-built scene.
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); dynamic.len()];
        let mut depth: Vec<usize> = vec![usize::MAX; dynamic.len()];
        let mut stack: Vec<(usize, usize)> = Vec::new();

        for (index, entity) in dynamic.iter().enumerate() {
            if let Some(p) = entity.parent
                && let Some(parent) = index_of.get(p).copied()
                && parent != index
            {
                children[parent].push(index);
            } else {
                stack.push((index, 0));
            }
        }

        while let Some((index, at)) = stack.pop() {
            if depth[index] <= at {
                continue;
            }

            depth[index] = at;
            for &child in &children[index] {
                stack.push((child, at + 1));
            }
        }

        // --- 1: every entity, empty ---
        //
        // The whole list is spawned before a single component is written, because a component of one
        // entity may point at another — including one the document declares later. Every id is bound
        // by the time the second pass needs it.
        let mut order: Vec<usize> = (0..dynamic.len()).collect();
        order.sort_by_key(|&index| depth[index]);
        for index in order {
            let parent = dynamic[index]
                .parent
                .and_then(|parent| world_of[index_of[parent]])
                .or(parent);

            let id = match self.try_spawn_empty(parent) {
                Ok(id) => id.id(),
                Err(error) => {
                    drop_spawned(self, &spawned);
                    return Err(ZlimError::error(error));
                }
            };

            spawned.push(id);
            world_of[index] = Some(id);
            id_map.insert(dynamic[index].id, id);
        }

        // --- 2: the components ---
        //
        // A dynamic scene says nothing about parenting beyond the id each entity carries, so the edge
        // is whatever `try_spawn_empty` was given and there is nothing left to resolve here.
        let mut references = EntityReferences::new();
        let mut scratch = BundleScratch::new();

        for (index, entity) in dynamic.drain(..).enumerate() {
            // `world_of` is indexed by position in the document, which is the order this loop walks;
            // `spawned` is in spawn order, and is only used to take everything back.
            let target = world_of[index].expect("every entity was spawned in the first pass");

            let result = (|| -> ZlimResult<()> {
                let mut owned = self.get_entity_owned(target).map_err(ZlimError::error)?;

                let mut writer = scratch.writer();
                let mut context = TemplateContext::new(&mut owned, &mut references, &mut id_map);

                for template in entity.components.into_values() {
                    // `apply_owned` is what makes this cheap: `ComponentTemplate` moves the component
                    // out of the template instead of cloning it, remaps the entities the component
                    // carries through `context`, and pushes it into the writer directly.
                    template.apply_owned(&mut context, &mut writer)?;
                }

                // `write` clears the scratch either way, so nothing is left in it after this returns.
                writer.write(&mut *context.entity).map_err(ZlimError::error)
            })();

            if let Err(error) = result {
                // Nothing here can fail today — the entity was spawned a moment ago and every
                // `apply_owned` returns `Ok` — so this branch is reached only by a future one that
                // can. It stays because the guarantee below is what callers rely on, and a guarantee
                // that lapses the first time a component learns to fail is not one.
                //
                // A template that failed partway left what it had already pushed in the scratch space,
                // where nothing would ever drop it: the write that hands components over never ran.
                // `write` keeps the scratch for reuse, so this only holds the current entity's
                // leftovers. The registry is read here rather than earlier so that its borrow ends
                // before `drop_spawned` takes `&mut World`.
                if !scratch.is_empty() {
                    scratch.manual_drop(self.components());
                }
                drop_spawned(self, &spawned);
                return Err(error);
            }
        }

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use zlim_core_derive::Component;
    use zlim_reflect::derive::{Reflect, TypePath};

    use super::{BorrowedSceneSerial, SceneSerial};
    use crate::entity::EntityId;
    use crate::world::World;

    #[derive(Debug, Clone, TypePath, Reflect, Component)]
    #[reflect(Clone, Debug)]
    #[component(reflect, serialize)]
    pub struct RawRefelct(String);

    #[derive(Debug, Clone, TypePath, Reflect, Component)]
    #[derive(Serialize, Deserialize)]
    #[reflect(Clone, Debug, Serialize, Deserialize)]
    #[component(reflect, serialize)]
    pub struct Name(String);

    #[derive(Debug, Clone, TypePath, Reflect, Component)]
    #[derive(Serialize, Deserialize)]
    #[reflect(Clone, Debug, Serialize, Deserialize)]
    #[component(reflect, serialize)]
    pub struct Transform {
        pub x: f32,
        pub y: f32,
    }

    #[derive(Debug, Clone, TypePath, Reflect, Component)]
    #[derive(Serialize, Deserialize)]
    #[reflect(Clone, Debug, Serialize, Deserialize)]
    #[component(reflect, serialize)]
    pub struct Target {
        #[entities]
        pub entity: EntityId,
    }

    #[derive(Debug, Clone, TypePath, Reflect, Component)]
    #[derive(Serialize, Deserialize)]
    #[reflect(Clone, Debug, Serialize, Deserialize)]
    #[component(reflect, serialize)]
    pub struct RelationShipTarget {
        #[entities]
        pub sources: Vec<EntityId>,
    }

    #[derive(Debug, Clone, Component)]
    pub struct SkipA {
        _skip: &'static str,
    }

    #[derive(Debug, Clone, Component, TypePath, Reflect)]
    #[component(reflect)]
    pub struct SkipB(String);

    fn build_world() -> Box<World> {
        let mut world = World::alloc();

        let mut root = world.spawn(
            (
                Name("root".into()),
                Transform { x: 1.0, y: 2.0 },
                SkipA { _skip: "SkipA" },
                SkipB("SkipB".into()),
            ),
            None,
        );

        root.with_child(Name("child".into())).unwrap();

        let root_id = root.id;

        let target_id = world.spawn(Target { entity: root_id }, None).id;

        let mut relation = world.spawn(
            (
                RelationShipTarget {
                    sources: vec![target_id, root_id],
                },
                RawRefelct("raw".into()),
            ),
            None,
        );

        relation.with_child(SkipB("SkipB".into())).unwrap();

        world
    }

    #[test]
    #[ignore = "manual trigger"]
    #[expect(
        clippy::print_stderr,
        reason = "the RON this prints is the whole point of the manual test"
    )]
    fn print_scene_ron() {
        let world = build_world();

        let borrowed = world.borrowed_scene_builder().with_all().finish().unwrap();

        let borrowed_serial = BorrowedSceneSerial {
            context: &(),
            scene: &borrowed,
        };

        let ron = ron::ser::to_string_pretty(&borrowed_serial, Default::default()).unwrap();
        std::eprintln!("--- BorrowedScene RON ---\n{ron}");

        let dynamic = borrowed.into_dynamic();

        let dynamic_serial = SceneSerial {
            context: &(),
            scene: &dynamic,
        };
        let ron = ron::ser::to_string_pretty(&dynamic_serial, Default::default()).unwrap();
        std::eprintln!("--- DynamicScene RON ---\n{ron}");
    }

    /// Reads the built world into a `DynamicScene` and spawns it into a fresh one.
    fn rebuild() -> (Box<World>, Box<World>, Vec<EntityId>) {
        let source = build_world();
        let dynamic = source
            .borrowed_scene_builder()
            .with_all()
            .finish()
            .unwrap()
            .into_dynamic();
        let document_ids = dynamic.entities.iter().map(|entity| entity.id).collect();

        let mut target = World::alloc();
        target
            .spawn_dynamic_scene(dynamic, None)
            .expect("the document spawns");

        (source, target, document_ids)
    }

    /// Every entity the call spawned, depth first from the roots.
    fn all_entities(world: &World) -> Vec<EntityId> {
        let mut out = Vec::new();
        let mut stack: Vec<EntityId> = world.entities().root_entities().collect();
        while let Some(id) = stack.pop() {
            out.push(id);
            // The node carries the children directly; `EntityRef` does not expose the tree.
            if let Ok(node) = world.entities().get(id) {
                stack.extend_from_slice(&node.children);
            }
        }
        out
    }

    /// Every entity of the document is spawned, with the components it carried.
    #[test]
    fn a_dynamic_scene_spawns_every_entity() {
        let (source, target, document_ids) = rebuild();

        assert_eq!(
            target.entity_count(),
            source.entity_count(),
            "the same entities, no more and no fewer",
        );

        let mut names = all_entities(&target)
            .into_iter()
            .filter_map(|id| target.entity_ref(id).get::<Name>().map(|n| n.0.clone()))
            .collect::<Vec<_>>();

        names.sort();

        assert_eq!(names, vec!["child".to_string(), "root".to_string()]);

        // Every entity the document declared exists in the target, one for one.
        assert_eq!(document_ids.len(), target.entity_count());
    }

    /// A parent that does not exist is refused, and nothing is spawned.
    #[test]
    fn a_missing_parent_is_refused() {
        let source = build_world();
        let dynamic = source
            .borrowed_scene_builder()
            .with_all()
            .finish()
            .unwrap()
            .into_dynamic();

        let mut target = World::alloc();
        let ghost = target.spawn_empty(None).id();
        target.despawn(ghost).unwrap();

        let _ = target
            .spawn_dynamic_scene(dynamic, Some(ghost))
            .expect_err("a despawned parent is not a parent");

        assert_eq!(target.entity_count(), 0, "nothing was spawned");
    }
}
