//! The `Command` and `EntityCommand` traits.

#![expect(clippy::module_inception, reason = "For better structure.")]

use zlim_utils::debug::DebugName;

use crate::entity::{EntityError, EntityId};
use crate::error::{ErrorContext, ErrorHandler};
use crate::error::{IntoZlimResult, ZlimError};
use crate::ops::EntityOwned;
use crate::world::World;

// -----------------------------------------------------------------------------
// Command
// -----------------------------------------------------------------------------

/// A deferred world mutation.
///
/// Commands are the primary mechanism for structural ECS changes (spawn,
/// despawn, resource insertion) that must happen outside active system
/// execution.  Instead of calling `&mut World` methods directly, systems
/// push commands into a `CommandQueue`, which applies them in batch
/// after the schedule run.
///
/// # Blanket implementations
///
/// Any `FnOnce(&mut World) -> O` (where `O: IntoZlimResult<()>`) implements
/// `Command`, so closures can be used directly as commands.  The
/// [`Commands`] helpers (e.g. [`spawn_empty`]) also queue `impl Command`s.
///
/// # Error handling
///
/// Implementations that return errors can use the provided error-handling
/// combinators ([`handle_error`], [`handle_error_with`], [`ignore_error`])
/// to convert them into `Command<Output = ()>` suitable for queuing.
///
/// # Examples
///
/// Closures with the right signature are [`Command`]s automatically:
///
/// ```rust
/// use zlim_core::command::CommandQueue;
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone)]
/// struct Health(u32);
///
/// let mut world = World::alloc();
/// let mut queue = CommandQueue::new();
///
/// // A closure returning `()` can be pushed directly.
/// queue.push(|world: &mut World| {
///     world.spawn(Health(100), None);
/// });
///
/// // Commands are applied in batch, later.
/// queue.apply(&mut world);
/// assert_eq!(world.entity_count(), 1);
/// ```
///
/// [`Commands`]: crate::command::Commands
/// [`spawn_empty`]: crate::command::Commands::spawn_empty
/// [`handle_error`]: Self::handle_error
/// [`handle_error_with`]: Self::handle_error_with
/// [`ignore_error`]: Self::ignore_error
pub trait Command: Send + Sized + 'static {
    /// The return type of [`apply`], convertible to `Result<(), ZlimError>`.
    ///
    /// [`apply`]: Self::apply
    type Output: IntoZlimResult<()>;

    /// Executes the command on the given [`World`].
    ///
    /// This is called by the command queue during batch application.
    /// Implementations should perform their mutation and return `Ok(())`
    /// on success, or an error on failure.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::command::CommandQueue;
    /// use zlim_core::prelude::*;
    ///
    /// struct SpawnEmpty;
    ///
    /// impl Command for SpawnEmpty {
    ///     type Output = ();
    ///     fn apply(self, world: &mut World) {
    ///         world.spawn((), None);
    ///     }
    /// }
    ///
    /// let mut world = World::alloc();
    /// let mut queue = CommandQueue::new();
    ///
    /// queue.push(SpawnEmpty);
    /// queue.apply(&mut world);
    ///
    /// assert_eq!(world.entity_count(), 1);
    /// ```
    fn apply(self, world: &mut World) -> Self::Output;

    /// Handles command errors with a custom [`ErrorHandler`].
    ///
    /// Converts this command into one with `Output = ()`.  If [`apply`]
    /// returns an error, the provided handler is invoked with the error and
    /// a [`Command`] context.
    ///
    /// [`apply`]: Self::apply
    /// [`Command`]: crate::error::ErrorContext::Command
    /// [`ErrorHandler`]: crate::error::ErrorHandler
    #[inline]
    fn handle_error_with(self, handler: ErrorHandler) -> impl Command<Output = ()> {
        move |world: &mut World| {
            if let Err(e) = self.apply(world).into_zlim_result() {
                let name = DebugName::type_name::<Self>();
                handler(e, ErrorContext::Command { name });
            }
        }
    }

    /// Handles command errors with the world's default [`ErrorHandler`].
    ///
    /// Converts this command into one with `Output = ()`.  Errors from
    /// [`apply`] are forwarded to `world.error_handler`. Equivalent to
    /// `self.handle_error_with(world.error_handler)`.
    ///
    /// [`apply`]: Self::apply
    #[inline]
    fn handle_error(self) -> impl Command<Output = ()> {
        move |world: &mut World| {
            if let Err(e) = self.apply(world).into_zlim_result() {
                let name = DebugName::type_name::<Self>();
                (world.error_handler())(e, ErrorContext::Command { name });
            }
        }
    }

    /// Silently discards any error from the command.
    ///
    /// Converts this command into one with `Output = ()` that calls
    /// [`apply`](Self::apply) and ignores the return value.
    #[inline(always)]
    fn ignore_error(self) -> impl Command<Output = ()> {
        move |world: &mut World| {
            let _ = self.apply(world);
        }
    }
}

/// An entity-scoped deferred mutation.
///
/// Like [`Command`], but the mutation targets a specific entity identified
/// by its [`EntityId`].  Use [`with_entity`] to wrap an `EntityCommand` into
/// a regular [`Command`] that the queue can apply.
///
/// # Blanket implementations
///
/// Any `FnOnce(EntityOwned) -> O` (where `O: IntoZlimResult<()>`) implements
/// `EntityCommand`, so closures can be used directly.
///
/// # Examples
///
/// Closures over an [`EntityOwned`] are [`EntityCommand`]s automatically:
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
/// struct Health(u32);
///
/// // `Output = ()`, so this can be wrapped and queued directly.
/// let heal = |mut entity: EntityOwned| {
///     // `insert` is fallible; this example ignores the error.
///     let _ = entity.insert(Health(200));
/// };
///
/// let mut world = World::alloc();
/// let entity = world.spawn(Health(100), None).id();
///
/// let mut commands = world.commands();
/// commands.with_entity(entity).queue(heal);
///
/// drop(commands);
/// world.flush();
///
/// assert_eq!(
///     world.get_entity(entity).ok().and_then(|e| e.get::<Health>().cloned()),
///     Some(Health(200))
/// );
/// ```
///
/// [`with_entity`]: Self::with_entity
pub trait EntityCommand: Send + Sized + 'static {
    /// The return type of [`apply`], convertible to `Result<(), ZlimError>`.
    ///
    /// [`apply`]: Self::apply
    type Output: IntoZlimResult<()>;

    /// Executes the command on the given [`EntityOwned`].
    fn apply(self, entity: EntityOwned) -> Self::Output;

    /// Wraps this entity command into a [`Command`].
    ///
    /// The returned [`Command`] looks up the entity by its [`EntityId`]
    /// at application time and calls [`apply`]. If the entity is not found,
    /// an [`EntityError`] is returned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use zlim_core::command::CommandQueue;
    /// use zlim_core::prelude::*;
    ///
    /// #[derive(TypePath, Component, Clone, Debug, PartialEq)]
    /// struct Health(u32);
    ///
    /// let mut world = World::alloc();
    /// let entity = world.spawn(Health(100), None).id();
    ///
    /// let mut queue = CommandQueue::new();
    ///
    /// queue.push(
    ///     (|mut entity: EntityOwned| {
    ///         // `insert` is fallible; this example ignores the error.
    ///         let _ = entity.insert(Health(999));
    ///     })
    ///     .with_entity(entity)
    ///     .handle_error(),
    /// );
    /// queue.apply(&mut world);
    ///
    /// assert_eq!(
    ///     world.get_entity(entity).ok().and_then(|e| e.get::<Health>().cloned()),
    ///     Some(Health(999))
    /// );
    /// ```
    ///
    /// [`apply`]: Self::apply
    #[inline]
    fn with_entity(self, entity: EntityId) -> impl Command {
        move |world: &mut World| -> Packed<Self::Output, EntityError> {
            match world.get_entity_owned(entity) {
                Ok(v) => Packed::Next(self.apply(v)),
                Err(e) => Packed::Error(e),
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Packed
// -----------------------------------------------------------------------------

enum Packed<T, E> {
    Next(T),
    Error(E),
}

impl<T: IntoZlimResult<()>, E: Into<ZlimError>> IntoZlimResult<()> for Packed<T, E> {
    #[inline]
    fn into_zlim_result(self) -> Result<(), ZlimError> {
        match self {
            Packed::Next(x) => x.into_zlim_result(),
            Packed::Error(e) => Err(e.into()),
        }
    }
}

// -----------------------------------------------------------------------------
// Implementation
// -----------------------------------------------------------------------------

impl<F, O> Command for F
where
    F: FnOnce(&mut World) -> O + Send + 'static,
    O: IntoZlimResult<()>,
{
    type Output = O;

    #[inline(always)]
    fn apply(self, world: &mut World) -> O {
        self(world)
    }
}

impl<O, F> EntityCommand for F
where
    F: FnOnce(EntityOwned) -> O + Send + 'static,
    O: IntoZlimResult<()>,
{
    type Output = O;

    #[inline(always)]
    fn apply(self, entity: EntityOwned) -> Self::Output {
        self(entity)
    }
}

// -----------------------------------------------------------------------------
