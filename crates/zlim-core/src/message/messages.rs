//! Per-world registry of message types and their queue metadata.

use core::any::TypeId;
use core::fmt::Debug;

use zlim_utils::debug::DebugName;
use zlim_utils::ext::TypeMap;

use super::{Message, MessageId, MessageQueue};
use crate::resource::{ResourceDB, ResourceId};
use crate::world::World;

// -----------------------------------------------------------------------------
// MessageMeta

/// Compact runtime metadata for a registered message type.
///
/// This struct stores the stable [`MessageId`], the debug-friendly type name,
/// the [`TypeId`] used for lookups, the [`ResourceId`] of the backing
/// `MessageQueue<T>` resource, and a function pointer used to update (rotate)
/// the queue once per update (see [`MessageQueue::update`]).
///
/// [`MessageId`]: crate::message::MessageId
/// [`ResourceId`]: crate::resource::ResourceId
/// [`MessageQueue::update`]: crate::message::MessageQueue::update
pub struct MessageMeta {
    id: MessageId,
    resource_id: ResourceId,
    update: fn(&mut World),
    name: DebugName,
}

impl Debug for MessageMeta {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Message")
            .field("id", &self.id)
            .field("name", &self.name)
            .finish()
    }
}

impl MessageMeta {
    /// Returns the message's unique ID.
    #[inline(always)]
    pub fn id(&self) -> MessageId {
        self.id
    }

    /// Returns the message's [`DebugName`].
    ///
    /// [`DebugName`]: zlim_utils::debug::DebugName
    #[inline(always)]
    pub fn name(&self) -> DebugName {
        self.name
    }

    /// Returns the [`ResourceId`] of the backing `MessageQueue<T>` resource.
    #[inline(always)]
    pub fn resource_id(&self) -> ResourceId {
        self.resource_id
    }

    /// Returns the queue rotation function for this message type.
    #[inline(always)]
    pub fn update(&self) -> fn(&mut World) {
        self.update
    }
}

fn update_queue<M: Message>(world: &mut World) {
    if let Some(mut queue) = world.get_resource_mut::<MessageQueue<M>>() {
        queue.update();
    }
}

// -----------------------------------------------------------------------------
// Messages

/// Registry of all message types registered in one [`World`].
///
/// Every [`World`] owns one `Messages` instance. Registering a message type
/// through [`World::register_message`] records its metadata here, including
/// the per-type queue rotation function (see [`MessageQueue::update`]).
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Message)]
/// struct Ping;
///
/// let mut world = World::alloc();
/// let id = world.register_message::<Ping>();
///
/// assert_eq!(world.messages().len(), 1);
/// assert!(world.messages().get(id).is_some());
/// assert!(world.messages().get_name(id).is_some());
/// ```
///
/// [`World`]: crate::world::World
/// [`World::register_message`]: crate::world::World::register_message
/// [`MessageQueue::update`]: crate::message::MessageQueue::update
pub struct Messages {
    metas: Vec<MessageMeta>,
    mapper: TypeMap<MessageId>,
}

impl Debug for Messages {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&self.metas, f)
    }
}

impl Messages {
    pub(crate) const fn new() -> Self {
        Self {
            metas: Vec::new(),
            mapper: TypeMap::new(),
        }
    }
}

impl Messages {
    /// Returns the number of registered message types.
    #[inline]
    #[expect(clippy::len_without_is_empty, reason = "useless")]
    pub fn len(&self) -> usize {
        self.metas.len()
    }

    /// Looks up a message ID by its [`TypeId`].
    #[inline]
    pub fn get_id(&self, type_id: TypeId) -> Option<MessageId> {
        self.mapper.get(type_id).copied()
    }

    /// Returns the message debug name for the given ID.
    #[inline]
    pub fn get_name(&self, id: MessageId) -> Option<DebugName> {
        self.metas.get(id.index()).map(MessageMeta::name)
    }

    /// Returns the message metadata for the given ID.
    #[inline]
    pub fn get(&self, id: MessageId) -> Option<&MessageMeta> {
        self.metas.get(id.index())
    }

    /// Returns a slice containing the entire message metadata registry.
    #[inline]
    pub fn as_slice(&self) -> &[MessageMeta] {
        self.metas.as_slice()
    }

    /// Returns an iterator over the [`MessageMeta`] values.
    #[inline]
    pub fn iter(&self) -> core::slice::Iter<'_, MessageMeta> {
        self.metas.iter()
    }
}

impl Messages {
    /// Registers message type `M` and records its metadata.
    ///
    /// If `M` is already registered this returns the existing [`MessageId`].
    /// The caller must ensure the backing `MessageQueue<M>` resource exists
    /// (see [`World::register_message`]).
    ///
    /// [`World::register_message`]: crate::world::World::register_message
    pub(crate) fn register<M: Message>(&mut self) -> MessageId {
        if let Some(id) = self.mapper.get(TypeId::of::<M>()) {
            return *id;
        }

        let id = MessageId::without_provenance(self.metas.len());
        let meta = MessageMeta {
            id,
            name: DebugName::type_name::<M>(),
            resource_id: ResourceDB::of::<MessageQueue<M>>().id,
            update: update_queue::<M>,
        };

        self.metas.push(meta);
        self.mapper.insert(TypeId::of::<M>(), id);

        id
    }
}

// -----------------------------------------------------------------------------
