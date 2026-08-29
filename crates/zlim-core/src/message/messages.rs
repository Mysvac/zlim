//! Per-world registry of message types and their queue metadata.

use core::any::TypeId;
use core::fmt::Debug;
use core::sync::atomic::AtomicU32;

use zlim_utils::debug::{DebugLocation, DebugName};
use zlim_utils::ext::TypeMap;

use super::{Message, MessageId, MessageQueue};
use crate::borrow::UntypedMut;
use crate::tick::{DetectChanges, Tick};
use crate::world::World;

// -----------------------------------------------------------------------------
// MessageMeta

/// Compact runtime metadata for a registered message type.
///
/// This struct stores the stable [`MessageId`], the debug-friendly type name,
/// the [`TypeId`] used for lookups, the [`ResourceId`] of the backing
/// `MessageQueue<T>` resource, and a function pointer used to update (rotate)
/// the queue once per update (see [`MessageQueue::update`]).
///
/// [`MessageId`]: crate::message::MessageId
/// [`ResourceId`]: crate::resource::ResourceId
/// [`MessageQueue::update`]: crate::message::MessageQueue::update
pub struct MessageMeta {
    id: MessageId,
    type_id: TypeId,
    update: unsafe fn(UntypedMut),
    name: DebugName,
}

impl Debug for MessageMeta {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Message")
            .field("id", &self.id)
            .field("name", &self.name)
            .finish()
    }
}

impl MessageMeta {
    /// Returns the message's unique ID.
    #[inline(always)]
    pub fn id(&self) -> MessageId {
        self.id
    }

    /// Returns the message's [`DebugName`].
    ///
    /// [`DebugName`]: zlim_utils::debug::DebugName
    #[inline(always)]
    pub fn name(&self) -> DebugName {
        self.name
    }

    /// Returns the [`TypeId`] of the backing `MessageQueue<T>`.
    #[inline(always)]
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }
}

// -----------------------------------------------------------------------------
// should_update

const WAIT: u32 = 0;
const READY: u32 = 1;
const ALWAYS: u32 = 2;

// -----------------------------------------------------------------------------
// Messages

/// Registry of all message types registered in one [`World`].
///
/// Every [`World`] owns one `Messages` instance. Registering a message type
/// through [`World::register_message`] records its metadata here, including
/// the per-type queue rotation function (see [`MessageQueue::update`]).
///
/// # Example
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Message)]
/// struct Ping;
///
/// let mut world = World::alloc();
/// let id = world.register_message::<Ping>();
///
/// assert!(world.messages().get(id).is_some());
/// assert!(world.messages().get_name(id).is_some());
/// ```
///
/// [`World`]: crate::world::World
/// [`World::register_message`]: crate::world::World::register_message
/// [`MessageQueue::update`]: crate::message::MessageQueue::update
pub struct Messages {
    metas: Vec<MessageMeta>,
    mapper: TypeMap<MessageId>,
    should_update: AtomicU32,
    last_update: Tick,
}

impl Debug for Messages {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&self.metas, f)
    }
}

impl Messages {
    pub(crate) fn new() -> Self {
        Self {
            metas: Vec::new(),
            mapper: TypeMap::new(),
            last_update: Tick::new(0),
            should_update: AtomicU32::new(ALWAYS),
        }
    }
}

impl Messages {
    /// Returns the number of registered message types.
    #[inline]
    #[expect(clippy::len_without_is_empty, reason = "useless")]
    pub fn len(&self) -> usize {
        self.metas.len()
    }

    /// Looks up a message ID by its [`TypeId`].
    #[inline]
    pub fn get_id(&self, type_id: TypeId) -> Option<MessageId> {
        self.mapper.get(type_id).copied()
    }

    /// Returns the message debug name for the given ID.
    #[inline]
    pub fn get_name(&self, id: MessageId) -> Option<DebugName> {
        self.metas.get(id.index()).map(MessageMeta::name)
    }

    /// Returns the message metadata for the given ID.
    #[inline]
    pub fn get(&self, id: MessageId) -> Option<&MessageMeta> {
        self.metas.get(id.index())
    }

    /// Returns a slice containing the entire message metadata registry.
    #[inline]
    pub fn as_slice(&self) -> &[MessageMeta] {
        self.metas.as_slice()
    }

    /// Returns an iterator over the [`MessageMeta`] values.
    #[inline]
    pub fn iter(&self) -> core::slice::Iter<'_, MessageMeta> {
        self.metas.iter()
    }
}

// -----------------------------------------------------------------------------

/// Advances the double-buffered queue of message type `M` to the current
/// buffer.
///
/// # Safety
///
/// `untyped` must be a valid, aligned `UntypedMut` pointing to a
/// [`MessageQueue<M>`] instance.
unsafe fn update_queue<M: Message>(untyped: UntypedMut) {
    unsafe {
        untyped.into_resource::<MessageQueue<M>>().update();
    }
}

#[inline]
pub(crate) fn update_messages(world: &mut World) {
    let this_run: Tick = world.this_run_fast();
    world.messages.last_update.clamp_with(this_run);

    let should_update = world.messages.should_update.get_mut();

    match *should_update {
        WAIT => return, //
        READY => *should_update = WAIT,
        _ => {}
    }

    let last_run: Tick = world.messages.last_update;

    let resources = &mut world.resources;
    for meta in &world.messages.metas {
        let ty = meta.type_id;
        let Some(cell) = resources.get_mut(ty) else {
            continue;
        };
        let Some(untyped) = cell.get_mut(last_run, this_run) else {
            continue;
        };
        if untyped.is_changed() {
            unsafe { (meta.update)(untyped) };
        }
    }
}

#[inline]
pub(crate) fn enable_manual_update(this: &mut Messages) {
    match *this.should_update.get_mut() {
        WAIT | READY => {}
        _ => *this.should_update.get_mut() = WAIT,
    }
}

impl Messages {
    /// Registers message type `M` and records its metadata.
    ///
    /// If `M` is already registered this returns the existing [`MessageId`].
    /// The caller must ensure the backing `MessageQueue<M>` resource exists
    /// (see [`World::register_message`]).
    ///
    /// [`World::register_message`]: crate::world::World::register_message
    pub(crate) fn register<M: Message>(&mut self) -> MessageId {
        if let Some(id) = self.mapper.get(TypeId::of::<M>()) {
            return *id;
        }

        let id = MessageId::without_provenance(self.metas.len());
        let meta = MessageMeta {
            id,
            name: DebugName::type_name::<M>(),
            type_id: TypeId::of::<MessageQueue<M>>(),
            update: update_queue::<M>,
        };

        self.metas.push(meta);
        self.mapper.insert(TypeId::of::<M>(), id);

        id
    }
}

// -----------------------------------------------------------------------------

use crate::job::{Job, JobDB, JobId, JobLabel};
use crate::system::{AccessTable, SystemError, SystemFlags};
use crate::world::WorldCell;

/// A special job that signals when the current world's messages are ready to be updated.
///
/// All worlds check whether their message queue needs updating in the [`refresh_metadata`]
/// function. When using the `App`, this check is invoked at the start of each frame's
/// logical update (i.e., during `App::Update`).
///
/// By default, messages use an `Always` update policy, meaning they are refreshed on
/// every call to [`World::refresh_metadata`].
///
/// However, some worlds require a different update cadence. For instance, the main world
/// may be split into a main loop and a separate logic loop, where the logic loop typically
/// runs at a lower frequency than the main loop. In such cases, message updates should
/// align with the logic loop's fixed timestep.
///
/// To customize the update strategy, use [`World::enable_update_messages_signal`]. This
/// function transitions the message update state from `Always` to `Wait`. The
/// [`UpdateMessagesSignal`] job will then transition the state from `Wait` to `Ready`.
///
/// When `refresh_metadata` observes the state as `Ready`, it updates the message queue
/// and resets the state back to `Wait`. For the `Always` state, messages are also updated,
/// but the state remains unchanged.
///
/// # Warning
///
/// If [`UpdateMessagesSignal`] is enabled but [`World::enable_update_messages_signal`]
/// is never called, the update state will still change after the signal is first triggered.
///
/// This can lead to message loss because the system starts with the `Always` policy and
/// only switches behavior after the first signal emission.
///
/// [`refresh_metadata`]: World::refresh_metadata
pub struct UpdateMessagesSignal {
    id: JobId,
    last_run: Tick,
}

impl Job for UpdateMessagesSignal {
    // inline is useless for dynamic object

    fn id(&self) -> JobId {
        self.id
    }

    fn flags(&self) -> SystemFlags {
        SystemFlags::empty()
    }

    fn last_run(&self) -> Tick {
        self.last_run
    }

    fn clamp_ticks(&mut self, now: Tick) {
        self.last_run.clamp_with(now);
    }

    fn set_last_run(&mut self, last_run: Tick) {
        self.last_run = last_run;
    }

    fn initialize(&mut self, world: &World) {
        self.last_run = world.last_run();
    }

    fn register_access(&self, _: &mut AccessTable) {}

    #[inline]
    unsafe fn run_raw(&mut self, w: WorldCell<'_>) -> Result<(), SystemError> {
        use core::sync::atomic::Ordering::Relaxed;
        unsafe {
            w.read_only().messages.should_update.store(READY, Relaxed);
            Ok(())
        }
    }

    fn apply_deferred(&mut self, _: &mut World) {}
}

impl JobLabel for UpdateMessagesSignal {
    fn name() -> &'static str {
        "zlim_core::SignalMessagesUpdate"
    }

    fn database() -> JobDB {
        JobDB {
            name: "zlim_core::SignalMessagesUpdate",
            ctor: |group: &'static str| -> Box<dyn Job> {
                Box::new(Self {
                    id: JobId::new("zlim_core::SignalMessagesUpdate", group),
                    last_run: Tick::new(0),
                })
            },
            run_if: &[],
            location: DebugLocation::caller(),
        }
    }
}

crate::register_job!(UpdateMessagesSignal);
