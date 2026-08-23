//! Message identifiers and stream-local message keys.

use core::cmp::Ordering;
use core::fmt::{Debug, Display};
use core::marker::PhantomData;

use zlim_utils::debug::DebugName;

use super::Message;

// -----------------------------------------------------------------------------
// MessageId

crate::utils::define_ident!(
    /// A unique identifier for a `Message` type within a specific `World`.
    ///
    /// IDs are assigned sequentially when message types are registered
    /// through [`World::register_message`], so their meaning is only valid
    /// within the world that created them.
    ///
    /// # Example
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    /// use zlim_reflect::derive::TypePath;
    ///
    /// #[derive(TypePath, Message)]
    /// struct Ping;
    ///
    /// let mut world = World::alloc();
    /// let id = world.register_message::<Ping>();
    ///
    /// assert!(world.messages().get(id).is_some());
    /// ```
    ///
    /// [`World::register_message`]: crate::world::World::register_message
    MessageId
);

// -----------------------------------------------------------------------------
// MessageKey

/// Key for one message in a `MessageQueue<M>` stream.
///
/// `MessageKey` is backed by a wrapping `usize` counter. It is stable for
/// correlation within the stream (for example, tracking ids returned by
/// [`MessageQueue::write_batch`]), but callers should avoid treating it as a
/// globally monotonic timestamp across very long runtimes.
///
/// Ordering is wrap-aware and designed for stream-local comparisons.
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
/// ```
///
/// [`MessageQueue::write_batch`]: crate::message::MessageQueue::write_batch
pub struct MessageKey<M: Message> {
    index: usize,
    _marker: PhantomData<M>,
}

impl<M: Message> MessageKey<M> {
    #[inline(always)]
    pub(crate) const fn new(index: usize) -> Self {
        Self {
            index,
            _marker: PhantomData,
        }
    }

    /// Creates a `MessageKey` from a raw index without any stream context.
    #[inline(always)]
    pub const fn without_provenance(index: usize) -> Self {
        Self {
            index,
            _marker: PhantomData,
        }
    }

    /// Returns the underlying index.
    #[inline(always)]
    pub const fn index(self) -> usize {
        self.index
    }
}

impl<M: Message> Copy for MessageKey<M> {}

impl<M: Message> Clone for MessageKey<M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M: Message> Display for MessageKey<M> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "message<{}>#{}", M::type_name(), self.index)
    }
}

impl<M: Message> Debug for MessageKey<M> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "message<{}>#{}", DebugName::type_name::<M>(), self.index)
    }
}

impl<M: Message> PartialEq for MessageKey<M> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl<M: Message> Eq for MessageKey<M> {}

impl<M: Message> PartialOrd for MessageKey<M> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<M: Message> Ord for MessageKey<M> {
    fn cmp(&self, other: &Self) -> Ordering {
        // Non-wrapping difference between two generations after
        // which a signed interpretation becomes negative.
        const DIFF_MAX: usize = usize::MAX >> 1;

        match self.index.wrapping_sub(other.index) {
            0 => Ordering::Equal,
            1..DIFF_MAX => Ordering::Greater,
            _ => Ordering::Less,
        }
    }
}

impl<M: Message> core::hash::Hash for MessageKey<M> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write_usize(self.index);
    }
}

// -----------------------------------------------------------------------------
