//! Double-buffered storage for message values of one type.
//!
//! [`MessageQueue<M>`] is a [`Resource`] holding messages of type `M` in two
//! sequences, rotated by [`MessageQueue::update`] to decouple writers from
//! readers across update ticks.
//!
//! [`MessageQueue<M>`]: crate::message::MessageQueue
//! [`Resource`]: crate::resource::Resource

use core::fmt::Debug;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

use zlim_reflect::TypePath;

use crate::message::MessageKeyIter;
use crate::message::{Message, MessageKey};
use crate::resource::Resource;

// -----------------------------------------------------------------------------
// MessageSequence

pub(super) struct MessageSequence<M: Message> {
    pub messages: Vec<(MessageKey<M>, M)>,
    pub start_id: usize,
}

impl<M: Message + Debug> Debug for MessageSequence<M> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_map()
            .entries(self.messages.iter().map(|x| (x.0, &x.1)))
            .finish()
    }
}

impl<M: Message> Default for MessageSequence<M> {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            start_id: 0,
        }
    }
}

impl<M: Message> Deref for MessageSequence<M> {
    type Target = Vec<(MessageKey<M>, M)>;

    fn deref(&self) -> &Self::Target {
        &self.messages
    }
}

impl<M: Message> DerefMut for MessageSequence<M> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.messages
    }
}

// -----------------------------------------------------------------------------
// MessageQueue

/// Resource storage for message values of type `M`.
///
/// `MessageQueue<M>` uses two sequences:
/// - `messages_a` keeps the older still-readable values,
/// - `messages_b` keeps the newer values written in the current update.
///
/// After calling [`Self::update`], sequences are rotated so new writes start
/// in a fresh `messages_b` while old values remain observable for one extra
/// update.
///
/// `MessageQueue<M>` implements [`Resource`]. The usual way to create it is
/// [`World::register_message::<M>`], after which it can be accessed through
/// the [`MessageWriter`], [`MessageReader`], and [`MessageMutator`] system
/// parameters.
///
/// [`Resource`]: crate::resource::Resource
/// [`World::register_message::<M>`]: crate::world::World::register_message
/// [`MessageWriter`]: crate::message::MessageWriter
/// [`MessageReader`]: crate::message::MessageReader
/// [`MessageMutator`]: crate::message::MessageMutator
///
/// # Example
///
/// ```rust
/// use zlim_core::message::{Message, MessageQueue};
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Message)]
/// struct Hit {
///     value: u32,
/// }
///
/// let mut messages = MessageQueue::<Hit>::default();
/// let id = messages.write(Hit { value: 10 });
///
/// assert_eq!(messages.len(), 1);
/// assert_eq!(messages.get(id.index()).map(|(_, m)| m.value), Some(10));
///
/// messages.update();
/// assert_eq!(messages.len(), 1);
///
/// messages.update();
/// assert_eq!(messages.len(), 0);
/// ```
#[derive(TypePath)]
#[type_path = "zlim_core::message::MessageQueue"]
pub struct MessageQueue<M: Message> {
    /// Holds the oldest still active messages.
    /// Note that `a.start_id + a.len()` should always be equal to `messages_b.start_id`.
    pub(super) messages_a: MessageSequence<M>,
    /// Holds the newer messages.
    pub(super) messages_b: MessageSequence<M>,
    pub(super) counter: usize,
}

impl<M: Message> Resource for MessageQueue<M> {}

// -----------------------------------------------------------------------------
// MessageQueue impls

impl<M: Message> Default for MessageQueue<M> {
    fn default() -> Self {
        Self {
            messages_a: Default::default(),
            messages_b: Default::default(),
            counter: 0,
        }
    }
}

impl<M: Message + Debug> Debug for MessageQueue<M> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MessageQueue")
            .field("previous", &self.messages_a)
            .field("current", &self.messages_b)
            .finish()
    }
}

impl<M: Message> Extend<M> for MessageQueue<M> {
    fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = M>,
    {
        let mut msg_count = self.counter;
        let messages = iter.into_iter().map(|message| {
            let id = MessageKey::new(msg_count);
            msg_count = msg_count.wrapping_add(1);
            (id, message)
        });

        self.messages_b.extend(messages);
        self.counter = msg_count;
    }
}

impl<M: Message> MessageQueue<M> {
    /// Returns the number of currently readable messages across both sequences.
    #[inline]
    pub fn len(&self) -> usize {
        self.messages_a.len() + self.messages_b.len()
    }

    /// Returns `true` when there are no readable messages.
    #[inline]
    pub fn is_empty(&self) -> bool {
        0 == (self.messages_a.len() | self.messages_b.len())
    }

    /// Returns the global index of the oldest still-readable message.
    ///
    /// This value is used by [`crate::message::MessageCursor`] to determine the
    /// lower bound of readable ids.
    #[inline]
    pub fn oldest_message_index(&self) -> usize {
        self.messages_a.start_id
    }

    /// Returns the global index of the *next* message.
    #[inline]
    pub fn counter(&self) -> usize {
        self.counter
    }

    /// Gets a message by global index.
    ///
    /// Returns `None` if the index is already expired or not yet written.
    pub fn get(&self, id: usize) -> Option<(MessageKey<M>, &M)> {
        if id < self.messages_a.start_id {
            return None;
        }

        let seq = if id < self.messages_b.start_id {
            &self.messages_a
        } else {
            &self.messages_b
        };

        let index = id.wrapping_sub(seq.start_id);
        seq.get(index).map(|m| (m.0, &m.1))
    }

    /// Gets a mutable message by global index.
    ///
    /// Returns `None` if the index is already expired or not yet written.
    pub fn get_mut(&mut self, id: usize) -> Option<(MessageKey<M>, &mut M)> {
        if id < self.messages_a.start_id {
            return None;
        }

        let seq = if id < self.messages_b.start_id {
            &mut self.messages_a
        } else {
            &mut self.messages_b
        };

        let index = id.wrapping_sub(seq.start_id);

        seq.get_mut(index).map(|m| (m.0, &mut m.1))
    }

    /// Appends one message into the current write sequence.
    ///
    /// Returns the generated [`MessageKey`] for later correlation.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::message::{Message, MessageQueue};
    /// use zlim_reflect::derive::TypePath;
    ///
    /// #[derive(TypePath, Message)]
    /// struct Ping;
    ///
    /// let mut messages = MessageQueue::<Ping>::default();
    /// let key = messages.write(Ping);
    ///
    /// assert_eq!(key.index(), 0);
    /// assert_eq!(messages.len(), 1);
    /// ```
    ///
    /// [`MessageKey`]: crate::message::MessageKey
    pub fn write(&mut self, message: M) -> MessageKey<M> {
        let id = MessageKey::new(self.counter);
        self.messages_b.push((id, message));
        self.counter = self.counter.wrapping_add(1);
        id
    }

    /// Appends a batch of messages and returns the generated id range.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::message::{Message, MessageQueue};
    /// use zlim_reflect::derive::TypePath;
    ///
    /// #[derive(TypePath, Message)]
    /// struct Ping(u32);
    ///
    /// let mut messages = MessageQueue::<Ping>::default();
    /// let count = messages.write_batch([Ping(1), Ping(2)]).count();
    /// assert_eq!(count, 2);
    /// assert_eq!(messages.len(), 2);
    /// ```
    pub fn write_batch(&mut self, messages: impl IntoIterator<Item = M>) -> MessageKeyIter<M> {
        let last = self.counter;
        self.extend(messages);
        let end = self.counter;
        MessageKeyIter {
            last,
            end,
            _marker: PhantomData,
        }
    }

    /// Removes all currently stored messages from both sequences.
    ///
    /// After this call, both sequence start indices are advanced to the
    /// current counter.
    pub fn clear(&mut self) {
        self.messages_a.start_id = self.counter;
        self.messages_b.start_id = self.counter;
        self.messages_a.clear();
        self.messages_b.clear();
    }

    /// Rotates sequences to advance lifecycle by one update.
    ///
    /// The current write sequence becomes the older readable sequence. Then a
    /// new empty write sequence is prepared by clearing the previous older
    /// sequence.
    ///
    /// This method is usually driven once per update by the application's
    /// update loop (the [`Messages`] registry stores the rotation function
    /// for every registered message type).
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::message::{Message, MessageQueue};
    /// use zlim_reflect::derive::TypePath;
    ///
    /// #[derive(TypePath, Message)]
    /// struct Hit;
    ///
    /// let mut messages = MessageQueue::<Hit>::default();
    /// messages.write(Hit);
    ///
    /// // First rotation: the written message becomes readable.
    /// messages.update();
    /// assert_eq!(messages.len(), 1);
    ///
    /// // Second rotation: the message is expired.
    /// messages.update();
    /// assert_eq!(messages.len(), 0);
    /// ```
    ///
    /// [`Messages`]: crate::message::Messages
    #[inline]
    pub fn update(&mut self) {
        core::mem::swap(&mut self.messages_a, &mut self.messages_b);

        self.messages_b.clear();
        self.messages_b.start_id = self.counter;

        debug_assert_eq!(
            self.messages_a.start_id + self.messages_a.len(),
            self.messages_b.start_id,
            "mismatched message start_id for type :{}",
            Self::type_path()
        );
    }

    /// Drains and returns all currently readable messages.
    ///
    /// Older messages are yielded before newer messages.
    #[must_use = "If you do not need the returned messages, call .clear() instead."]
    pub fn drain(&mut self) -> impl Iterator<Item = M> + '_ {
        self.messages_a.start_id = self.counter;
        self.messages_b.start_id = self.counter;

        // Drain the oldest messages first, then the newest.
        self.messages_a
            .drain(..)
            .chain(self.messages_b.drain(..))
            .map(|i| i.1)
    }

    /// Rotates lifecycle and drains the sequence that was previously older.
    ///
    /// This is useful when the caller wants update semantics while consuming
    /// stale messages in one pass.
    #[must_use = "If you do not need the returned messages, call .update() instead."]
    pub fn update_drain(&mut self) -> impl Iterator<Item = M> + '_ {
        core::mem::swap(&mut self.messages_a, &mut self.messages_b);
        let iter = self.messages_b.messages.drain(..);
        self.messages_b.start_id = self.counter;

        debug_assert_eq!(
            self.messages_a.start_id + self.messages_a.len(),
            self.messages_b.start_id,
            "mismatched message start_id for type :{}",
            Self::type_path()
        );

        iter.map(|e| e.1)
    }
}

// -----------------------------------------------------------------------------
