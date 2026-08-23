//! An unbounded async MPSC (multi-producer, single-consumer) channel.
//!
//! Built on [`SegQueue`] and [`event_listener::Event`].
//!
//! The fixed internal implementation is lock-free and faster than `async-channel`
//! for the single-consumer use case.
//!
//! # Examples
//!
//! ```no_run
//! use zlim_utils::mpsc;
//! use futures_lite::future::block_on;
//!
//! let (tx, mut rx) = mpsc::channel::<i32>();
//!
//! tx.send(1);
//! tx.send(2);
//! drop(tx);
//!
//! block_on(async {
//!     assert_eq!(rx.recv().await, Some(1));
//!     assert_eq!(rx.recv().await, Some(2));
//!     assert_eq!(rx.recv().await, None);
//! });
//! ```

use core::fmt;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};
use core::task::{Context, Poll};
use std::sync::Arc;

use event_listener::{Event, EventListener, IntoNotification};

use crate::sync::SegQueue;

// -----------------------------------------------------------------------------
// channel

/// Creates an unbounded async MPSC channel.
///
/// ```rust
/// use zlim_utils::mpsc;
/// use futures_lite::future::block_on;
///
/// let (tx, mut rx) = mpsc::channel::<i32>();
///
/// tx.send(1);
/// tx.send(2);
/// drop(tx);
///
/// block_on(async {
///     assert_eq!(rx.recv().await, Some(1));
///     assert_eq!(rx.recv().await, Some(2));
///     assert_eq!(rx.recv().await, None);
/// });
/// ```
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Inner {
        queue: SegQueue::new(),
        event: Event::new(),
        send_num: AtomicUsize::new(1),
        is_closed: AtomicBool::new(false),
    });

    let tx = Sender {
        inner: inner.clone(),
    };
    let rx = Receiver { inner };

    (tx, rx)
}

// -----------------------------------------------------------------------------
// Inner

struct Inner<T> {
    queue: SegQueue<T>,
    event: Event,
    send_num: AtomicUsize,
    is_closed: AtomicBool,
}

// -----------------------------------------------------------------------------
// Sender

/// The sending half of an async MPSC channel.
///
/// Multiple `Sender`s can be created via [`Clone`].
/// When all senders are dropped, the [`Receiver`] yields `None`.
pub struct Sender<T> {
    inner: Arc<Inner<T>>,
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        // Relaxed: only atomicity is needed for the counter.
        self.inner.send_num.fetch_add(1, Relaxed);
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        // We cannot simply check `Arc::strong_count == 2`: the load and
        // the subsequent decrement are separate operations.  When two
        // Senders drop concurrently, both can observe `strong_count == 3`
        // (self + other Sender + Receiver), skip the notification, and
        // leave the Receiver waiting forever.  An independent `send_num`
        // atomic counter with `fetch_sub` solves this — the RMW operation
        // is atomic, so exactly one Sender sees `prev == 1`.
        let prev = self.inner.send_num.fetch_sub(1, Relaxed);
        if prev == 1 && !self.inner.is_closed.swap(true, AcqRel) {
            self.inner.event.notify(usize::MAX);
        }
    }
}

impl<T> Sender<T> {
    /// Closes the channel.
    ///
    /// Returns `true` if the channel was closed by this call,
    /// or `false` if it was already closed.
    pub fn close(&self) -> bool {
        // - old_val == true: already closed → return false.
        // - old_val == false: close now → return true.
        if self.inner.is_closed.swap(true, AcqRel) {
            false
        } else {
            self.inner.event.notify(usize::MAX);
            true
        }
    }

    /// Returns `true` if the channel is closed.
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed.load(Acquire)
    }

    /// Sends a value into the channel.
    ///
    /// Returns `Err(value)` if the channel is already closed.
    #[inline]
    pub fn send(&self, value: T) -> Result<(), T> {
        if self.inner.is_closed.load(Acquire) {
            core::hint::cold_path();
            Err(value)
        } else {
            self.inner.queue.push(value);
            // SegQueue ensures push (Release) is visible to pop (Acquire).
            // But notify must stay after push, so Event's internal ordering
            // is required, cannot be `.relaxed()`.
            self.inner.event.notify(1.additional());
            Ok(())
        }
    }
}

impl<T> fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sender")
    }
}

// -----------------------------------------------------------------------------
// Receiver

/// The receiving half of an async MPSC channel.
///
/// Only one `Receiver` exists per channel (it is not [`Clone`]).
/// When the `Receiver` is dropped, the channel is closed.
pub struct Receiver<T> {
    inner: Arc<Inner<T>>,
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.inner.is_closed.store(true, Release)
    }
}

impl<T> Receiver<T> {
    /// Closes the channel.
    ///
    /// Returns `true` if the channel was closed by this call,
    /// or `false` if it was already closed.
    #[inline]
    pub fn close(&self) -> bool {
        // - old_val == true: already closed → return false.
        // - old_val == false: close now → return true.
        !self.inner.is_closed.swap(true, AcqRel)
    }

    /// Returns `true` if the channel is closed.
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed.load(Acquire)
    }

    /// Receives the next value.
    ///
    /// Returns `None` when all [`Sender`]s have been dropped and the
    /// channel is empty.
    ///
    /// Takes `&mut self` so that only one in-flight [`Recv`] future exists
    /// per [`Receiver`] at a time.
    #[inline]
    pub fn recv(&mut self) -> Recv<'_, T> {
        Recv {
            rx: self,
            listener: None,
        }
    }

    /// Attempts to receive a value without waiting.
    ///
    /// Returns `None` if the channel is empty (not closed).
    #[inline]
    pub fn try_recv(&self) -> Option<T> {
        self.inner.queue.pop()
    }
}

impl<T> fmt::Debug for Receiver<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Receiver")
    }
}

// -----------------------------------------------------------------------------
// Recv

/// Future returned by [`Receiver::recv`].
pub struct Recv<'a, T> {
    rx: &'a Receiver<T>,
    /// Kept across polls so that the waker registered with
    /// [`Event`] survives — otherwise a `Sender::drop` that
    /// fires `event.notify()` while the future is parked would
    /// find no listener and the future would never wake.
    listener: Option<EventListener>,
}

impl<T> Future for Recv<'_, T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let this = self.get_mut();
        let inner = &this.rx.inner;

        loop {
            // Fast path — queue already has data
            if let Some(value) = inner.queue.pop() {
                return Poll::Ready(Some(value));
            }

            // All senders gone → drain and return
            if inner.is_closed.load(Acquire) {
                core::hint::cold_path();
                // Use pop for secondary inspection.
                return Poll::Ready(inner.queue.pop());
            }

            if let Some(listener) = &mut this.listener {
                match Pin::new(listener).poll(cx) {
                    Poll::Ready(()) => continue,
                    Poll::Pending => return Poll::Pending,
                }
            } else {
                // Create listener on first poll.
                core::hint::cold_path();
                this.listener = Some(inner.event.listen());
                // Check message queue again.
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use std::thread::{scope, spawn};

    use futures_lite::future::block_on;

    use super::channel;

    #[test]
    fn smoke() {
        let (tx, mut rx) = channel::<i32>();

        tx.send(1).unwrap();
        tx.send(2).unwrap();
        drop(tx);

        assert_eq!(block_on(rx.recv()), Some(1));
        assert_eq!(block_on(rx.recv()), Some(2));
        assert_eq!(block_on(rx.recv()), None);
    }

    #[test]
    fn try_recv() {
        let (tx, rx) = channel::<i32>();

        // Empty, not closed.
        assert_eq!(rx.try_recv(), None);

        tx.send(7).unwrap();
        assert_eq!(rx.try_recv(), Some(7));
        assert_eq!(rx.try_recv(), None);

        // Closed and empty.
        drop(tx);
        assert_eq!(rx.try_recv(), None);
    }

    #[test]
    fn sender_clone_keeps_channel_open() {
        let (tx, mut rx) = channel::<i32>();
        let tx2 = tx.clone();

        tx.send(1).unwrap();
        drop(tx);

        assert_eq!(block_on(rx.recv()), Some(1));

        // `tx2` is still alive, so the channel stays open.
        tx2.send(2).unwrap();
        assert_eq!(block_on(rx.recv()), Some(2));

        drop(tx2);
        assert_eq!(block_on(rx.recv()), None);
    }

    #[test]
    fn sender_close() {
        let (tx, mut rx) = channel::<i32>();

        assert!(!tx.is_closed());
        assert!(tx.close());
        assert!(!tx.close()); // already closed
        assert!(tx.is_closed());
        assert!(rx.is_closed());

        assert_eq!(tx.send(1), Err(1));
        assert_eq!(block_on(rx.recv()), None);
    }

    #[test]
    fn receiver_close() {
        let (tx, mut rx) = channel::<i32>();

        assert!(rx.close());
        assert!(!rx.close());
        assert!(tx.is_closed());

        assert_eq!(tx.send(1), Err(1));
        assert_eq!(block_on(rx.recv()), None);
    }

    #[test]
    fn receiver_drop_closes() {
        let (tx, rx) = channel::<i32>();

        drop(rx);
        assert!(tx.is_closed());
        assert_eq!(tx.send(1), Err(1));
    }

    #[test]
    fn recv_wakes_on_send() {
        let (tx, mut rx) = channel::<i32>();

        spawn(move || {
            tx.send(42).unwrap();
        });

        assert_eq!(block_on(rx.recv()), Some(42));
    }

    #[test]
    fn recv_wakes_on_sender_drop() {
        let (tx, mut rx) = channel::<i32>();

        spawn(move || drop(tx));

        assert_eq!(block_on(rx.recv()), None);
    }

    #[test]
    fn concurrent_senders() {
        #[cfg(miri)]
        const COUNT: usize = 50;
        #[cfg(not(miri))]
        const COUNT: usize = 10_000;
        const THREADS: usize = 4;

        let (tx, mut rx) = channel::<usize>();

        scope(|scope| {
            for _ in 0..THREADS {
                let tx = tx.clone();
                scope.spawn(move || {
                    for i in 0..COUNT {
                        tx.send(i).unwrap();
                    }
                });
            }
            // Drop the original sender; the channel closes once all
            // spawned senders have finished.
            drop(tx);
        });

        let mut total = 0;
        let mut count = 0;
        while let Some(v) = block_on(rx.recv()) {
            total += v;
            count += 1;
        }

        assert_eq!(count, COUNT * THREADS);
        assert_eq!(total, THREADS * COUNT * (COUNT - 1) / 2);
    }
}
