//! The `CommandQueue` and its raw executor.

use core::any::Any;
use core::fmt::Debug;
use core::mem::MaybeUninit;
use core::panic::AssertUnwindSafe;
use core::ptr;
use core::ptr::NonNull;
use std::panic::catch_unwind;

use zlim_utils::debug::{DebugLocation, DebugName};

use super::Command;
use crate::error::PanicPayload;
use crate::world::{World, WorldCell};

// -----------------------------------------------------------------------------
// CommandQueue & CommandMeta
// -----------------------------------------------------------------------------

/// A queue for storing and executing deferred [`Command`]s.
///
/// `CommandQueue` stores commands as type-erased bytes in a
/// contiguous buffer, which is faster than `Box<dyn Command>`.
///
/// Dropping a queue that still holds unapplied commands emits a warning; use
/// [`silent`](Self::silent) or [`silence_on_unapplied`](Self::silence_on_unapplied)
/// to suppress it.
///
/// # Examples
///
/// ```rust
/// use zlim_core::command::CommandQueue;
/// use zlim_core::prelude::*;
///
/// let mut world = World::alloc();
/// let mut queue = CommandQueue::new();
///
/// // Commands with `Output = ()` are pushed directly...
/// queue.push(|world: &mut World| {
///     world.spawn((), None);
/// });
///
/// // Applying executes every queued command and clears the queue.
/// queue.apply(&mut world);
/// assert_eq!(world.entity_count(), 1);
/// ```
pub struct CommandQueue {
    /// This buffer densely stores all queued commands.
    bytes: Vec<MaybeUninit<u8>>,
    caller: DebugLocation,
    /// Always emit a warning if a command is dropped before it is applied.
    warn_on_unapplied: bool,
}

/// Function pointer used to execute (or drop) a command and advance the cursor.
///
/// - If world is Some(_), execute the command and move cursor.
/// - If world is None, drop the command and move cursor.
type CommandMeta = unsafe fn(value: NonNull<u8>, world: Option<NonNull<World>>, cursor: &mut usize);

// -----------------------------------------------------------------------------
// Basic Methods
// -----------------------------------------------------------------------------

unsafe impl Send for CommandQueue {}
unsafe impl Sync for CommandQueue {}

impl Debug for CommandQueue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CommandQueue")
            .field("len_bytes", &self.bytes.len())
            .field("caller", &self.caller)
            .finish_non_exhaustive()
    }
}

impl Drop for CommandQueue {
    fn drop(&mut self) {
        if self.bytes.is_empty() {
            return;
        }
        ::core::hint::cold_path();

        if self.warn_on_unapplied {
            zlim_log::warn!(
                "Dropping a CommandQueue with unapplied commands (defined at {}). \
                 These commands will not be executed.",
                self.caller,
            );
        }

        let bytes_ptr: NonNull<u8> =
            unsafe { NonNull::new_unchecked(self.bytes.as_mut_ptr() as *mut u8) };

        let mut cursor: usize = 0;
        let end: usize = self.bytes.len();

        while cursor < end {
            unsafe {
                let meta: CommandMeta = bytes_ptr
                    .byte_add(cursor)
                    .cast::<CommandMeta>()
                    .read_unaligned();

                cursor += ::core::mem::size_of::<CommandMeta>();

                let value: NonNull<u8> = bytes_ptr.byte_add(cursor);
                meta(value, None, &mut cursor);
            }
        }
    }
}

// -----------------------------------------------------------------------------
// ctor
// -----------------------------------------------------------------------------

impl Default for CommandQueue {
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    fn default() -> Self {
        Self {
            bytes: Vec::new(),
            caller: DebugLocation::caller(),
            warn_on_unapplied: true,
        }
    }
}

impl CommandQueue {
    /// Creates a new empty command queue with given caller.
    #[inline]
    pub const fn with_caller(caller: DebugLocation) -> Self {
        Self {
            bytes: Vec::new(),
            caller,
            warn_on_unapplied: true,
        }
    }

    /// Creates a new empty command queue.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub const fn new() -> Self {
        Self {
            bytes: Vec::new(),
            caller: DebugLocation::caller(),
            warn_on_unapplied: true,
        }
    }

    /// Create a queue that does not warn when dropped.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub const fn silent() -> Self {
        Self {
            bytes: Vec::new(),
            caller: DebugLocation::caller(),
            warn_on_unapplied: false,
        }
    }

    /// Warning on drop if commands are unapplied.
    #[inline(always)]
    pub fn warn_on_unapplied(&mut self) {
        self.warn_on_unapplied = true;
    }

    /// Silences drop warning if commands are unapplied.
    #[inline(always)]
    pub fn silence_on_unapplied(&mut self) {
        self.warn_on_unapplied = false;
    }

    /// Returns the len of bytes in the queue.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns `true` if there are no commands in the queue.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Returns false if there are any commands in the queue
    #[inline(always)]
    pub fn append(&mut self, other: &mut CommandQueue) {
        self.bytes.append(&mut other.bytes);
    }
}

// -----------------------------------------------------------------------------
// push
// -----------------------------------------------------------------------------

#[cold]
#[inline(never)]
fn propagate_panic(payload: Box<dyn Any + Send>, name: DebugName) -> ! {
    match payload.downcast::<PanicPayload>() {
        Ok(panic_payload) => std::panic::resume_unwind(panic_payload),
        #[expect(clippy::print_stderr, reason = "panic outout")]
        Err(payload) => {
            ::core::hint::cold_path();
            std::eprintln!("Encounter a unexpected panic in command `{}`.", name);
            std::panic::resume_unwind(Box::new(PanicPayload { payload }))
        }
    }
}

impl CommandQueue {
    /// Appends a [`Command`] to the back of the queue.
    ///
    /// The command is stored inline as type-erased bytes in a packed
    /// `(CommandMeta, C)` layout.  This avoids a heap allocation per
    /// command and enables contiguous iteration during application.
    #[inline] // Inline to reduce moving overhead.
    pub fn push<C: Command<Output = ()>>(&mut self, command: C) {
        let meta: CommandMeta = |command, world, cursor| {
            // Move cursor to the end of this Command.
            *cursor += const { ::core::mem::size_of::<C>() };

            let func = || {
                // SAFETY: read_unaligned because the command pointer is unaligned.
                let command: C = unsafe { command.cast::<C>().read_unaligned() };

                if let Some(mut world) = world {
                    let world = unsafe { world.as_mut() };
                    command.apply(world);
                    // The command may have add new deferred commands for world,
                    // which we flush here to ensure they are also picked up.
                    if world.command_start < world.command_queue.len() {
                        ::core::hint::cold_path();
                        flush_world(world); // not inline
                    }
                } else {
                    // If the input world is `None`, we drop the data directly.
                    ::core::hint::cold_path();
                    ::core::mem::drop(command);
                }
            };

            if let Err(payload) = catch_unwind(AssertUnwindSafe(func)) {
                propagate_panic(payload, DebugName::type_name::<C>());
            }
        };

        unsafe {
            // Write command to queue
            let bytes: &mut Vec<MaybeUninit<u8>> = self.bytes.as_mut();
            let meta_offset: usize = bytes.len();
            let data_offset: usize = meta_offset + const { size_of::<CommandMeta>() };

            let packed_length: usize = const { size_of::<CommandMeta>() + size_of::<C>() };
            let new_length: usize = meta_offset + packed_length;

            bytes.reserve(packed_length);

            // unpacked (meta, data)
            let base_ptr: *mut MaybeUninit<u8> = bytes.as_mut_ptr();
            let meta_ptr: *mut MaybeUninit<u8> = base_ptr.add(meta_offset);
            let data_ptr: *mut MaybeUninit<u8> = base_ptr.add(data_offset);

            // SAFETY: write_unaligned because the command pointer is unaligned.
            meta_ptr.cast::<CommandMeta>().write_unaligned(meta);
            data_ptr.cast::<C>().write_unaligned(command);

            bytes.set_len(new_length);
        }
    }
}

// -----------------------------------------------------------------------------
// Runner
// -----------------------------------------------------------------------------

struct CommandRunner<'a> {
    queue: &'a mut CommandQueue,
    start: usize,
    stop: usize,
    cursor: usize,
}

impl Drop for CommandRunner<'_> {
    fn drop(&mut self) {
        if self.cursor < self.stop {
            self.clean(); // not inline
        }
        unsafe { self.queue.bytes.set_len(self.start) };
    }
}

impl CommandRunner<'_> {
    fn new(queue: &mut CommandQueue, start: usize) -> CommandRunner<'_> {
        let stop = queue.bytes.len();
        CommandRunner {
            queue,
            cursor: start,
            start,
            stop,
        }
    }

    #[cold]
    #[inline(never)]
    fn clean(&mut self) {
        let bytes_ptr: NonNull<u8> =
            unsafe { NonNull::new_unchecked(self.queue.bytes.as_mut_ptr() as *mut u8) };

        while self.cursor < self.stop {
            unsafe {
                let meta: CommandMeta = bytes_ptr
                    .byte_add(self.cursor)
                    .cast::<CommandMeta>()
                    .read_unaligned();

                self.cursor += ::core::mem::size_of::<CommandMeta>();

                let value: NonNull<u8> = bytes_ptr.byte_add(self.cursor);
                meta(value, None, &mut self.cursor);
            }
        }
    }

    fn run(&mut self, world: &mut World) {
        let bytes_ptr: NonNull<u8> =
            unsafe { NonNull::new_unchecked(self.queue.bytes.as_mut_ptr() as *mut u8) };

        let world: Option<NonNull<World>> = Some(NonNull::from_mut(world));

        while self.cursor < self.stop {
            unsafe {
                let meta: CommandMeta = bytes_ptr
                    .byte_add(self.cursor)
                    .cast::<CommandMeta>()
                    .read_unaligned();

                self.cursor += ::core::mem::size_of::<CommandMeta>();

                let value: NonNull<u8> = bytes_ptr.byte_add(self.cursor);
                meta(value, world, &mut self.cursor);
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Apply

impl CommandQueue {
    pub fn apply(&mut self, world: &mut World) {
        // flush the world's internal queue
        if world.command_start < world.command_queue.len() {
            ::core::hint::cold_path();
            flush_world(world); // not inline
        }

        debug_assert!(
            !ptr::eq(self, &world.command_queue),
            "Attempted to apply CommandQueue to the same World it belongs to. \
            This would cause infinite recursion. Use world.flush() instead.",
        );

        CommandRunner::new(self, 0).run(world);
    }
}

#[inline(never)]
pub(crate) fn flush_world(world: &mut World) {
    struct Guard<'a> {
        world: WorldCell<'a>,
        start: usize,
    }

    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            let world = unsafe { self.world.data_mut() };
            // Return `command_start` to its original value.
            world.command_start = self.start;

            // `CommandRunner` should set `len()` to `start`.
            debug_assert_eq!(world.command_queue.len(), self.start);
        }
    }

    let start = world.command_start;
    let stop = world.command_queue.len();
    let world = world.cell();
    let _guard = Guard { world, start };

    unsafe {
        world.data_mut().command_start = stop;
        let queue = &mut world.data_mut().command_queue;
        let mut runner = CommandRunner::new(queue, start);
        runner.run(world.full_mut());
        ::core::mem::drop(runner);
    }
}

// -----------------------------------------------------------------------------
