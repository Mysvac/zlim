//! Mutable system parameter for consuming and editing unread messages.

use crate::borrow::ResMut;
use crate::message::{Message, MessageCursor, MessageQueue};
use crate::message::{MessageMutIterator, MessageMutWithKeyIter};
use crate::system::{AccessTable, Local, SystemParam, SystemParamError};
use crate::tick::Tick;
use crate::world::{World, WorldCell};

/// Mutable reader parameter for consuming and editing unread messages of
/// type `M`.
///
/// Like [`MessageReader`], each system instance maintains its own local
/// cursor.
///
/// Reading mutably still follows unread semantics: this parameter only yields
/// messages not yet observed by this system's cursor.
///
/// [`MessageReader`]: crate::message::MessageReader
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Message)]
/// struct Damage {
///     amount: u32,
/// }
///
/// fn clamp_damage(mut mutator: MessageMutator<Damage>) {
///     for damage in mutator.read() {
///         damage.amount = damage.amount.min(100);
///     }
/// }
///
/// // `MessageMutator` reads mutably through the backing queue plus a local
/// // cursor:
/// let mut queue = MessageQueue::<Damage>::default();
/// queue.write(Damage { amount: 120 });
///
/// let mut cursor = MessageCursor::new(&queue);
/// for damage in cursor.read_mut(&mut queue) {
///     damage.amount = damage.amount.min(100);
/// }
/// assert_eq!(queue.get(0).map(|(_, m)| m.amount), Some(100));
/// ```
pub struct MessageMutator<'w, 's, M: Message> {
    cursor: Local<'s, MessageCursor<M>>,
    messages: ResMut<'w, MessageQueue<M>>,
}

impl<'w, 's, M: Message> MessageMutator<'w, 's, M> {
    /// Returns a mutable iterator over unread messages for this cursor.
    ///
    /// Iteration advances the cursor.
    pub fn read(&mut self) -> MessageMutIterator<'_, M> {
        self.cursor.read_mut(&mut self.messages)
    }

    /// Returns mutable unread messages together with their [`MessageKey`]s.
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
    /// struct Damage {
    ///     amount: u32,
    /// }
    ///
    /// fn clamp_damage(mut mutator: MessageMutator<Damage>) {
    ///     for (key, damage) in mutator.read_with_id() {
    ///         let _ = key.index();
    ///         damage.amount = damage.amount.min(100);
    ///     }
    /// }
    ///
    /// let mut queue = MessageQueue::<Damage>::default();
    /// queue.write(Damage { amount: 120 });
    ///
    /// let mut cursor = MessageCursor::new(&queue);
    /// for (key, damage) in cursor.read_mut_with_id(&mut queue) {
    ///     let _ = key.index();
    ///     damage.amount = damage.amount.min(100);
    /// }
    /// assert_eq!(queue.get(0).map(|(_, m)| m.amount), Some(100));
    /// ```
    ///
    /// [`MessageKey`]: crate::message::MessageKey
    pub fn read_with_id(&mut self) -> MessageMutWithKeyIter<'_, M> {
        self.cursor.read_mut_with_id(&mut self.messages)
    }

    /// Returns the number of unread messages for this cursor.
    pub fn len(&self) -> usize {
        self.cursor.len(&self.messages)
    }

    /// Returns `true` if there are no unread messages for this cursor.
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
    ResMut<'static, MessageQueue<M>>,
);

// SAFETY: Delegates state, access declaration, and value fetching to the
// tuple parameter implementation.
unsafe impl<M: Message> SystemParam for MessageMutator<'_, '_, M> {
    type State = <InternalParam<M> as SystemParam>::State;

    type Item<'world, 'state> = MessageMutator<'world, 'state, M>;

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

        Ok(MessageMutator { cursor, messages })
    }
}

// -----------------------------------------------------------------------------
