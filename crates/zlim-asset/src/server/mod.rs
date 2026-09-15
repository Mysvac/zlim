//! The asset server: the handle-issuing, load- and save-starting front of the asset system.
//!
//! [`AssetServer`] is the entry point. It hands out [`Handle`]s, starts loads and queues saves;
//! the load or save being started is described by [`LoadBuilder`] / [`SaveBuilder`], which
//! [`AssetServer::load_builder`] / [`AssetServer::save_builder`] return and a terminal method
//! consumes.
//!
//! Three modes shape what the server does:
//!
//! - [`AssetServerMode`]: read the raw source side or the processed side;
//! - [`AssetMetaCheckMode`]: when a `.meta` sidecar is read;
//! - [`UnapprovedPathMode`]: what happens to a path that escapes its source root.
//!
//! Progress is observed through the load states ([`LoadState`], [`DependencyLoadState`],
//! [`RecursiveDependencyLoadState`]) or by awaiting the server's wait functions. The jobs
//! exported here ([`AssetServerDiagnosticJob`], [`ClearFinishedAssetTask`],
//! [`HandleAssetSaveCommands`], [`HandleAssetSeverEvents`]) are what keep a running app in sync
//! with the server.
//!
//! The module also defines [`UNTYPED_SOURCE_SUFFIX`], the synthetic source a type-erased load
//! registers its wrapper under.
//!
//! [`Handle`]: crate::handle::Handle

// -----------------------------------------------------------------------------
// Modules

// - `server.rs`: `AssetServer` itself, and the jobs and commands that drive it;
// - `internal.rs`: `AssetServerData`, the state the server's `Arc` shares — crate-internal;
// - `info.rs`: the crate-internal bookkeeping of handles, paths and dependency states;
// - `builder.rs`: `LoadBuilder` and `SaveBuilder`;
// - `state.rs`: the three load states;
// - `config.rs`: the three modes above;
// - `event.rs`: the events a load task posts back to the server — crate-internal.

mod builder;
mod config;
mod event;
mod info;
mod internal;
mod server;
mod state;

// -----------------------------------------------------------------------------
// Constants

/// Suffix appended to the source name of an asset that is loaded without knowing
/// its type.
///
/// The type-erased load stores its [`LoadedUntypedAsset`]
/// wrapper under a path with this synthetic source, so that the wrapper and the
/// concrete asset it points to can both live in the server at the same time.
///
/// [`LoadedUntypedAsset`]: crate::loaded::LoadedUntypedAsset
pub const UNTYPED_SOURCE_SUFFIX: &str = "--untyped";

// -----------------------------------------------------------------------------
// Exports

pub use builder::{LoadBuilder, SaveBuilder};
pub(crate) use event::AssetServerEvent;
pub(crate) use info::HandleLoadingMode;
pub(crate) use internal::AssetServerData;

pub use config::AssetMetaCheckMode;
pub use config::AssetServerMode;
pub use config::UnapprovedPathMode;
pub use server::AssetServer;
pub use server::AssetServerDiagnosticJob;
pub use state::*;

// Job
pub use server::{ClearFinishedAssetTask, HandleAssetSaveCommands, HandleAssetSeverEvents};
