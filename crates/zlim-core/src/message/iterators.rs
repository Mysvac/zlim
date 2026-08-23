//! Cursors and iterators over unread message streams.

use core::fmt::Debug;
use core::iter::{Chain, FusedIterator};
use core::marker::PhantomData;
use core::slice::{Iter, IterMut};

use crate::world::{FromWorld, World};

use super::ident::MessageKey;
use super::{Message, MessageQueue};

// -----------------------------------------------------------------------------
// MessageKeyIter

/// Iterator over [`MessageKey`] values written by a batch call.
///
/// This iterator yields ids in write order: `[start, end)`.
///
/// [`MessageKey`]: crate::message::MessageKey
///
/// # Example
///
/// ```rust
/// use zlim_core::message::{Message, MessageQueue};
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Message)]
/// struct Event;
///
/// let mut messages = MessageQueue::<Event>::default();
/// let mut ids = messages.write_batch([Event, Event]);
///
/// assert_eq!(ids.next().map(|id| id.index()), Some(0));
/// assert_eq!(ids.next().map(|id| id.index()), Some(1));
/// assert_eq!(ids.next(), None);
/// ```
pub struct MessageKeyIter<M: Message> {
    pub(super) last: usize,
    pub(super) end: usize,
    pub(super) _marker: PhantomData<M>,
}

impl<M: Message> Iterator for MessageKeyIter<M> {
    type Item = MessageKey<M>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.last == self.end {
            return None;
        }

        let id = MessageKey::new(self.last);
        self.last = self.last.wrapping_add(1);
        Some(id)
    }

    #[inline]
    fn count(self) -> usize {
        self.end.wrapping_sub(self.last)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.end.wrapping_sub(self.last);
        (len, Some(len))
    }
}

impl<M: Message> ExactSizeIterator for MessageKeyIter<M> {}
impl<M: Message> FusedIterator for MessageKeyIter<M> {}

// -----------------------------------------------------------------------------
// MessageCursor

/// Per-system read position for one `MessageQueue<M>` stream.
///
/// `MessageCursor` is usually managed by ECS as a local system parameter state
/// (see [`MessageReader`] and [`MessageMutator`]). Each system instance has
/// its own cursor, so one system reading messages does not consume them for
/// other systems.
///
/// Cursor advancement is pull-based: it advances when iterator items are
/// consumed (or when methods like `count`/`nth`/`last` skip items).
///
/// # Example
///
/// ```rust
/// use zlim_core::message::{Message, MessageCursor, MessageQueue};
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Message)]
/// struct Hit;
///
/// let mut messages = MessageQueue::<Hit>::default();
/// messages.write(Hit);
///
/// let mut cursor = MessageCursor::new(&messages);
/// assert_eq!(cursor.len(&messages), 1);
///
/// let hits: Vec<_> = cursor.read(&messages).collect();
/// assert_eq!(hits.len(), 1);
/// assert!(cursor.is_empty(&messages));
/// ```
///
/// [`MessageReader`]: crate::message::MessageReader
/// [`MessageMutator`]: crate::message::MessageMutator
pub struct MessageCursor<M: Message> {
    pub(super) last_index: usize,
    pub(super) _marker: PhantomData<M>,
}

impl<M: Message> FromWorld for MessageCursor<M> {
    fn from_world(world: &World) -> Self {
        let last_index = world
            .get_resource::<MessageQueue<M>>()
            .map(MessageQueue::<M>::oldest_message_index)
            .unwrap_or_default();

        Self {
            last_index,
            _marker: PhantomData,
        }
    }
}

impl<M: Message> Debug for MessageCursor<M> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("MessageCursor")
            .field(&self.last_index)
            .finish()
    }
}

impl<M: Message> Copy for MessageCursor<M> {}

impl<M: Message> Clone for MessageCursor<M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M: Message> MessageCursor<M> {
    /// Creates a `MessageCursor` that points at the message of given indexs.
    pub const fn with_index(index: usize) -> Self {
        Self {
            last_index: index,
            _marker: PhantomData,
        }
    }

    /// Creates a `MessageCursor` that points at the oldest readable message.
    pub fn new(messages: &MessageQueue<M>) -> Self {
        let last_index = messages.oldest_message_index();
        Self {
            last_index,
            _marker: PhantomData,
        }
    }

    /// Returns the unread count for this cursor in the given message storage.
    pub fn len(&self, messages: &MessageQueue<M>) -> usize {
        let upper = messages.counter.wrapping_sub(self.last_index);
        upper.min(messages.len())
    }

    /// Returns `true` if this cursor has no unread messages.
    pub fn is_empty(&self, messages: &MessageQueue<M>) -> bool {
        messages.is_empty() || messages.counter == self.last_index
    }

    /// Marks all currently readable messages as consumed for this cursor.
    pub fn clear(&mut self, messages: &MessageQueue<M>) {
        self.last_index = messages.counter;
    }

    /// Reads unread messages and advances the cursor as items are consumed.
    pub fn read<'a>(&'a mut self, messages: &'a MessageQueue<M>) -> MessageIterator<'a, M> {
        MessageWithKeyIter::new(self, messages).without_key()
    }

    /// Reads unread messages with ids and advances the cursor as items are consumed.
    pub fn read_with_key<'a>(
        &'a mut self,
        messages: &'a MessageQueue<M>,
    ) -> MessageWithKeyIter<'a, M> {
        MessageWithKeyIter::new(self, messages)
    }

    /// Reads unread messages mutably and advances the cursor as items are consumed.
    pub fn read_mut<'a>(
        &'a mut self,
        messages: &'a mut MessageQueue<M>,
    ) -> MessageMutIterator<'a, M> {
        MessageMutWithKeyIter::new(self, messages).without_id()
    }

    /// Reads unread mutable messages with ids and advances the cursor.
    pub fn read_mut_with_id<'a>(
        &'a mut self,
        messages: &'a mut MessageQueue<M>,
    ) -> MessageMutWithKeyIter<'a, M> {
        MessageMutWithKeyIter::new(self, messages)
    }
}

// -----------------------------------------------------------------------------
// MessageWithKeyIter

/// Iterator over unread messages with their message IDs.
///
/// Created by [`MessageCursor::read_with_key`] and
/// [`MessageReader::read_with_key`].
///
/// Consuming this iterator advances the underlying cursor.
///
/// [`MessageReader::read_with_key`]: crate::message::MessageReader::read_with_key
#[derive(Debug)]
pub struct MessageWithKeyIter<'a, M: Message> {
    cursor: &'a mut MessageCursor<M>,
    chain: Chain<Iter<'a, (MessageKey<M>, M)>, Iter<'a, (MessageKey<M>, M)>>,
    unread: usize,
}

impl<'a, M: Message> MessageWithKeyIter<'a, M> {
    fn new(cursor: &'a mut MessageCursor<M>, messages: &'a MessageQueue<M>) -> Self {
        let unread = cursor.len(messages);
        cursor.last_index = messages.counter.wrapping_sub(unread);

        let a_index = cursor.last_index.wrapping_sub(messages.messages_a.start_id);
        let b_index = cursor.last_index.wrapping_sub(messages.messages_b.start_id);

        let a = messages.messages_a.get(a_index..).unwrap_or_default();
        let b = messages.messages_b.get(b_index..).unwrap_or_default();
        debug_assert_eq!(unread, a.len() + b.len());

        Self {
            cursor,
            chain: a.iter().chain(b.iter()),
            unread,
        }
    }

    /// Strips the ids, yielding only message references.
    pub fn without_key(self) -> MessageIterator<'a, M> {
        MessageIterator { iter: self }
    }
}

impl<'a, M: Message> Iterator for MessageWithKeyIter<'a, M> {
    type Item = (MessageKey<M>, &'a M);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(item) = self.chain.next() {
            self.cursor.last_index = self.cursor.last_index.wrapping_add(1);
            self.unread = self.unread.saturating_sub(1);
            Some((item.0, &item.1))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.unread, Some(self.unread))
    }

    fn count(self) -> usize {
        self.cursor.last_index = self.cursor.last_index.wrapping_add(self.unread);
        self.unread
    }

    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        let instance = self.chain.last()?;
        self.cursor.last_index = self.cursor.last_index.wrapping_add(self.unread);
        Some((instance.0, &instance.1))
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        if let Some(instance) = self.chain.nth(n) {
            let advanced = n.saturating_add(1);
            self.cursor.last_index = self.cursor.last_index.wrapping_add(advanced);
            self.unread = self.unread.saturating_sub(advanced);
            Some((instance.0, &instance.1))
        } else {
            self.cursor.last_index = self.cursor.last_index.wrapping_add(self.unread);
            self.unread = 0;
            None
        }
    }
}

impl<M: Message> ExactSizeIterator for MessageWithKeyIter<'_, M> {}
impl<M: Message> FusedIterator for MessageWithKeyIter<'_, M> {}

// -----------------------------------------------------------------------------
// MessageIterator

/// Iterator over unread messages.
///
/// This is the id-stripped form of [`MessageWithKeyIter`].
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
/// use zlim_reflect::derive::TypePath;
///
/// #[derive(TypePath, Message)]
/// struct Hit;
///
/// fn read_hits(mut reader: MessageReader<Hit>) {
///     for _ in reader.read() {}
/// }
///
/// // `MessageReader` wraps a queue plus a local cursor:
/// let mut queue = MessageQueue::<Hit>::default();
/// queue.write(Hit);
///
/// let mut cursor = MessageCursor::new(&queue);
/// assert_eq!(cursor.read(&queue).count(), 1);
/// assert!(cursor.is_empty(&queue));
/// ```
#[derive(Debug)]
pub struct MessageIterator<'a, M: Message> {
    iter: MessageWithKeyIter<'a, M>,
}

impl<'a, M: Message> Iterator for MessageIterator<'a, M> {
    type Item = &'a M;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(_, m)| m)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    fn count(self) -> usize {
        self.iter.count()
    }

    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.iter.last().map(|(_, m)| m)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.iter.nth(n).map(|(_, m)| m)
    }
}

impl<M: Message> ExactSizeIterator for MessageIterator<'_, M> {}
impl<M: Message> FusedIterator for MessageIterator<'_, M> {}

// -----------------------------------------------------------------------------
// MessageMutWithKeyIter

/// Iterator over unread messages with their message IDs.
///
/// Mutable counterpart of [`MessageWithKeyIter`].
///
/// Created by [`MessageCursor::read_mut_with_id`] and
/// [`MessageMutator::read_with_id`].
///
/// Consuming this iterator advances the underlying cursor.
///
/// [`MessageMutator::read_with_id`]: crate::message::MessageMutator::read_with_id
#[derive(Debug)]
pub struct MessageMutWithKeyIter<'a, M: Message> {
    cursor: &'a mut MessageCursor<M>,
    chain: Chain<IterMut<'a, (MessageKey<M>, M)>, IterMut<'a, (MessageKey<M>, M)>>,
    unread: usize,
}

impl<'a, M: Message> MessageMutWithKeyIter<'a, M> {
    fn new(cursor: &'a mut MessageCursor<M>, messages: &'a mut MessageQueue<M>) -> Self {
        let unread = cursor.len(messages);
        cursor.last_index = messages.counter.wrapping_sub(unread);

        let a_index = cursor.last_index.wrapping_sub(messages.messages_a.start_id);
        let b_index = cursor.last_index.wrapping_sub(messages.messages_b.start_id);

        let a = messages.messages_a.get_mut(a_index..).unwrap_or_default();
        let b = messages.messages_b.get_mut(b_index..).unwrap_or_default();
        debug_assert_eq!(unread, a.len() + b.len());

        Self {
            cursor,
            chain: a.iter_mut().chain(b.iter_mut()),
            unread,
        }
    }

    /// Strips the ids, yielding only mutable message references.
    pub fn without_id(self) -> MessageMutIterator<'a, M> {
        MessageMutIterator { iter: self }
    }
}

impl<'a, M: Message> Iterator for MessageMutWithKeyIter<'a, M> {
    type Item = (MessageKey<M>, &'a mut M);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(item) = self.chain.next() {
            self.cursor.last_index = self.cursor.last_index.wrapping_add(1);
            self.unread = self.unread.saturating_sub(1);
            Some((item.0, &mut item.1))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.unread, Some(self.unread))
    }

    fn count(self) -> usize {
        self.cursor.last_index = self.cursor.last_index.wrapping_add(self.unread);
        self.unread
    }

    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        let instance = self.chain.last()?;
        self.cursor.last_index = self.cursor.last_index.wrapping_add(self.unread);
        Some((instance.0, &mut instance.1))
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        if let Some(instance) = self.chain.nth(n) {
            let advanced = n.saturating_add(1);
            self.cursor.last_index = self.cursor.last_index.wrapping_add(advanced);
            self.unread = self.unread.saturating_sub(advanced);
            Some((instance.0, &mut instance.1))
        } else {
            self.cursor.last_index = self.cursor.last_index.wrapping_add(self.unread);
            self.unread = 0;
            None
        }
    }
}

impl<M: Message> ExactSizeIterator for MessageMutWithKeyIter<'_, M> {}
impl<M: Message> FusedIterator for MessageMutWithKeyIter<'_, M> {}

// -----------------------------------------------------------------------------
// MessageMutIterator

/// Iterator over unread mutable messages.
///
/// This is the id-stripped form of [`MessageMutWithKeyIter`].
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
/// fn clamp(mut mutator: MessageMutator<Damage>) {
///     for msg in mutator.read() {
///         msg.amount = msg.amount.min(100);
///     }
/// }
///
/// // `MessageMutator` mutates through the backing queue plus a local cursor:
/// let mut queue = MessageQueue::<Damage>::default();
/// queue.write(Damage { amount: 120 });
///
/// let mut cursor = MessageCursor::new(&queue);
/// for msg in cursor.read_mut(&mut queue) {
///     msg.amount = msg.amount.min(100);
/// }
/// assert_eq!(queue.get(0).map(|(_, m)| m.amount), Some(100));
/// ```
#[derive(Debug)]
pub struct MessageMutIterator<'a, M: Message> {
    iter: MessageMutWithKeyIter<'a, M>,
}

impl<'a, M: Message> Iterator for MessageMutIterator<'a, M> {
    type Item = &'a mut M;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(_, m)| m)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    fn count(self) -> usize {
        self.iter.count()
    }

    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.iter.last().map(|(_, m)| m)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.iter.nth(n).map(|(_, m)| m)
    }
}

impl<M: Message> ExactSizeIterator for MessageMutIterator<'_, M> {}
impl<M: Message> FusedIterator for MessageMutIterator<'_, M> {}

// -----------------------------------------------------------------------------
