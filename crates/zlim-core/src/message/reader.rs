//! Read-only system parameter for consuming unread messages.

use crate::borrow::Res;
use crate::message::{Message, MessageCursor, MessageQueue};
use crate::message::{MessageIterator, MessageWithKeyIter};
use crate::system::{AccessTable, Local, SystemParam, SystemParamError};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

/// Read-only system parameter for consuming unread messages of type `M`.
///
/// Each system instance keeps its own local [`MessageCursor`], so independent
/// systems can read the same messages without interfering with each other.
///
/// Calling [`Self::read`] or [`Self::read_with_key`] advances this system's
/// local cursor as iterator items are consumed.
///
/// [`MessageCursor`]: crate::message::MessageCursor
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
/// fn handle_collisions(mut reader: MessageReader<Collision>) {
///     for collision in reader.read() {
///         let _ = (collision.lhs, collision.rhs);
///     }
/// }
///
/// // `MessageReader` wraps a `MessageQueue<Collision>` plus a local cursor:
/// let mut queue = MessageQueue::<Collision>::default();
/// queue.write(Collision { lhs: 1, rhs: 2 });
///
/// let mut cursor = MessageCursor::new(&queue);
/// let collisions: Vec<_> = cursor.read(&queue).collect();
/// assert_eq!(collisions.len(), 1);
/// assert_eq!(collisions[0].lhs, 1);
/// ```
pub struct MessageReader<'w, 's, M: Message> {
    cursor: Local<'s, MessageCursor<M>>,
    messages: Res<'w, MessageQueue<M>>,
}

impl<'w, 's, M: Message> MessageReader<'w, 's, M> {
    /// Returns an iterator over unread messages for this reader cursor.
    ///
    /// Iteration advances the cursor.
    pub fn read(&mut self) -> MessageIterator<'_, M> {
        self.cursor.read(&self.messages)
    }

    /// Returns unread messages together with their [`MessageKey`].
    ///
    /// Iteration advances the cursor.
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
    /// fn handle_collisions(mut reader: MessageReader<Collision>) {
    ///     for (key, collision) in reader.read_with_key() {
    ///         let _ = (key.index(), collision.lhs, collision.rhs);
    ///     }
    /// }
    ///
    /// let mut queue = MessageQueue::<Collision>::default();
    /// queue.write(Collision { lhs: 1, rhs: 2 });
    ///
    /// let mut cursor = MessageCursor::new(&queue);
    /// let mut seen = Vec::new();
    /// for (key, collision) in cursor.read_with_key(&queue) {
    ///     seen.push((key.index(), collision.lhs, collision.rhs));
    /// }
    /// assert_eq!(seen, vec![(0, 1, 2)]);
    /// ```
    ///
    /// [`MessageKey`]: crate::message::MessageKey
    pub fn read_with_key(&mut self) -> MessageWithKeyIter<'_, M> {
        self.cursor.read_with_key(&self.messages)
    }

    /// Returns the number of unread messages for this reader cursor.
    pub fn len(&self) -> usize {
        self.cursor.len(&self.messages)
    }

    /// Returns `true` if there are no unread messages for this reader cursor.
    pub fn is_empty(&self) -> bool {
        self.cursor.is_empty(&self.messages)
    }

    /// Marks all currently readable messages as seen for this cursor.
    pub fn clear(&mut self) {
        self.cursor.clear(&self.messages);
    }
}

type InternalParam<M> = (
    Local<'static, MessageCursor<M>>,
    Res<'static, MessageQueue<M>>,
);

// SAFETY: Delegates state, access declaration, and value fetching to the
// tuple parameter implementation.
unsafe impl<M: Message> SystemParam for MessageReader<'_, '_, M> {
    type State = <InternalParam<M> as SystemParam>::State;

    type Item<'world, 'state> = MessageReader<'world, 'state, M>;

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
        // SAFETY: same world/state/tick contract as the delegated tuple
        // parameter.
        let (cursor, messages) = unsafe {
            <InternalParam<M> as SystemParam>::build_param(state, world, last_run, this_run)?
        };

        Ok(MessageReader { cursor, messages })
    }
}

// -----------------------------------------------------------------------------
