//! The [`SystemParam`] trait: statically-declared system parameters.

use super::{AccessTable, SystemParamError};
use crate::tick::Tick;
use crate::world::{DeferredWorld, World, WorldCell};

/// A statically-declared system parameter that fetches data from the [`World`]
/// during system execution.
///
/// System parameters are the building blocks of function system signatures:
/// every argument of a system function must implement `SystemParam`.  The
/// crate ships implementations for resource access ([`Res`] / [`ResMut`] in
/// `crate::borrow`), queries ([`Query`] / [`Single`] in `crate::query`),
/// [`Local`] state, [`Commands`], and tuples of parameters (up to 12
/// elements).
///
/// # Examples
///
/// Custom parameters are usually defined by composing existing ones with the
/// `SystemParam` derive:
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// #[derive(SystemParam)]
/// struct Greet<'w, 's> {
///     // Data borrowed from the world for one run.
///     world: &'w World,
///     // Data borrowed from the system's persistent state.
///     counter: Local<'s, u32>,
///     commands: Commands<'w, 's>,
/// }
///
/// fn greet_system(mut greet: Greet) {
///     *greet.counter += 1;
///     greet.commands.spawn((), None);
///     let _ = greet.world;
/// }
///
/// let mut world = World::alloc();
/// world.run_once(greet_system, ()).unwrap();
/// ```
///
/// # Safety
///
/// Implementors must ensure that [`SystemParam::build_param`] only accesses
/// world data declared through [`SystemParam::register_access`], and that
/// [`SystemParam::register_access`] reports the parameter's complete access
/// pattern.  Otherwise the scheduler may run conflicting systems in parallel,
/// causing undefined behavior.
///
/// [`Res`]: crate::borrow::Res
/// [`ResMut`]: crate::borrow::ResMut
/// [`Query`]: crate::query::Query
/// [`Single`]: crate::query::Single
/// [`Commands`]: crate::command::Commands
/// [`Local`]: crate::system::Local
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a valid `SystemParam`",
    label = "invalid `SystemParam`",
    note = "Consider annotating `{Self}` with `#[derive(SystemParam)]`."
)]
pub unsafe trait SystemParam: Sized {
    /// Persistent parameter state stored with the compiled system.
    type State: Send + Sync + 'static;

    /// Concrete parameter type produced for one system run.
    ///
    /// `'world` is tied to borrows coming from [`World`].
    /// `'state` is tied to borrows from [`SystemParam::State`].
    type Item<'world, 'state>: SystemParam<State = Self::State>;

    /// Whether this parameter requires `apply_deferred` to run.
    const DEFERRED: bool = false;

    /// Whether this parameter is thread-affine (`NonSend`).
    const NON_SEND: bool;

    /// Whether this parameter requires exclusive world access.
    const EXCLUSIVE: bool;

    /// Initializes persistent state for this parameter when a system is built.
    fn init_state(world: &World) -> Self::State;

    /// Declares world / resource access used by this parameter.
    ///
    /// When `strict` is `true`, a detected conflict is logged as an error
    /// before being force-merged; when `strict` is `false`, the conflict is
    /// merged silently.
    ///
    /// - Returns `true` if the access could be registered without any conflict.
    /// - Returns `false` if the access contains conflicts but was force
    ///   registered (merged).
    fn register_access(state: &Self::State, table: &mut AccessTable, strict: bool) -> bool;

    /// Fetches the per-run parameter value from world + state.
    ///
    /// # Safety
    ///
    /// Caller guarantees that `register_access` was used to validate conflicts
    /// for this parameter configuration before invoking `build_param`.
    ///
    /// Caller guarantees that the world mutability is correct.
    unsafe fn build_param<'w, 's>(
        state: &'s mut Self::State,
        world: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamError>;

    /// Queues deferred effects into a [`DeferredWorld`] view.
    #[inline(always)]
    #[expect(unused_variables, reason = "default implementation")]
    fn queue_deferred(state: &mut Self::State, world: DeferredWorld) {}

    /// Applies previously queued deferred effects to the real world.
    #[inline(always)]
    #[expect(unused_variables, reason = "default implementation")]
    fn apply_deferred(state: &mut Self::State, world: &mut World) {}
}
