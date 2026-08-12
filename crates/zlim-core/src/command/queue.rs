use core::fmt::Debug;
use core::mem::MaybeUninit;
use core::ptr::NonNull;

use zlim_utils::debug::DebugLocation;

use super::Command;
use crate::world::World;

// -----------------------------------------------------------------------------
// CommandQueue & CommandMeta
// -----------------------------------------------------------------------------

/// A queue for storing and executing deferred [`Command`]s.
///
/// `CommandQueue` stores commands as type-erased bytes in a
/// contiguous buffer, which is faster than `Box<dyn Command>`.
///
/// Internally, each queued item is encoded as `CommandMeta` followed by the
/// command payload bytes. During application, `cursor` marks the already
/// drained prefix, while the active pass processes `[start, stop)`.
pub struct CommandQueue {
    cursor: usize,
    bytes: Vec<MaybeUninit<u8>>,
    panic_recovery: Vec<MaybeUninit<u8>>,
    caller: DebugLocation,
}

/// A raw, non-owning handle to a [`CommandQueue`].
///
/// `RawCommandQueue` stores pointers into a parent [`CommandQueue`]'s
/// internal buffers (`bytes`, `cursor`, `panic_recovery`).  It is the
/// type-erased executor used by `World::flush` — commands are pushed and
/// applied through this handle without generic type parameters in the
/// hot path.
///
/// # Safety
///
/// All methods are `unsafe` because the pointed-to memory must remain
/// valid for the lifetime of the handle.  The parent [`CommandQueue`]
/// must not be dropped or moved while a `RawCommandQueue` derived from
/// it is in use.
#[derive(Clone)]
pub(crate) struct RawCommandQueue {
    cursor: NonNull<usize>,
    bytes: NonNull<Vec<MaybeUninit<u8>>>,
    panic_recovery: NonNull<Vec<MaybeUninit<u8>>>,
}

/// Function pointer used to execute (or drop) a command and advance the cursor.
#[repr(transparent)]
struct CommandMeta {
    /// - If world is Some(_), execute the command and move cursor.
    /// - If world is None, drop the command and move cursor.
    apply_or_drop: unsafe fn(value: NonNull<u8>, world: Option<NonNull<World>>, cursor: &mut usize),
}

// -----------------------------------------------------------------------------
// RawCommandQueue Methods
// -----------------------------------------------------------------------------

impl RawCommandQueue {
    /// Checks whether the queue is empty.
    ///
    /// # Safety
    /// The internal pointers must be valid.
    #[inline]
    pub unsafe fn is_empty(&self) -> bool {
        // SAFETY: Pointers are guaranteed to be valid by requirements on `.clone_unsafe`
        // It should be `>=`, because the `append` function does not modify the cursor (maybe).
        unsafe { *self.cursor.as_ref() >= self.bytes.as_ref().len() }
    }

    /// Appends a [`Command`] to the back of the queue.
    ///
    /// The command is stored inline as type-erased bytes in a packed
    /// `(CommandMeta, C)` layout.  This avoids a heap allocation per
    /// command and enables contiguous iteration during application.
    ///
    /// # Safety
    /// The internal pointers must be valid.
    #[inline] // Inline to reduce moving overhead.
    pub unsafe fn push<C: Command<Output = ()>>(&mut self, command: C) {
        // - `repr(C)` prevents the compiler from reordering the fields.
        // - `repr(packed)` prevents the compiler from inserting padding bytes.
        #[repr(C, packed)]
        struct Packed<C: Command<Output = ()>> {
            meta: CommandMeta,
            command: C,
        }

        let _: () = const {
            assert!(size_of::<CommandMeta>() + size_of::<C>() == size_of::<Packed<C>>());
        };

        let meta = CommandMeta {
            apply_or_drop: |command, world, cursor| {
                // Move cursor to the end of this Command.
                *cursor += size_of::<C>();

                // SAFETY: read_unaligned because the command pointer is unaligned.
                let command: C = unsafe { command.cast::<C>().read_unaligned() };
                if let Some(mut world) = world {
                    let world = unsafe { world.as_mut() };
                    command.apply(world);
                    // The command may have add new deferred commands for world,
                    // which we flush here to ensure they are also picked up.
                    world.flush();
                } else {
                    // If the input world is `None`, we drop the data directly.
                    ::core::hint::cold_path();
                    ::core::mem::drop(command)
                }
            },
        };

        unsafe {
            // Write command to queue
            let bytes = self.bytes.as_mut();
            let old_len = bytes.len();

            bytes.reserve(size_of::<Packed<C>>());

            let ptr = bytes.as_mut_ptr().add(old_len);

            // SAFETY: write_unaligned because the command pointer is unaligned.
            ptr.cast::<Packed<C>>()
                .write_unaligned(Packed { meta, command });
            bytes.set_len(old_len + size_of::<Packed<C>>());
        }
    }

    /// Applies or drops all commands in the queue.
    ///
    /// If `world` is `Some`, commands are applied to the world.
    /// If `world` is `None`, commands are dropped without execution.
    ///
    /// # Safety
    /// - The internal pointers of `World` must be valid.
    /// - The world access is exclusive.
    #[inline(always)]
    pub unsafe fn apply_or_drop(&mut self, world: Option<NonNull<World>>) {
        if unsafe { self.is_empty() } {
            return; // Optimize World::flush.
        }

        unsafe {
            self.apply_or_drop_inner(world);
        }
    }

    /// Applies or drops all commands in the queue.
    ///
    /// If `world` is `Some`, commands are applied to the world.
    /// If `world` is `None`, commands are dropped without execution.
    ///
    /// # Safety
    /// - The internal pointers must be valid.
    /// - The world access is exclusive.
    #[inline(never)]
    unsafe fn apply_or_drop_inner(&mut self, world: Option<NonNull<World>>) {
        let start = unsafe { *self.cursor.as_ref() };
        let stop = unsafe { self.bytes.as_ref().len() };
        let mut local_cursor = start;

        unsafe {
            *self.cursor.as_mut() = stop;
        }

        while local_cursor < stop {
            let bytes_ptr = unsafe { self.bytes.as_mut().as_mut_ptr() };

            // SAFETY: The cursor is either at the start of the buffer, or just after the previous command.
            // Since we know that the cursor is in bounds, it must point to the start of a new command.
            let meta: CommandMeta = unsafe {
                bytes_ptr
                    .add(local_cursor)
                    .cast::<CommandMeta>()
                    .read_unaligned()
            };

            // Advance to the bytes just after `meta`, which represent a type-erased command.
            local_cursor += size_of::<CommandMeta>();

            let cmd: NonNull<u8> =
                unsafe { NonNull::new_unchecked(bytes_ptr.add(local_cursor).cast()) };

            let f = core::panic::AssertUnwindSafe(|| {
                unsafe { (meta.apply_or_drop)(cmd, world, &mut local_cursor) };
            });

            if let Err(payload) = ::std::panic::catch_unwind(f) {
                let panic_recovery = unsafe { self.panic_recovery.as_mut() };
                let bytes = unsafe { self.bytes.as_mut() };
                let current_stop = bytes.len();

                // We need to use a stack to maintain order.
                panic_recovery.extend_from_slice(&bytes[local_cursor..current_stop]);

                unsafe {
                    bytes.set_len(start);
                    *self.cursor.as_mut() = start;
                }

                // Restore remaining commands when reaching the top-level apply pass.
                if start == 0 {
                    bytes.append(panic_recovery);
                }

                ::std::panic::resume_unwind(payload);
            }
        }

        unsafe {
            self.bytes.as_mut().set_len(start);
            *self.cursor.as_mut() = start;
        }
    }

    /// Moves all commands from `other` into `self`.
    ///
    /// After this operation, `other` becomes empty.
    ///
    /// The cursor will reset in `apply_or_drop`.
    ///
    /// # Safety
    /// - The internal pointers must be valid.
    #[inline]
    pub unsafe fn append(&mut self, other: &mut Self) {
        if self.bytes != other.bytes {
            unsafe {
                self.bytes.as_mut().append(other.bytes.as_mut());
                *other.cursor.as_mut() = 0;
            }
        }
    }
}

// -----------------------------------------------------------------------------
// CommandQueue Methods
// -----------------------------------------------------------------------------

unsafe impl Send for CommandQueue {}
unsafe impl Sync for CommandQueue {}

impl Debug for CommandQueue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CommandQueue")
            .field("bytes", &self.bytes.len())
            .field("caller", &self.caller)
            .finish_non_exhaustive()
    }
}

impl Drop for CommandQueue {
    fn drop(&mut self) {
        if !self.bytes.is_empty() {
            let caller = self.caller;
            log::warn!("CommandQueue has un-applied commands being dropped. {caller}");
            unsafe { self.raw().apply_or_drop(None) };
        }
    }
}

impl Default for CommandQueue {
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    fn default() -> Self {
        Self {
            bytes: Vec::new(),
            cursor: 0,
            panic_recovery: Vec::new(),
            caller: DebugLocation::caller(),
        }
    }
}

impl CommandQueue {
    /// Creates a raw handle to this command queue.
    #[inline]
    pub(crate) fn raw(&mut self) -> RawCommandQueue {
        RawCommandQueue {
            bytes: NonNull::from_mut(&mut self.bytes),
            cursor: NonNull::from_mut(&mut self.cursor),
            panic_recovery: NonNull::from_mut(&mut self.panic_recovery),
        }
    }
}

impl CommandQueue {
    /// Creates a new empty command queue.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub const fn new() -> Self {
        Self {
            bytes: Vec::new(),
            cursor: 0,
            panic_recovery: Vec::new(),
            caller: DebugLocation::caller(),
        }
    }

    /// Checks whether the queue contains any pending commands.
    #[inline]
    pub fn is_empty(&self) -> bool {
        // It should be `>=`, because the `append`
        // function does not modify the cursor.
        self.cursor >= self.bytes.len()
    }

    /// Moves all commands from `other` into `self`.
    ///
    /// After this operation, `other` becomes empty.
    #[inline]
    pub fn append(&mut self, other: &mut CommandQueue) {
        unsafe {
            self.raw().append(&mut other.raw());
        }
    }

    /// Applies all commands in the queue to the given `World`.
    ///
    /// This function first applies commands currently queued in `world`, then
    /// applies this queue. The queue is cleared after application.
    ///
    /// Calling `world.flush()` from inside command execution is supported:
    /// cursor bookkeeping ensures already-scheduled commands in the current
    /// pass are not re-applied.
    #[inline]
    pub fn apply(&mut self, world: &mut World) {
        world.apply_commands();
        unsafe {
            self.raw().apply_or_drop(Some(world.into()));
        }
    }

    /// Pushes a command into the queue.
    ///
    /// The command will be executed when [`apply`](Self::apply) is called.
    #[inline]
    pub fn push(&mut self, command: impl Command<Output = ()>) {
        unsafe {
            self.raw().push(command);
        }
    }
}
