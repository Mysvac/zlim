//! The [`System`] trait: the type-erased contract every runnable system implements.

#![expect(clippy::module_inception, reason = "For better structure.")]

use core::fmt::Debug;

use super::{AccessTable, SystemFlags};
use super::{SystemError, SystemId};
use crate::system::SystemInput;
use crate::tick::Tick;
use crate::world::DeferredWorld;
use crate::world::World;
use crate::world::WorldCell;

/// A system: a unit of logic that runs against a [`World`].
///
/// `System` is the type-erased contract every runnable system implements.
/// The trait is `Send + Sync + 'static` so systems can be scheduled and
/// executed in parallel, and it is implemented by the function-system
/// wrappers produced from closures and functions.
///
/// Most users never implement `System` directly — plain functions and
/// closures are converted into systems via [`IntoSystem`].
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// fn hello() {
///     println!("Hello, world!");
/// }
///
/// let mut world = World::alloc();
///
/// // `IntoSystem::into_system` converts the function into a `System`.
/// let mut system = IntoSystem::into_system(hello);
/// system.initialize(&world);
/// let result = system.run((), &mut world);
/// assert!(result.is_ok());
/// ```
///
/// [`IntoSystem`]: crate::system::IntoSystem
#[diagnostic::on_unimplemented(message = "`{Self}` is not a system", label = "invalid system")]
pub trait System: Send + Sync + 'static {
    /// The system's input type, describing how input values are passed into
    /// this system.
    type Input: SystemInput;

    /// The system's output type, produced after each run.
    type Output;

    /// Returns the stable [`SystemId`] identifying this system.
    fn id(&self) -> SystemId;

    /// Returns the system's behavioral flags.
    fn flags(&self) -> SystemFlags;

    /// Returns the [`Tick`] when this system last completed execution; used
    /// as the lower bound of the change-detection window.
    fn last_run(&self) -> Tick;

    /// Clamps this system's `last_run` tick to at most `now`.
    ///
    /// The scheduler uses this to keep change-detection windows bounded when
    /// the world's tick counter wraps around.
    fn clamp_ticks(&mut self, now: Tick);

    /// Sets the tick when this system last completed execution.
    fn set_last_run(&mut self, last_run: Tick);

    /// Initializes the system's persistent state.
    ///
    /// Using `world` to build any parameter state the system requires
    /// before its first run (e.g. `Local` values and query states).
    ///
    /// The implementer must ensure that this function is safe to be
    /// called repeatedly. And initialization should be skipped directly
    /// when called repeatedly to endure performance.
    fn initialize(&mut self, world: &World);

    /// Declares the world / resource access used by this system.
    ///
    /// When `strict` is true, conflicting access within this system is logged
    /// as an error before being force-merged; otherwise it merges silently.
    fn register_access(&self, table: &mut AccessTable, strict: bool);

    /// Executes the system's logic against the provided world, without
    /// applying deferred effects.
    ///
    /// This function does not initialize the System. If the System is not
    /// initialized, the call always returns [`SystemError::Uninitialized`]`.
    ///
    /// Due to the uncertain accessibility of World, this function will not
    /// handle delayed commands submitted.
    ///
    /// The safe [`System::run`] method wraps  `initialize`, this and
    /// `apply_deferred` pass afterwards.
    ///
    /// # Safety
    ///
    /// - The caller must ensure that the world's access patterns do not conflict
    ///   with other systems running concurrently.
    /// - The implementation must respect the access patterns declared in
    ///   [`System::register_access`] and not access components/resources outside
    ///   those patterns.
    /// - For `NON_SEND` systems, the caller must ensure execution occurs on the
    ///   same thread where the system was created.
    /// - For `EXCLUSIVE` systems, the caller must ensure exclusive world access.
    unsafe fn run_raw(
        &mut self,
        input: <Self::Input as SystemInput>::Data<'_>,
        world: WorldCell<'_>,
    ) -> Result<Self::Output, SystemError>;

    /// Executes the system's logic against the provided world, then applies
    /// any queued deferred effects.
    ///
    /// This function will also automatically initialize the job.
    fn run(
        &mut self,
        input: <Self::Input as SystemInput>::Data<'_>,
        world: &mut World,
    ) -> Result<Self::Output, SystemError> {
        self.initialize(world);
        let result = unsafe { self.run_raw(input, world.into()) };
        self.apply_deferred(world);
        result
    }

    /// Moves this system's queued deferred effects into the provided
    /// [`DeferredWorld`] view, so they can be applied later.
    ///
    /// The scheduler calls this only for systems whose flags include
    /// `DEFERRED`.
    fn queue_deferred(&mut self, world: DeferredWorld);

    /// Applies this system's queued deferred mutations to the [`World`].
    ///
    /// The scheduler calls this only for systems whose flags include
    /// `DEFERRED`.
    fn apply_deferred(&mut self, world: &mut World);
}

impl<I, O> Debug for dyn System<Input = I, Output = O>
where
    I: SystemInput + 'static,
    O: 'static,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("System").field(&self.id()).finish()
    }
}
