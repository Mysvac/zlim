//! ECS system abstraction: the [`System`] trait, its [`SystemParam`] extractors,
//! system construction via [`IntoSystem`], and the scheduler access model.
//!
//! # Systems
//!
//! A *system* is a unit of logic that runs against a [`World`] (or a
//! [`WorldCell`] view of it).  The [`System`] trait is the type-erased
//! contract every runnable system implements.  Instead of implementing
//! [`System`] by hand, users build systems from plain functions and closures
//! through [`IntoSystem`], which wraps them in a function system.
//!
//! ```ignore
//! fn system(query: Query<Name>, logger: Res<Logger>) {
//!     for name in query { logger.log(&name); }
//! }
//! ```
//!
//! # Building and running systems
//!
//! Functions and closures are converted into systems with [`IntoSystem`]
//! (`into_system`, `pipe`, `map`, `with_input`).
//!
//! Every system is identified by a stable [`SystemId`] and can be cached
//! in a [`World`] behind a type-safe [`SystemHandle`].
//!
//! ```rust
//! use zlim_core::prelude::*;
//!
//! fn hello_world() {
//!     std::println!("Hello World!");
//! }
//!
//! let mut world = World::alloc();
//! world.invoke(hello_world, ()).unwrap();
//! ```
//!
//! # System Inputs
//!
//! System supports input parameters through: [`In`] / [`InMut`] / [`InRef`].
//!
//! ```rust
//! use zlim_core::prelude::*;
//!
//! fn print_num(input: In<u32>) {
//!     std::println!("{}", input.0);
//! }
//!
//! fn print_ref(input: InMut<u32>) {
//!     std::println!("{}", input.0);
//! }
//!
//! fn add_with(input: (InMut<u32>, In<u32>)) {
//!     let (InMut(x), In(y)) = input;
//!     *x += y;
//! }
//!
//! let mut world = World::alloc();
//!
//! world.invoke_once(print_num, 1_u32).unwrap();
//!
//! let mut one = 1_u32;
//! world.invoke_once(print_ref, &mut one).unwrap();
//!
//! let mut x: u32 = 0;
//! world.invoke_once(add_with, (&mut x, 123)).unwrap();
//!
//! assert_eq!(x, 123);
//! ```
//!
//! The input must be the first parameter of the system function.
//!
//! # System parameters
//!
//! System functions declare the data they need as *system parameters* — types
//! implementing [`SystemParam`].  The crate provides:
//!
//! - world: `&World`, `&mut World`, `&NonSendWorld`, `&mut NonSendWorld`, `DeferredWorld`.
//!   - [`World`] : The reference of the world itself.
//!   - [`NonSendWorld`] : The reference of the world itself, but run on main thread.
//!   - [`DeferredWorld`] : The mutable reference of the world itself, but cannot change world structure.
//!
//! - resource access: [`Res`] / [`ResMut`] / [`NonSend`] / [`NonSendMut`] (defined in `crate::borrow`),
//!   - [`Res`] : readonly resource reference with change detections
//!   - [`ResMut`] : mutable resource reference with change detections
//!   - [`NonSend`] : readonly resource reference with change detections, but run on main thread
//!   - [`NonSendMut`] : mutable resource reference with change detections, but run on main thread
//!
//! - entity queries: [`Query`] / [`Single`] (defined in `crate::query`),
//!
//! - deferred commands: [`Commands`] (defined in `crate::command`),
//!
//! - scheduling hints: [`ExclusiveMarker`] / [`NonSendMarker`].
//!
//! - system-local state: [`Local`], local data stored in the system.
//!
//! - system param wrapper: [`Option`] / [`If`].
//!   - [`If`] : Skip if system params build failed, return a `ignore` leveled [`SystemParamError`].
//!   - [`Option`] : `None` if system params build failed, but still run this system.
//!
//! - meanless placeholder: [`PhantomData`].
//!
//! - hierarchy information: [`RootEntities`] and [`HierarchyQuery`].
//!   Efficient hierarchy queries without affecting system parallelism. Not compatible
//!   with exclusive parameters — holds an immutable reference to hierarchy internals.
//!
//! Tuples of parameters (up to 12 elements) also implement [`SystemParam`],
//! and custom parameters can be composed from existing ones with `#[derive(SystemParam)]`,
//! see [derive documents](crate::derive::SystemParam) for details.
//!
//! ```ignore
//!
//! #[derive(SystemParam)]
//! struct MyParam<'w, 's> {
//!     // 'w: 'world, 's: 'system|'state
//!     query: Query<'w, 's, Name, With<Health>>,
//!     logger: If<Res<Logger>>,
//! }
//! ```
//!
//! [`World`]: crate::world::World
//! [`WorldCell`]: crate::world::WorldCell
//! [`NonSendWorld`]: crate::world::NonSendWorld
//! [`DeferredWorld`]: crate::world::DeferredWorld
//! [`Res`]: crate::borrow::Res
//! [`ResMut`]: crate::borrow::ResMut
//! [`NonSend`]: crate::borrow::NonSend
//! [`NonSendMut`]: crate::borrow::NonSendMut
//! [`Query`]: crate::query::Query
//! [`Single`]: crate::query::Single
//! [`Commands`]: crate::command::Commands
//! [`PhantomData`]: core::marker::PhantomData
//! [`RootEntities`]: crate::entity::RootEntities

mod access;
mod error;
mod function;
mod input;
mod into_system;
mod meta;
mod param;
mod params;
mod registry;
mod system;

pub use access::{AccessTable, ComponentAccess};
pub use access::{FilterParam, FilterParamBuilder};
pub use error::{SystemError, SystemParamError};
pub use input::SystemInput;
pub use input::{In, InMut, InRef};
pub use into_system::IntoSystem;
pub use meta::{SystemFlags, SystemId, SystemMeta};
pub use param::SystemParam;
pub use params::*;
pub use registry::SystemHandle;
pub use system::System;

pub use zlim_core_derive::SystemParam;

pub(crate) use registry::SystemCache;
