//! System parameter for appending messages to the write buffer.

use crate::borrow::ResMut;
use crate::message::{Message, MessageKey, MessageKeyIter, MessageQueue};
use crate::system::{AccessTable, SystemParam, SystemParamError};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

/// System parameter that appends messages of type `M`.
///
/// Messages are appended into the current write sequence of
/// [`MessageQueue<M>`] and become readable according to the message
/// lifecycle rotation (see [`MessageQueue::update`]).
///
/// [`MessageQueue<M>`]: crate::message::MessageQueue
/// [`MessageQueue::update`]: crate::message::MessageQueue::update
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Message)]
/// struct Collision {
///     lhs: u32,
///     rhs: u32,
/// }
///
/// fn detect_collisions(mut writer: MessageWriter<Collision>) {
///     writer.write(Collision { lhs: 1, rhs: 2 });
///
///     writer.write_batch([
///         Collision { lhs: 10, rhs: 11 },
///         Collision { lhs: 20, rhs: 21 },
///     ]);
/// }
///
/// // `MessageWriter` appends into the write sequence of the backing queue:
/// let mut queue = MessageQueue::<Collision>::default();
/// queue.write(Collision { lhs: 1, rhs: 2 });
/// queue.write_batch([
///     Collision { lhs: 10, rhs: 11 },
///     Collision { lhs: 20, rhs: 21 },
/// ]);
///
/// assert_eq!(queue.len(), 3);
/// assert_eq!(queue.get(0).map(|(_, m)| (m.lhs, m.rhs)), Some((1, 2)));
/// ```
pub struct MessageWriter<'w, M: Message> {
    messages: ResMut<'w, MessageQueue<M>>,
}

impl<'w, M: Message> MessageWriter<'w, M> {
    /// Writes a default-constructed message and returns its generated id.
    ///
    /// Requires `M: Default` to construct the message value.
    #[inline]
    pub fn write_default(&mut self) -> MessageKey<M>
    where
        M: Default,
    {
        self.messages.write(M::default())
    }

    /// Writes one message and returns its generated id.
    #[inline]
    pub fn write(&mut self, message: M) -> MessageKey<M> {
        self.messages.write(message)
    }

    /// Writes a batch of messages and returns the generated id range.
    #[inline]
    pub fn write_batch(&mut self, messages: impl IntoIterator<Item = M>) -> MessageKeyIter<M> {
        self.messages.write_batch(messages)
    }
}

type InternalParam<M> = ResMut<'static, MessageQueue<M>>;

// SAFETY: Delegates state, access declaration, and value fetching to
// `ResMut<MessageQueue<M>>`.
unsafe impl<M: Message> SystemParam for MessageWriter<'_, M> {
    type State = <InternalParam<M> as SystemParam>::State;

    type Item<'world, 'state> = MessageWriter<'world, M>;

    const NON_SEND: bool = false;
    const EXCLUSIVE: bool = false;

    fn init_state(world: &World) -> Self::State {
        <InternalParam<M> as SystemParam>::init_state(world)
    }

    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        <InternalParam<M> as SystemParam>::register_access(state, table, strict)
    }

    unsafe fn build_param<'w, 's>(
        state: &'s mut Self::State,
        world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        // SAFETY: same world/state/tick contract as the delegated parameter.
        let messages = unsafe {
            <InternalParam<M> as SystemParam>::build_param(state, world, last_run, this_run)?
        };

        Ok(MessageWriter { messages })
    }
}

// -----------------------------------------------------------------------------
