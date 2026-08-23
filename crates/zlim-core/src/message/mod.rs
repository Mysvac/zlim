//! Message passing primitives for ECS systems.
//!
//! This module implements a compact, double-buffered message pipeline used to
//! decouple producers and consumers inside the scheduler. Messages are stored
//! in [`MessageQueue`] resources and rotated by the [`Messages`] registry so
//! that writers and readers observe a stable view without unbounded buffering.
//!
//! # Key Types
//!
//! - [`Message`]: marker trait for payload types derived by `Message` proc-macro.
//! - [`MessageQueue<M>`]: the double-buffered resource storing messages for `M`.
//! - [`MessageWriter<M>`]: system parameter for appending messages to the write buffer.
//! - [`MessageReader<M>`]: system parameter for reading unread messages from the read buffer.
//! - [`MessageMutator<M>`]: system parameter for mutating unread messages in place.
//! - [`MessageId`]: compact identifier assigned to each registered message type.
//! - [`MessageKey<M>`]: per-stream key for one message in a `MessageQueue<M>`.
//! - [`MessageCursor<M>`]: per-system read position over a `MessageQueue<M>` stream.
//! - [`MessageMeta`]: per-type metadata record stored in the [`Messages`] registry.
//! - [`Messages`]: global registry holding metadata for all registered message types
//!   and rotating their queues in sync.
//!
//! # Registration & Lifecycle
//!
//! To use a message type `T` you must register it with the world (usually at
//! startup) via [`World::register_message`]. Registration ensures the
//! underlying `MessageQueue<T>` resource exists and records a [`MessageId`] in
//! the [`Messages`] registry.
//!
//! Writers append new messages to the write buffer. Rotating a queue (via
//! [`MessageQueue::update`], usually driven by the application's update
//! loop) swaps the write buffer into the read position and clears the old
//! one. This guarantees that messages written during one update are visible
//! to readers in the following update, while keeping memory usage bounded.
//!
//! # Typical Flow
//!
//! 1. `world.register_message::<T>()` to create the `MessageQueue<T>` resource.
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
//! use zlim_reflect::derive::TypePath;
//!
//! #[derive(TypePath, Message)]
//! struct Ping;
//!
//! let mut world = World::alloc();
//! world.register_message::<Ping>();
//!
//! assert!(world.write_message(Ping).is_some());
//! assert_eq!(world.get_resource::<MessageQueue<Ping>>().unwrap().len(), 1);
//!
//! // In a system, messages are consumed through system parameters:
//! fn consume(mut reader: MessageReader<Ping>) {
//!     for _ in reader.read() {}
//! }
//! ```
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
pub use messages::{MessageMeta, Messages};
pub use mutator::MessageMutator;
pub use queue::MessageQueue;
pub use reader::MessageReader;
pub use writer::MessageWriter;

pub use zlim_core_derive::Message;

// -----------------------------------------------------------------------------
