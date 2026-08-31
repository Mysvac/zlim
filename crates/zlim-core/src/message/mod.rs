//! Message passing primitives for ECS systems.
//!
//! This module implements a compact, double-buffered message pipeline used to
//! decouple producers and consumers inside the scheduler. Messages are stored
//! in [`MessageQueue`] resources and rotated by the [`Messages`] registry so
//! that writers and readers observe a stable view without unbounded buffering.
//!
//! # Typical Flow
//!
//! 1. `world.register_message::<T>()` at startup to create the
//!    `MessageQueue<T>` resource and register the type.
//! 2. In producer systems use [`MessageWriter<T>`] to append messages.
//! 3. In consumer systems use [`MessageReader<T>`] / [`MessageMutator<T>`] to consume.
//! 4. Rotate the queues once per update via [`MessageQueue::update`].
//!
//! Each consumer system keeps its own [`MessageCursor`], so reading messages
//! in one system never consumes them for another system.
//!
//! # Example
//!
//! ```rust
//! use zlim_core::prelude::*;
//!
//! #[derive(TypePath, Message)]
//! struct Ping;
//!
//! let mut world = World::alloc();
//! world.register_message::<Ping>();
//!
//! world.write_message(Ping);
//! assert_eq!(world.resource::<MessageQueue<Ping>>().len(), 1);
//!
//! // In a system, messages are read through MessageReader parameters:
//! fn read(mut reader: MessageReader<Ping>) {
//!     for _ in reader.read() {}
//! }
//!
//! // In a system, messages are write through MessageWriter parameters:
//! fn write(mut writer: MessageWriter<Ping>) {
//!     writer.write(Ping);
//!     writer.write_batch([Ping, Ping, Ping]);
//! }
//! ```
//!
//!
//! # Registration & Lifecycle
//!
//! Message types should be **registered** before use.  The recommended way is
//! to call [`World::register_message`] once at program startup for each type
//! the application uses.  Registration performs all three steps:
//!
//! 1. records the message type's metadata — a stable [`MessageId`] and the
//!    queue's per-frame update function — in the [`Messages`] registry;
//! 2. initializes the backing `MessageQueue<T>` resource;
//! 3. wires the queue into the world's update/rotation pass.
//!
//! [`World::write_message`] is lenient: writing a message whose queue does
//! not exist yet **implicitly registers** the type on the spot (the same two
//! steps above), so direct writes never fail.
//!
//! Still, prefer explicit startup registration over relying on the implicit
//! path: system parameters — [`MessageWriter`], [`MessageReader`],
//! [`MessageMutator`] — depend on an existing `MessageQueue<T>` resource but
//! do **not** register the type themselves.  If the queue is missing, such a
//! system is skipped and a warning-level error is reported.
//!
//! Writers append new messages to the write buffer. Rotating a queue (via
//! [`MessageQueue::update`], usually driven by the application's update
//! loop) swaps the write buffer into the read position and clears the old
//! one. This guarantees that messages written during one update are visible
//! to readers in the following update, while keeping memory usage bounded.
//!
//! Rotation does **not** trigger change detection on the `MessageQueue<T>`
//! resource: it goes through the `bypass()` path, so `Res<MessageQueue<T>>`
//! never reports the swap itself as a change — the buffer swap is internal
//! bookkeeping, not user data.
//!
//! [`MessageCursor`]: MessageCursor
//! [`MessageCursor<M>`]: MessageCursor
//! [`MessageKey<M>`]: MessageKey
//! [`MessageMeta`]: MessageMeta
//! [`MessageQueue`]: MessageQueue
//! [`MessageQueue<M>`]: MessageQueue
//! [`MessageQueue::update`]: MessageQueue::update
//! [`MessageWriter<T>`]: MessageWriter
//! [`MessageWriter<M>`]: MessageWriter
//! [`MessageReader<T>`]: MessageReader
//! [`MessageReader<M>`]: MessageReader
//! [`MessageMutator<T>`]: MessageMutator
//! [`MessageMutator<M>`]: MessageMutator
//! [`World::register_message`]: crate::world::World::register_message
//! [`World::write_message`]: crate::world::World::write_message

// -----------------------------------------------------------------------------
// Modules

mod ident;
mod iterators;
mod message;
mod messages;
mod mutator;
mod queue;
mod reader;
mod writer;

pub use ident::{MessageId, MessageKey};
pub use iterators::{MessageCursor, MessageKeyIter};
pub use iterators::{MessageIterator, MessageWithKeyIter};
pub use iterators::{MessageMutIterator, MessageMutWithKeyIter};
pub use message::Message;
pub use messages::{MessageMeta, Messages, UpdateMessagesSignal};
pub use mutator::MessageMutator;
pub use queue::MessageQueue;
pub use reader::MessageReader;
pub use writer::MessageWriter;

pub use zlim_core_derive::Message;

pub(crate) use messages::{enable_manual_update, update_messages};

// -----------------------------------------------------------------------------

pub use pre_defined::ReparentSignal;

/// This module defines a set of built-in messages.
///
/// Currently, only [`ReparentSignal`] is defined, which is used by the Transform Plugin.
/// Since our hierarchy is built-in rather than component-based, we need this to observe
/// hierarchy changes.
///
/// If needed, additional signals such as `DespawnSignal` and `SpawnSignal` may be added
/// in the future.
mod pre_defined {
    use super::Message;
    use crate::entity::EntityId;
    use zlim_reflect::derive::TypePath;

    /// A predefined message sent when an entity is reparented.
    #[derive(Debug, TypePath, Message, Clone, Copy)]
    pub struct ReparentSignal {
        pub entity: EntityId,
    }
}

// -----------------------------------------------------------------------------
