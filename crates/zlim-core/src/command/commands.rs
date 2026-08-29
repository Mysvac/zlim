//! The `Commands` and `EntityCommands` deferred mutation interfaces.

use core::fmt::{Debug, Formatter};
use core::panic::{RefUnwindSafe, UnwindSafe};

use super::CommandQueue;
use super::function as func;
use crate::bundle::{Bundle, DataBundle};
use crate::command::{Command, EntityCommand};
use crate::entity::{EntityError, EntityId};
use crate::error::{ErrorHandler, IntoZlimResult};
use crate::message::Message;
use crate::resource::Resource;
use crate::schedule::ScheduleLabel;
use crate::system::{AccessTable, IntoSystem, SystemInput};
use crate::system::{SystemHandle, SystemParam, SystemParamError};
use crate::tick::Tick;
use crate::world::{DeferredWorld, FromWorld, World, WorldCell, WorldId};

// -----------------------------------------------------------------------------
// Commands

/// A deferred world-mutation interface.
///
/// `Commands` collects operations into a command queue and applies them later,
/// typically at the end of a schedule stage.
///
/// This lets systems request structural world changes (spawn, insert/remove
/// components, resource updates) without requiring immediate exclusive access
/// to `World` during system execution.
///
/// Queued commands are applied later at deferred synchronization points.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Tag;
///
/// // `Commands` is a system parameter; queued work is applied later.
/// fn setup(mut commands: Commands) {
///     commands.spawn(Tag, None);
/// }
///
/// let mut world = World::alloc();
/// let mut commands = world.commands();
/// setup(commands.reborrow());
///
/// ::core::mem::drop(commands);
/// world.flush();
/// assert_eq!(world.entity_count(), 1);
/// ```
pub struct Commands<'w, 's> {
    queue: &'s mut CommandQueue,
    world: &'w World,
}

unsafe impl Sync for Commands<'_, '_> {}
unsafe impl Send for Commands<'_, '_> {}
impl UnwindSafe for Commands<'_, '_> {}
impl RefUnwindSafe for Commands<'_, '_> {}

impl Debug for Commands<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "Commands(in World({}))", self.world.id())
    }
}

// -----------------------------------------------------------------------------
// EntityCommands

/// Entity-scoped command builder.
///
/// `EntityCommands` wraps a target [`EntityId`] plus a [`Commands`] handle,
/// making it ergonomic to enqueue multiple operations for the same entity.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
/// struct Hp(u32);
///
/// let mut world = World::alloc();
/// let player = world.spawn((), None).id();
///
/// let mut commands = world.commands();
/// // One handle can enqueue multiple operations for the same entity.
/// commands
///     .with_entity(player)
///     .insert(Hp(100))
///     .insert(Hp(150));
///
/// drop(commands);
/// world.flush();
///
/// assert_eq!(
///     world.get_entity(player).ok().and_then(|e| e.get::<Hp>().cloned()),
///     Some(Hp(150))
/// );
/// ```
pub struct EntityCommands<'a> {
    entity: EntityId,
    commands: Commands<'a, 'a>,
}

impl Debug for EntityCommands<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "EntityCommands(Entity({}) in World({}))",
            self.entity,
            self.commands.world.id()
        )
    }
}

// -----------------------------------------------------------------------------
// Methods

impl<'w, 's> Commands<'w, 's> {
    /// Creates a command writer from a world view and a target queue.
    ///
    /// Most users obtain this through the [`SystemParam`] implementation.
    #[inline]
    pub fn new(world: &'w World, queue: &'s mut CommandQueue) -> Self {
        Commands { queue, world }
    }

    /// Returns a new `Commands` that writes to the provided
    /// [`CommandQueue`] instead of the one from `self`.
    ///
    /// Useful when composing APIs that stage commands into dedicated queues.
    #[inline]
    pub fn rebound_to<'q>(&self, queue: &'q mut CommandQueue) -> Commands<'w, 'q> {
        Commands {
            queue,
            world: self.world,
        }
    }

    /// Returns a reborrowed command writer with a shorter lifetime.
    #[inline]
    pub fn reborrow(&mut self) -> Commands<'w, '_> {
        Commands {
            queue: self.queue,
            world: self.world,
        }
    }

    /// Returns the id of the [`World`] bound to this command writer.
    #[inline]
    pub fn world_id(&self) -> WorldId {
        self.world.id
    }

    /// Returns whether this queue currently has no pending commands.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Appends all commands from `other` into this queue, leaving `other` empty.
    #[inline]
    pub fn append(&mut self, other: &mut CommandQueue) {
        self.queue.append(other);
    }

    /// Returns an [`EntityCommands`] handle for the given [`EntityId`].
    ///
    /// Existence is validated when queued commands execute, not when queued.
    /// The entity may be despawned before application time.
    #[inline]
    pub fn with_entity(&mut self, entity: EntityId) -> EntityCommands<'_> {
        EntityCommands {
            entity,
            commands: self.reborrow(),
        }
    }

    /// Returns an [`EntityCommands`] handle if the entity exists at call time.
    ///
    /// This is an eager validation helper only. The entity can still be
    /// despawned before queued commands are applied.
    ///
    /// # Errors
    ///
    /// Returns [`EntityError`] if the requested entity does not currently exist.
    #[inline]
    pub fn with_entity_checked(
        &mut self,
        entity: EntityId,
    ) -> Result<EntityCommands<'_>, EntityError> {
        self.world.entities.get(entity)?;
        Ok(EntityCommands {
            entity,
            commands: self.reborrow(),
        })
    }

    /// Pushes a generic [`Command`] to the queue.
    ///
    /// If the [`Command`] returns a [`Result`], it will be handled
    /// using the world's [error handler](World::error_handler).
    ///
    /// To use a custom error handler, see [`Commands::queue_handled`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// let mut world = World::alloc();
    /// let mut commands = world.commands();
    ///
    /// commands.queue(|world: &mut World| {
    ///     world.spawn((), None);
    /// });
    ///
    /// drop(commands);
    /// world.flush();
    /// assert_eq!(world.entity_count(), 1);
    /// ```
    #[inline]
    pub fn queue(&mut self, cmd: impl Command) {
        self.queue.push(cmd.handle_error());
    }

    /// Pushes a generic [`Command`] to the queue.
    ///
    /// If the [`Command`] returns a [`Result`],
    /// the given `error_handler` will be used to handle error cases.
    ///
    /// To implicitly use the fallback error handler, see [`Commands::queue`].
    #[inline]
    pub fn queue_handled(&mut self, cmd: impl Command, handler: ErrorHandler) {
        self.queue.push(cmd.handle_error_with(handler));
    }

    /// Pushes a generic [`Command`] and silently ignores command errors.
    #[inline]
    pub fn queue_silenced(&mut self, cmd: impl Command) {
        self.queue.push(cmd.ignore_error());
    }

    /// Spawns an empty entity.
    ///
    /// This command is faster than `spawn((), parent)`.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn spawn_empty(&mut self, parent: Option<EntityId>) -> EntityCommands<'_> {
        let entity = self.world.alloc_entity();

        self.queue(func::spawn_empty_at(entity, parent));

        self.with_entity(entity)
    }

    /// Enqueues a spawn operation and returns the corresponding [`EntityCommands`].
    ///
    /// To spawn many entities with the same combination of components,
    /// [`spawn_batch`](Self::spawn_batch) can be used for better performance.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::prelude::*;
    ///
    /// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
    /// struct Health(u32);
    ///
    /// let mut world = World::alloc();
    /// let mut commands = world.commands();
    ///
    /// // The returned handle can queue further per-entity commands.
    /// let mut entity = commands.spawn(Health(100), None);
    /// let id = entity.id();
    /// entity.insert(Health(150));
    ///
    /// // Nothing is applied until the deferred flush point.
    /// drop(entity);
    /// drop(commands);
    /// world.flush();
    ///
    /// // The spawn and the insert were both applied.
    /// assert_eq!(world.entity_count(), 1);
    /// assert_eq!(
    ///     world.get_entity(id).ok().and_then(|e| e.get::<Health>().cloned()),
    ///     Some(Health(150))
    /// );
    /// ```
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn spawn<B: Bundle>(&mut self, bundle: B, parent: Option<EntityId>) -> EntityCommands<'_> {
        let entity = self.world.alloc_entity();

        self.queue(func::spawn_at(bundle, entity, parent));

        self.with_entity(entity)
    }

    /// Enqueues spawning multiple entities from a batch of [`DataBundle`] values.
    ///
    /// A batch can be any type that implements [`IntoIterator`] and
    /// contains bundles, such as a `Vec<Bundle>` or an array `[Bundle; N]`.
    ///
    /// This is equivalent to repeatedly calling [`spawn`](Self::spawn), but can
    /// be faster due to batched allocation and contiguous processing.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn spawn_batch<I>(&mut self, batch: I, parent: Option<EntityId>)
    where
        I: IntoIterator + Send + Sync + 'static,
        I::Item: DataBundle,
    {
        self.queue(func::spawn_batch(batch, parent));
    }

    /// Despawns an entity and removes all of its components.
    ///
    /// Logs at warn level if the entity is already despawned.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn despawn(&mut self, entity: EntityId) {
        self.queue(func::despawn(entity));
    }

    /// Despawns an entity and removes all of its components.
    ///
    /// No-op if the entity is already despawned.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn try_despawn(&mut self, entity: EntityId) {
        self.queue(func::try_despawn(entity));
    }

    /// Despawns many entities from iterator.
    ///
    /// No-op for entities that are already despawned.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn despawn_batch<I>(&mut self, batch: I)
    where
        I: IntoIterator<Item = EntityId> + Send + Sync + 'static,
    {
        self.queue(func::despawn_batch(batch));
    }

    /// Initializes a [`Resource`] in the [`World`] using [`FromWorld`].
    ///
    /// If the resource already exists, this is a no-op.
    #[inline]
    pub fn init_resource<R: Resource + Send + FromWorld>(&mut self) {
        self.queue(func::init_resource::<R>());
    }

    /// Inserts a [`Resource`] into the [`World`] with a specific value.
    ///
    /// This will overwrite any previous value of the same resource type.
    #[inline]
    pub fn insert_resource<R: Resource + Send>(&mut self, resource: R) {
        self.queue(func::insert_resource::<R>(resource));
    }

    /// Removes a [`Resource`] from the [`World`] if it exists.
    #[inline]
    pub fn remove_resource<R: Resource + Send>(&mut self) {
        self.queue(func::remove_resource::<R>());
    }

    /// Queues a command that writes a [`Message`] into the world.
    ///
    /// The message type must have been registered with
    /// [`World::register_message`] first.
    ///
    /// # Panics
    ///
    /// Panics when the command is applied if the message type is not
    /// registered.
    #[inline]
    pub fn write_message<M: Message>(&mut self, message: M) {
        self.queue(func::write_message(message));
    }

    /// Queues a command that runs the schedule with the given
    /// [`ScheduleLabel`].
    ///
    /// The schedule is created empty if it does not exist yet (see
    /// [`World::run_schedule`]).
    #[inline]
    pub fn run_schedule(&mut self, label: impl ScheduleLabel) {
        self.queue(func::run_schedule(label));
    }

    /// Queues a command that inserts a system into the world's cache.
    ///
    /// Note that this function only registers the System object and does not actively run it.
    ///
    /// The insertion itself is queued and applied with the rest of the
    /// commands.  The matching [`SystemHandle`] is derived from the system's
    /// type identity — obtain it from any instance of the same system type
    /// (e.g. via [`IntoSystem::system_handle`]) — and is valid immediately.
    /// Commands apply in queue order: queue [`Commands::insert_system`]
    /// before [`Commands::invoke_handle`] for the same system.
    #[inline]
    pub fn insert_system<I, O, M>(&mut self, system: impl IntoSystem<I, O, M> + Send + 'static)
    where
        I: SystemInput + 'static,
        O: 'static,
        M: 'static,
    {
        self.queue(func::insert_system(system));
    }

    /// Queues a command that removes a system from the world's cache.
    ///
    /// Does nothing if the handle was never inserted (or was already
    /// removed).
    #[inline]
    pub fn remove_system<I, O>(&mut self, handle: SystemHandle<I, O>)
    where
        I: SystemInput + 'static,
        O: 'static,
    {
        self.queue(func::remove_system(handle));
    }

    /// Queues a command that runs the given system, caching its instance.
    ///
    /// A cached instance with the same type identity is reused when the
    /// command is applied (see [`World::invoke`]); only systems with `()`
    /// output can be run through `Commands`.
    #[inline]
    pub fn invoke<I, O, M>(
        &mut self,
        system: impl IntoSystem<I, O, M> + Send + 'static,
        input: I::Data<'static>,
    ) where
        I: SystemInput<Data<'static>: Send> + Send + 'static,
        O: IntoZlimResult<()> + Send + 'static,
        M: 'static,
    {
        self.queue(func::invoke::<I, O, M>(system, input));
    }

    /// Queues a command that runs the cached system identified by `handle`.
    ///
    /// The system must have been inserted first (e.g. with
    /// [`Commands::insert_system`]); otherwise the command fails with
    /// [`SystemError::Unregistered`] when applied.  Only systems with `()`
    /// output can be run through `Commands`.
    ///
    /// [`SystemError::Unregistered`]: crate::system::SystemError::Unregistered
    #[inline]
    pub fn invoke_handle<I, O>(&mut self, handle: SystemHandle<I, O>, input: I::Data<'static>)
    where
        I: SystemInput<Data<'static>: Send> + Send + 'static,
        O: IntoZlimResult<()> + Send + 'static,
    {
        self.queue(func::invoke_handle::<I, O>(handle, input));
    }

    /// Queues a command that runs the given system once, without caching.
    ///
    /// A fresh instance is built, initialized, executed, and discarded at
    /// command-apply time (see [`World::invoke_once`]).  Only systems
    /// with `()` output can be run through `Commands`.
    #[inline]
    pub fn invoke_once<I, O, M>(
        &mut self,
        system: impl IntoSystem<I, O, M> + Send + 'static,
        input: I::Data<'static>,
    ) where
        I: SystemInput<Data<'static>: Send> + Send + 'static,
        O: IntoZlimResult<()> + Send + 'static,
        M: 'static,
    {
        self.queue(func::invoke_once::<I, O, M>(system, input));
    }
}

impl<'a> EntityCommands<'a> {
    /// Returns the target [`EntityId`].
    #[inline]
    pub fn id(&self) -> EntityId {
        self.entity
    }

    /// Returns the underlying [`Commands`].
    #[inline]
    pub fn into_inner(self) -> Commands<'a, 'a> {
        self.commands
    }

    /// Returns an [`EntityCommands`] reborrow with a shorter lifetime.
    ///
    /// This is useful if you have `&mut EntityCommands` but you need `EntityCommands`.
    #[inline]
    pub fn reborrow(&mut self) -> EntityCommands<'_> {
        EntityCommands {
            entity: self.entity,
            commands: self.commands.reborrow(),
        }
    }

    /// Returns the underlying [`Commands`].
    #[inline]
    pub fn commands(&mut self) -> Commands<'_, '_> {
        self.commands.reborrow()
    }

    /// Returns a mutable reference to the underlying [`Commands`].
    #[inline]
    pub fn commands_mut(&mut self) -> &mut Commands<'a, 'a> {
        &mut self.commands
    }

    /// Pushes an [`EntityCommand`] for this entity.
    ///
    /// The world's [error handler] will be used to handle error cases.
    /// Every [`EntityCommand`] checks whether the entity exists at the time
    /// of execution and returns an error if it does not.
    ///
    /// To use a custom error handler, see [`EntityCommands::queue_handled`].
    ///
    /// [error handler]: crate::world::World::error_handler
    #[inline]
    pub fn queue(&mut self, command: impl EntityCommand) -> &mut Self {
        self.commands.queue(command.with_entity(self.entity));
        self
    }

    /// Pushes an [`EntityCommand`] for this entity with a custom error handler.
    ///
    /// The given `error_handler` will be used to handle error cases. Every [`EntityCommand`] checks
    /// whether the entity exists at the time of execution and returns an error if it does not.
    ///
    /// To implicitly use the fallback error handler, see [`EntityCommands::queue`].
    #[inline]
    pub fn queue_handled(
        &mut self,
        command: impl EntityCommand,
        handler: ErrorHandler,
    ) -> &mut Self {
        self.commands
            .queue_handled(command.with_entity(self.entity), handler);
        self
    }

    /// Pushes an [`EntityCommand`] for this entity and ignores errors.
    ///
    /// Unlike [`EntityCommands::queue_handled`], this will completely ignore any errors that occur.
    #[inline]
    pub fn queue_silenced(&mut self, command: impl EntityCommand) -> &mut Self {
        self.commands
            .queue_silenced(command.with_entity(self.entity));
        self
    }

    /// Adds a [`Bundle`] of components to the entity.
    ///
    /// This will overwrite any previous value(s) of the same component type.
    ///
    /// If the entity does not exist, this command will log a warning.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::derive::Component;
    /// use zlim_core::prelude::*;
    /// use zlim_reflect::derive::TypePath;
    ///
    /// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
    /// struct Hp(u32);
    ///
    /// let mut world = World::alloc();
    /// let entity = world.spawn(Hp(100), None).id();
    ///
    /// let mut commands = world.commands();
    /// commands.with_entity(entity).insert(Hp(150));
    /// drop(commands);
    /// world.flush();
    ///
    /// assert_eq!(
    ///     world.get_entity(entity).ok().and_then(|e| e.get::<Hp>().cloned()),
    ///     Some(Hp(150))
    /// );
    /// ```
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn insert(&mut self, bundle: impl Bundle) -> &mut Self {
        self.queue(func::insert(bundle))
    }

    /// Adds a [`Bundle`] of components to the entity.
    ///
    /// Errors are ignored if the entity is despawned before command execution.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn try_insert(&mut self, bundle: impl Bundle) -> &mut Self {
        self.queue_silenced(func::insert(bundle))
    }

    /// Inserts a [`Bundle`] into an entity if missing.
    ///
    /// If the entity does not exist, this command will log a warning.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn insert_if_new<T: DataBundle, F>(&mut self, bundle: F) -> &mut Self
    where
        F: FnOnce() -> T + Send + 'static,
    {
        self.queue_silenced(func::insert_if_new(bundle))
    }

    /// Inserts a [`Bundle`] into an entity if missing.
    ///
    /// Errors are ignored if the entity is despawned before command execution.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn try_insert_if_new<T: DataBundle, F>(&mut self, bundle: F) -> &mut Self
    where
        F: FnOnce() -> T + Send + 'static,
    {
        self.queue_silenced(func::insert_if_new(bundle))
    }

    /// Removes all explicit component types in a [`Bundle`] from the entity.
    ///
    /// As same as [`Self::remove_explicit`].
    ///
    /// If the entity does not exist, this command will log a warning.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn remove<B: DataBundle>(&mut self) -> &mut Self {
        self.queue(func::remove::<B>())
    }

    /// Removes all explicit component types in a [`Bundle`] from the entity.
    ///
    /// As same as [`Self::try_remove_explicit`].
    ///
    /// Errors are ignored if the entity is despawned before command execution.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn try_remove<B: DataBundle>(&mut self) -> &mut Self {
        self.queue_silenced(func::remove::<B>())
    }

    /// Removes all component types in a [`Bundle`] from the entity.
    ///
    /// The required components included in the bundle will also be removed (if they are not dependent on other components).
    ///
    /// If the entity does not exist, this command will log a warning.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn remove_required<B: DataBundle>(&mut self) -> &mut Self {
        self.queue(func::remove_required::<B>())
    }

    /// Removes all explicit component types in a [`Bundle`] from the entity.
    ///
    /// If the entity does not exist, this command will log a warning.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn try_remove_required<B: DataBundle>(&mut self) -> &mut Self {
        self.queue_silenced(func::remove_required::<B>())
    }

    /// Removes all explicit component types in a [`Bundle`] from the entity.
    ///
    /// Errors are ignored if the entity is despawned before command execution.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn remove_explicit<B: DataBundle>(&mut self) -> &mut Self {
        self.queue(func::remove_explicit::<B>())
    }

    /// Removes all component types in a [`Bundle`] from the entity.
    ///
    /// The required components included in the bundle will also be removed (if they are not dependent on other components).
    ///
    /// Errors are ignored if the entity is despawned before command execution.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn try_remove_explicit<B: DataBundle>(&mut self) -> &mut Self {
        self.queue_silenced(func::remove_explicit::<B>())
    }

    /// Removes all components from this entity.
    ///
    /// The entity is moved to the empty archetype; its children are left
    /// untouched.
    ///
    /// If the entity does not exist, this command will log a warning.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn clear(&mut self) -> &mut Self {
        self.queue(func::clear())
    }

    /// Removes all components from this entity.
    ///
    /// Errors are ignored if the entity is despawned before command execution.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn try_clear(&mut self) -> &mut Self {
        self.queue_silenced(func::clear())
    }

    /// Clones this entity.
    ///
    /// Note that this function returns `self`, instead of cloned new entity.
    ///
    /// If the entity does not exist, this command will log a warning.
    ///
    /// - If `recursive` is set to true, it will recursively clone sub entities.
    /// - If `recursive` is set to false, it will clone self and skip all sub entities.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn clone(&mut self, recursive: bool) -> &mut Self {
        self.queue(func::clone(recursive))
    }

    /// Clones this entity.
    ///
    /// Note that this function returns `self`, instead of cloned new entity.
    ///
    /// - If `recursive` is set to true, it will recursively clone sub entities.
    /// - If `recursive` is set to false, it will clone self and skip all sub entities.
    ///
    /// Errors are ignored if the entity is despawned before command execution.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn try_clone(&mut self, recursive: bool) -> &mut Self {
        self.queue_silenced(func::clone(recursive))
    }

    /// ReparentSignal an entity.
    ///
    /// - Pass `None` to detach this entity from its current parent
    ///   (making it a root entity).
    ///
    /// - Pass `Some(id)` to make `id` the new parent.
    ///
    /// If the entity (or parent) does not exist, this command will log a warning.
    ///
    /// Note that if the parent entity does not exist, the operation will not take effect.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn reparent(&mut self, parent: Option<EntityId>) -> &mut Self {
        self.queue(func::reparent(parent))
    }

    /// ReparentSignal an entity.
    ///
    /// - Pass `None` to detach this entity from its current parent
    ///   (making it a root entity).
    ///
    /// - Pass `Some(id)` to make `id` the new parent.
    ///
    /// If parent entity does not exist, the operation will not take effect.
    ///
    /// Errors are ignored if the entity is despawned before command execution,
    /// or the parent entity is already despawned.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn try_reparent(&mut self, parent: Option<EntityId>) -> &mut Self {
        self.queue_silenced(func::reparent(parent))
    }

    /// Despawns an entity and removes all of its components.
    ///
    /// If the entity does not exist, this command will log a warning.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn despawn(mut self) {
        self.commands.despawn(self.entity);
    }

    /// Despawns an entity and removes all of its components.
    ///
    /// No-op if the entity is already despawned.
    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    pub fn try_despawn(mut self) {
        self.commands.try_despawn(self.entity);
    }
}

// -----------------------------------------------------------------------------
// SystemParam Implementation

unsafe impl SystemParam for Commands<'_, '_> {
    type State = CommandQueue;
    type Item<'world, 'state> = Commands<'world, 'state>;

    const DEFERRED: bool = true;
    const NON_SEND: bool = false;
    const EXCLUSIVE: bool = false;

    #[inline(always)]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    fn init_state(_: &World) -> Self::State {
        CommandQueue::new()
    }

    #[inline(always)]
    fn register_access(_: &Self::State, _: &mut AccessTable, _: bool) -> bool {
        true
    }

    #[inline(always)]
    unsafe fn build_param<'w, 's>(
        state: &'s mut Self::State,
        world: WorldCell<'w>,
        _last_run: Tick,
        _this_run: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError> {
        Ok(Commands::new(unsafe { world.read_only() }, state))
    }

    #[inline(never)]
    fn queue_deferred(state: &mut Self::State, world: DeferredWorld) {
        unsafe {
            world.cell().data_mut().command_queue.append(state);
        }
    }

    #[inline(never)]
    fn apply_deferred(state: &mut Self::State, world: &mut World) {
        state.apply(world);
    }
}

// -----------------------------------------------------------------------------
