#![expect(clippy::module_inception, reason = "For better structure.")]

//! Marker trait for ECS message payload types.

use zlim_reflect::path::TypePath;

/// Marker trait for ECS message payload types.
///
/// A `Message` type is a short-lived payload sent between systems through
/// [`MessageQueue<M>`]. The trait has no methods: it only encodes the bounds
/// required by message storage and cross-system usage (`Send`, `Sync`,
/// [`TypePath`], and `'static`).
///
/// For user code, the recommended path is `#[derive(TypePath, Message)]`,
/// which implements both the [`TypePath`] and [`Message`] traits.
///
/// To participate in lifecycle rotation, register the type with
/// [`World::register_message`] and rotate its [`MessageQueue<M>`] once per
/// update via [`MessageQueue::update`].
///
/// [`MessageQueue::update`]: crate::message::MessageQueue::update
/// [`TypePath`]: zlim_reflect::TypePath
///
/// # Using MessageQueue In World
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Message)]
/// struct Collision;
///
/// let mut world = World::alloc();
/// world.register_message::<Collision>();
///
/// world.write_message(Collision);
///
/// let queue = world.get_resource::<MessageQueue<Collision>>().unwrap();
/// assert_eq!(queue.len(), 1);
/// ```
///
/// # Using MessageQueue In Systems
///
/// `Message` is consumed through system parameters in three roles:
/// - [`MessageWriter<T>`]: append new messages.
/// - [`MessageReader<T>`]: read unread messages immutably.
/// - [`MessageMutator<T>`]: read unread messages mutably.
///
/// `MessageReader` and `MessageMutator` each keep an independent local cursor,
/// so one system reading messages does not consume them for another system.
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Message)]
/// struct Damage {
///     amount: u32,
/// }
///
/// fn emit(mut writer: MessageWriter<Damage>) {
///     writer.write(Damage { amount: 120 });
/// }
///
/// fn clamp(mut mutator: MessageMutator<Damage>) {
///     for msg in mutator.read() {
///         msg.amount = msg.amount.min(100);
///     }
/// }
///
/// fn log(mut reader: MessageReader<Damage>) {
///     for msg in reader.read() {
///         let _ = msg.amount;
///     }
/// }
///
/// // The system parameters above wrap a `MessageQueue<Damage>` plus a
/// // per-system `MessageCursor`. Drive the same machinery directly to verify
/// // the writer/mutator/reader roles:
/// let mut queue = MessageQueue::<Damage>::default();
///
/// queue.write(Damage { amount: 120 });
///
/// let mut mutator_cursor = MessageCursor::new(&queue);
///
/// for damage in mutator_cursor.read_mut(&mut queue) {
///     damage.amount = damage.amount.min(100);
/// }
///
/// assert_eq!(queue.get(0).map(|(_, m)| m.amount), Some(100));
///
/// // A fresh cursor still observes the message: cursors are independent.
/// let mut reader_cursor = MessageCursor::new(&queue);
/// assert_eq!(reader_cursor.read(&queue).count(), 1);
/// assert!(reader_cursor.is_empty(&queue));
/// ```
///
/// [`MessageQueue<M>`]: crate::message::MessageQueue
/// [`MessageWriter<T>`]: crate::message::MessageWriter
/// [`MessageReader<T>`]: crate::message::MessageReader
/// [`MessageMutator<T>`]: crate::message::MessageMutator
/// [`World::register_message`]: crate::world::World::register_message
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a message",
    label = "invalid message",
    note = "Consider annotating `{Self}` with `#[derive(TypePath, Message)]`."
)]
pub trait Message: Send + Sync + TypePath + 'static {}

// -----------------------------------------------------------------------------
