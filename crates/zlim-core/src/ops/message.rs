//! Message registration and writing methods implemented on `World` and
//! `DeferredWorld`.

use zlim_utils::debug::DebugName;

use zlim_log as log;

use crate::message::{Message, MessageId, MessageKey};
use crate::message::{MessageKeyIter, MessageQueue};
use crate::world::{DeferredWorld, World};

#[cold]
#[inline(never)]
fn unregistered_message(name: DebugName) {
    log::error!(
        "Unable to write message `{name}`, call `World::register_message` before write it."
    );
}

impl World {
    /// Registers a message type in the global message registry.
    pub fn register_message<M: Message>(&mut self) -> MessageId {
        self.init_resource::<MessageQueue<M>>();
        self.messages.register::<M>()
    }

    /// Writes a [`Message`].
    ///
    /// This method returns the [`MessageKey`] of the written `message`, or
    /// [`None`] if the `message` could not be written because its queue has
    /// not been registered.
    pub fn write_message<M: Message>(&mut self, message: M) -> Option<MessageKey<M>> {
        let Some(mut msgs) = self.get_resource_mut::<MessageQueue<M>>() else {
            unregistered_message(DebugName::type_name::<M>());
            return None;
        };
        Some(msgs.write(message))
    }

    /// Writes a batch of [`Message`]s from an iterator.
    ///
    /// This method returns the [IDs](`MessageKey`) of the written `messages`,
    /// or [`None`] if the `messages` could not be written because their queue
    /// has not been registered.
    pub fn write_message_batch<M: Message>(
        &mut self,
        messages: impl IntoIterator<Item = M>,
    ) -> Option<MessageKeyIter<M>> {
        let Some(mut msgs) = self.get_resource_mut::<MessageQueue<M>>() else {
            unregistered_message(DebugName::type_name::<M>());
            return None;
        };
        Some(msgs.write_batch(messages))
    }
}

// -----------------------------------------------------------------------------
// DeferredWorld — writing messages
// -----------------------------------------------------------------------------

impl DeferredWorld<'_> {
    /// Writes a [`Message`].
    ///
    /// This method returns the [`MessageKey`] of the written `message`, or
    /// [`None`] if the `message` could not be written because its queue has
    /// not been registered.
    pub fn write_message<M: Message>(&mut self, message: M) -> Option<MessageKey<M>> {
        // SAFETY: `DeferredWorld` holds exclusive access to the world for the
        // duration of this borrow; `data_mut` reinterprets it as `&mut World`,
        // and the resource accessors only mutate existing values.
        unsafe { self.cell().data_mut() }.write_message::<M>(message)
    }

    /// Writes a batch of [`Message`]s from an iterator.
    ///
    /// This method returns the [IDs](`MessageKey`) of the written `messages`,
    /// or [`None`] if the `messages` could not be written because their queue
    /// has not been registered.
    pub fn write_message_batch<M: Message>(
        &mut self,
        messages: impl IntoIterator<Item = M>,
    ) -> Option<MessageKeyIter<M>> {
        // SAFETY: `DeferredWorld` holds exclusive access to the world for the
        // duration of this borrow; `data_mut` reinterprets it as `&mut World`,
        // and the resource accessors only mutate existing values.
        unsafe { self.cell().data_mut() }.write_message_batch::<M>(messages)
    }
}

// -----------------------------------------------------------------------------
