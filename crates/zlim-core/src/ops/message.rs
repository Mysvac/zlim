//! Message registration and writing methods implemented on `World` and
//! `DeferredWorld`.

use crate::borrow::ResMut;
use crate::message::{Message, MessageId, MessageKey};
use crate::message::{MessageKeyIter, MessageQueue};
use crate::world::{DeferredWorld, World};

/// The implicit-registration fallback of [`World::write_message`]: registers
/// message type `M` (recording its metadata and the queue's update function)
/// and initializes the backing `MessageQueue<M>` resource, then returns it
/// for writing.  This is the same work [`World::register_message`] performs.
#[cold]
#[inline(never)]
fn try_init_queue<M: Message>(world: &mut World) -> ResMut<'_, MessageQueue<M>> {
    world.messages.register::<M>();
    world.resource_mut_or_init::<MessageQueue<M>>()
}

impl World {
    /// Registers a message type for use with this world.
    ///
    /// Call this once at program startup for every message type the
    /// application uses.  It performs all registration steps:
    ///
    /// 1. records the message type's metadata — a stable [`MessageId`] and
    ///    the queue's per-frame update function — in the message registry;
    /// 2. initializes the backing `MessageQueue<M>` resource;
    /// 3. wires the queue into the world's update/rotation pass.
    ///
    /// Registering the same type again is a no-op that returns the existing
    /// [`MessageId`].
    pub fn register_message<M: Message>(&mut self) -> MessageId {
        self.init_resource::<MessageQueue<M>>();
        self.messages.register::<M>()
    }

    /// Writes a [`Message`] and returns its [`MessageKey`].
    ///
    /// If the message type has not been registered, it is **implicitly
    /// registered** on first write (the queue is initialized and the type is
    /// added to the registry), so writing never fails.
    ///
    /// Prefer explicit registration via [`World::register_message`] at
    /// startup: system parameters such as [`MessageWriter`] /
    /// [`MessageReader`](crate::message::MessageReader) only *read* the queue
    /// and do not register the type themselves.  Without an existing queue
    /// those systems are skipped with a warning-level error.
    ///
    /// [`MessageWriter`]: crate::message::MessageWriter
    pub fn write_message<M: Message>(&mut self, message: M) -> MessageKey<M> {
        match self.get_resource_mut::<MessageQueue<M>>() {
            Some(mut msgs) => msgs.write(message),
            None => try_init_queue(self).write(message),
        }
    }

    /// Writes a batch of [`Message`]s from an iterator and returns their
    /// [IDs](`MessageKey`).
    ///
    /// Like [`World::write_message`], a missing queue is implicitly
    /// registered on first write; prefer explicit registration at startup.
    pub fn write_message_batch<M: Message>(
        &mut self,
        messages: impl IntoIterator<Item = M>,
    ) -> MessageKeyIter<M> {
        match self.get_resource_mut::<MessageQueue<M>>() {
            Some(mut msgs) => msgs.write_batch(messages),
            None => try_init_queue(self).write_batch(messages),
        }
    }
}

// -----------------------------------------------------------------------------
// DeferredWorld — writing messages
// -----------------------------------------------------------------------------

// We temporarily believe that the "creation" of resources does not belong to
// structure. Because ResourceCell is stored in isolation, creation" does not
// cause external references to become invalid.

impl DeferredWorld<'_> {
    /// Writes a [`Message`] and returns its [`MessageKey`].
    ///
    /// If the message type has not been registered, it is **implicitly
    /// registered** on first write (see [`World::write_message`]); prefer
    /// explicit registration via [`World::register_message`] at startup.
    pub fn write_message<M: Message>(&mut self, message: M) -> MessageKey<M> {
        // SAFETY: `DeferredWorld` holds exclusive access to the world for the
        // duration of this borrow; `data_mut` reinterprets it as `&mut World`,
        // and the resource accessors only mutate existing values.
        unsafe { self.cell().data_mut() }.write_message::<M>(message)
    }

    /// Writes a batch of [`Message`]s from an iterator and returns their
    /// [IDs](`MessageKey`).
    ///
    /// Like [`World::write_message`], a missing queue is implicitly
    /// registered on first write; prefer explicit registration at startup.
    pub fn write_message_batch<M: Message>(
        &mut self,
        messages: impl IntoIterator<Item = M>,
    ) -> MessageKeyIter<M> {
        // SAFETY: `DeferredWorld` holds exclusive access to the world for the
        // duration of this borrow; `data_mut` reinterprets it as `&mut World`,
        // and the resource accessors only mutate existing values.
        unsafe { self.cell().data_mut() }.write_message_batch::<M>(messages)
    }
}

// -----------------------------------------------------------------------------
