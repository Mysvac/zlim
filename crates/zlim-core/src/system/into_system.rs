//! The [`IntoSystem`] trait and its system combinator wrappers.

use core::marker::PhantomData;

use super::access::AccessTable;
use super::meta::SystemFlags;
use super::{System, SystemError};
use super::{SystemId, SystemInput};
use crate::system::SystemHandle;
use crate::tick::Tick;
use crate::world::{DeferredWorld, World, WorldCell};

// -----------------------------------------------------------------------------
// IntoSystem

/// Trait for converting a value into a [`System`].
///
/// This trait enables ergonomic system construction from closures, functions,
/// and combinators. It serves as the entry point for creating systems that
/// can be scheduled and executed by the ECS.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// fn produce() -> u32 { 42 }
/// fn double(input: In<u32>) -> u32 { input.0 * 2 }
///
/// let mut world = World::alloc();
///
/// // `pipe` chains two systems, feeding the first's output to the second.
/// let result = world.invoke_once(produce.pipe(double), ()).unwrap();
/// assert_eq!(result, 84);
///
/// // `map` transforms a system's output with a closure.
/// let result = world.invoke_once(produce.map(|n| n + 1), ()).unwrap();
/// assert_eq!(result, 43);
///
/// // `with_input` fixes the input, leaving a system with `()` input.
/// let result = world.invoke_once(double.with_input(5), ()).unwrap();
/// assert_eq!(result, 10);
/// ```
///
/// # Combinators
///
/// `IntoSystem` provides several combinator methods for system composition:
///
/// - [`pipe`]: Chain two systems, feeding output of first as input to second
/// - [`map`]: Transform system output using a function
///
/// [`pipe`]: IntoSystem::pipe
/// [`map`]: IntoSystem::map
pub trait IntoSystem<I: SystemInput, O, M>: Sized + 'static {
    /// The concrete [`System`] this value builds into.
    type System: System<Input = I, Output = O>;

    /// Converts `this` into its runtime [`System`] representation.
    fn into_system(this: Self) -> Self::System;

    /// Returns the stable [`SystemId`] for systems built from this value.
    ///
    /// Users should not override this implementation.
    #[inline]
    fn system_id(&self) -> SystemId {
        SystemId::of::<Self>()
    }

    /// Returns the stable [`SystemHandle`] for systems built from this value.
    ///
    /// Users should not override this implementation.
    #[inline]
    fn system_handle(&self) -> SystemHandle<I, O> {
        SystemHandle::new(self.system_id())
    }

    /// Feeds a fixed input into the system, yielding a system with `()` input.
    ///
    /// Users should not override this implementation.
    #[inline]
    fn with_input(self, input: I::Data<'static>) -> IntoWithInputSystem<Self, I>
    where
        I::Data<'static>: Clone,
    {
        IntoWithInputSystem { s: self, i: input }
    }

    /// Merge two systems, if the former returns true, run the latter.
    ///
    /// Users should not override this implementation.
    #[inline]
    fn run_if<A, MA>(self, other: A) -> IntoRunIfSystem<A, Self>
    where
        O: 'static,
        A: IntoSystem<(), bool, MA>,
    {
        IntoRunIfSystem { a: other, b: self }
    }

    /// Chains two systems, feeding the output of `self` as the input to `other`.
    ///
    /// Users should not override this implementation.
    #[inline]
    fn pipe<B, BI, BO, MB>(self, other: B) -> IntoPipeSystem<Self, B>
    where
        O: 'static,
        B: IntoSystem<BI, BO, MB>,
        for<'a> BI: SystemInput<Data<'a> = O>,
    {
        IntoPipeSystem { a: self, b: other }
    }

    /// Transforms the system's output using `func`.
    ///
    /// Users should not override this implementation.
    #[inline]
    fn map<F, FO>(self, func: F) -> IntoMapSystem<Self, F>
    where
        F: FnMut(O) -> FO + Sync + Send + 'static,
    {
        IntoMapSystem { s: self, f: func }
    }
}

// -----------------------------------------------------------------------------
// System itself

impl<T: System> IntoSystem<T::Input, T::Output, ()> for T {
    type System = T;

    #[inline(always)]
    fn into_system(this: Self) -> Self {
        this
    }

    #[inline(always)]
    fn system_id(&self) -> SystemId {
        <T as System>::id(self)
    }
}

// -----------------------------------------------------------------------------
// WithInputSystem

/// Marker type distinguishing the `WithInputSystem` [`IntoSystem`] implementation.
pub struct WithInputSystemMarker;

/// Pending `with_input` combinator: a system plus a fixed input to feed it.
#[derive(Clone, Copy)]
pub struct IntoWithInputSystem<S, I: SystemInput> {
    s: S,
    i: I::Data<'static>,
}

/// A system that runs its inner system with a fixed, pre-supplied input.
pub struct WithInputSystem<S, I: SystemInput> {
    id: SystemId,
    s: S,
    i: I::Data<'static>,
}

#[rustfmt::skip]
impl<I, O, S, M> IntoSystem<(), O, (WithInputSystemMarker, (M, fn(I) -> O))>
    for IntoWithInputSystem<S, I>
where
    I: SystemInput + 'static,
    I::Data<'static>: Clone + Send + Sync,
    S: IntoSystem<I, O, M>,
    M: 'static,
{
    type System = WithInputSystem<S::System, I>;

    fn into_system(this: Self) -> Self::System {
        WithInputSystem {
            id: Self::system_id(&this),
            s: IntoSystem::into_system(this.s),
            i: this.i,
        }
    }

    fn system_id(&self) -> SystemId {
        struct WithInput<T>(PhantomData<T>);
        SystemId::of::<(WithInput<I>, S)>()
    }
}

impl<I, O, S> System for WithInputSystem<S, I>
where
    I: SystemInput + 'static,
    I::Data<'static>: Clone + Send + Sync,
    S: System<Input = I, Output = O>,
{
    type Input = ();
    type Output = O;

    fn id(&self) -> SystemId {
        self.id
    }

    fn flags(&self) -> SystemFlags {
        self.s.flags()
    }

    fn last_run(&self) -> Tick {
        self.s.last_run()
    }

    fn set_last_run(&mut self, last_run: Tick) {
        self.s.set_last_run(last_run);
    }

    fn clamp_ticks(&mut self, now: Tick) {
        self.s.clamp_ticks(now);
    }

    fn initialize(&mut self, world: &World) {
        self.s.initialize(world);
    }

    fn register_access(&self, table: &mut AccessTable, strict: bool) {
        self.s.register_access(table, strict);
    }

    unsafe fn run_raw(
        &mut self,
        _input: (),
        world: WorldCell<'_>,
    ) -> Result<Self::Output, SystemError> {
        unsafe { self.s.run_raw(self.i.clone(), world) }
    }

    fn queue_deferred(&mut self, world: DeferredWorld) {
        self.s.queue_deferred(world);
    }

    fn apply_deferred(&mut self, world: &mut World) {
        self.s.apply_deferred(world);
    }
}

// -----------------------------------------------------------------------------
// PipeSystem

/// Marker type distinguishing the `PipeSystem` [`IntoSystem`] implementation.
pub struct PipeSystemMarker;

/// Pending `pipe` combinator: two systems to chain output-to-input.
#[derive(Clone, Copy)]
pub struct IntoPipeSystem<A, B> {
    a: A,
    b: B,
}

/// A system that chains two systems, feeding the first's output into the second.
pub struct PipeSystem<A, B> {
    id: SystemId,
    a: A,
    b: B,
}

#[rustfmt::skip]
impl<AI, AO, BI, BO, A, B, MA, MB>
    IntoSystem<AI, BO, (PipeSystemMarker, (MA, MB, fn(AI) -> AO, fn(BI) -> BO), (A, B))>
    for IntoPipeSystem<A, B>
where
    AI: SystemInput,
    for<'a> BI: SystemInput<Data<'a> = AO>,
    A: IntoSystem<AI, AO, MA>,
    B: IntoSystem<BI, BO, MB>,
{
    type System = PipeSystem<A::System, B::System>;

    fn into_system(this: Self) -> Self::System {
        PipeSystem {
            id: Self::system_id(&this),
            a: IntoSystem::into_system(this.a),
            b: IntoSystem::into_system(this.b),
        }
    }

    fn system_id(&self) -> SystemId {
        struct Pipe;
        SystemId::of::<(A, Pipe, B)>()
    }
}

impl<AI, AO, BI, BO, A, B> System for PipeSystem<A, B>
where
    AI: SystemInput,
    for<'a> BI: SystemInput<Data<'a> = AO>,
    A: System<Input = AI, Output = AO>,
    B: System<Input = BI, Output = BO>,
{
    type Input = AI;
    type Output = BO;

    fn id(&self) -> SystemId {
        self.id
    }

    fn flags(&self) -> SystemFlags {
        self.a.flags().union(self.b.flags())
    }

    fn last_run(&self) -> Tick {
        self.a.last_run()
    }

    fn set_last_run(&mut self, last_run: Tick) {
        self.a.set_last_run(last_run);
        self.b.set_last_run(last_run);
    }

    fn clamp_ticks(&mut self, now: Tick) {
        self.a.clamp_ticks(now);
        self.b.clamp_ticks(now);
    }

    fn initialize(&mut self, world: &World) {
        self.a.initialize(world);
        self.b.initialize(world);
    }

    fn register_access(&self, table: &mut AccessTable, strict: bool) {
        let mut t = AccessTable::new();
        self.a.register_access(table, strict);
        self.b.register_access(&mut t, strict);
        table.merge(t);
    }

    unsafe fn run_raw(
        &mut self,
        input: <Self::Input as SystemInput>::Data<'_>,
        world: WorldCell<'_>,
    ) -> Result<Self::Output, SystemError> {
        let data = unsafe { self.a.run_raw(input, world)? };
        unsafe { self.b.run_raw(data, world) }
    }

    fn queue_deferred(&mut self, mut world: DeferredWorld) {
        self.a.queue_deferred(world.reborrow());
        self.b.queue_deferred(world.reborrow());
    }

    fn apply_deferred(&mut self, world: &mut World) {
        self.a.apply_deferred(world);
        self.b.apply_deferred(world);
    }
}

// -----------------------------------------------------------------------------
// MapSystem

/// Marker type distinguishing the `MapSystem` [`IntoSystem`] implementation.
pub struct MapSystemMarker;

/// Pending `map` combinator: a system plus a function to transform its output.
#[derive(Clone, Copy)]
pub struct IntoMapSystem<S, F> {
    s: S,
    f: F,
}

/// A system that transforms its inner system's output with a function.
pub struct MapSystem<S, F> {
    id: SystemId,
    s: S,
    f: F,
}

#[rustfmt::skip]
impl<I, O, FO, S, F, M>
    IntoSystem<I, FO, (MapSystemMarker, (M, fn(I) -> O, fn(O) -> FO), (S, F))>
    for IntoMapSystem<S, F>
where
    I: SystemInput,
    S: IntoSystem<I, O, M>,
    F: FnMut(O) -> FO + Sync + Send + 'static,
{
    type System = MapSystem<S::System, F>;

    fn into_system(this: Self) -> Self::System {
        MapSystem {
            id: Self::system_id(&this),
            s: IntoSystem::into_system(this.s),
            f: this.f,
        }
    }

    fn system_id(&self) -> SystemId {
        struct Map;
        SystemId::of::<(S, Map, F)>()
    }
}

impl<I, O, FO, S, F> System for MapSystem<S, F>
where
    I: SystemInput,
    S: System<Input = I, Output = O>,
    F: FnMut(O) -> FO + Sync + Send + 'static,
{
    type Input = I;
    type Output = FO;

    fn id(&self) -> SystemId {
        self.id
    }

    fn flags(&self) -> SystemFlags {
        self.s.flags()
    }

    fn last_run(&self) -> Tick {
        self.s.last_run()
    }

    fn set_last_run(&mut self, last_run: Tick) {
        self.s.set_last_run(last_run);
    }

    fn clamp_ticks(&mut self, now: Tick) {
        self.s.clamp_ticks(now);
    }

    fn initialize(&mut self, world: &World) {
        self.s.initialize(world)
    }

    fn register_access(&self, table: &mut AccessTable, strict: bool) {
        self.s.register_access(table, strict);
    }

    unsafe fn run_raw(
        &mut self,
        input: <Self::Input as SystemInput>::Data<'_>,
        world: WorldCell<'_>,
    ) -> Result<Self::Output, SystemError> {
        let data = unsafe { self.s.run_raw(input, world)? };
        Ok((self.f)(data))
    }

    fn queue_deferred(&mut self, world: DeferredWorld) {
        self.s.queue_deferred(world);
    }

    fn apply_deferred(&mut self, world: &mut World) {
        self.s.apply_deferred(world);
    }
}

// -----------------------------------------------------------------------------
// RunIfSystem

/// Marker type distinguishing the `PipeSystem` [`IntoSystem`] implementation.
pub struct RunIfSystemMarker;

/// Pending `pipe` combinator: two systems to chain output-to-input.
#[derive(Clone, Copy)]
pub struct IntoRunIfSystem<A, B> {
    a: A,
    b: B,
}

/// A system that chains two systems, feeding the first's output into the second.
pub struct RunIfSystem<A, B> {
    id: SystemId,
    a: A,
    b: B,
}

#[rustfmt::skip]
impl<BI, BO, A, B, MA, MB>
    IntoSystem<BI, BO, (RunIfSystemMarker, (MA, MB, fn() -> bool, fn(BI) -> BO), (A, B))>
    for IntoRunIfSystem<A, B>
where
    BI: SystemInput,
    A: IntoSystem<(), bool, MA>,
    B: IntoSystem<BI, BO, MB>,
{
    type System = RunIfSystem<A::System, B::System>;

    fn into_system(this: Self) -> Self::System {
        RunIfSystem {
            id: Self::system_id(&this),
            a: IntoSystem::into_system(this.a),
            b: IntoSystem::into_system(this.b),
        }
    }

    fn system_id(&self) -> SystemId {
        struct RunIf;
        SystemId::of::<(B, RunIf, A)>()
    }
}

impl<BI, BO, A, B> System for RunIfSystem<A, B>
where
    BI: SystemInput,
    A: System<Input = (), Output = bool>,
    B: System<Input = BI, Output = BO>,
{
    type Input = BI;
    type Output = BO;

    fn id(&self) -> SystemId {
        self.id
    }

    fn flags(&self) -> SystemFlags {
        self.a.flags().union(self.b.flags())
    }

    fn last_run(&self) -> Tick {
        self.a.last_run()
    }

    fn set_last_run(&mut self, last_run: Tick) {
        self.a.set_last_run(last_run);
        self.b.set_last_run(last_run);
    }

    fn clamp_ticks(&mut self, now: Tick) {
        self.a.clamp_ticks(now);
        self.b.clamp_ticks(now);
    }

    fn initialize(&mut self, world: &World) {
        self.a.initialize(world);
        self.b.initialize(world);
    }

    fn register_access(&self, table: &mut AccessTable, strict: bool) {
        let mut t = AccessTable::new();
        self.a.register_access(table, strict);
        self.b.register_access(&mut t, strict);
        table.merge(t);
    }

    unsafe fn run_raw(
        &mut self,
        input: <Self::Input as SystemInput>::Data<'_>,
        world: WorldCell<'_>,
    ) -> Result<Self::Output, SystemError> {
        let condition = unsafe { self.a.run_raw((), world)? };
        if condition {
            Err(SystemError::None)
        } else {
            unsafe { self.b.run_raw(input, world) }
        }
    }

    fn queue_deferred(&mut self, mut world: DeferredWorld) {
        self.a.queue_deferred(world.reborrow());
        self.b.queue_deferred(world.reborrow());
    }

    fn apply_deferred(&mut self, world: &mut World) {
        self.a.apply_deferred(world);
        self.b.apply_deferred(world);
    }
}

// -----------------------------------------------------------------------------
