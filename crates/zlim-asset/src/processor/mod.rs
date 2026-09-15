//! The asset import step: the importer, its interfaces, and the pieces it is built from.
//!
//! Processing turns a *source* asset into the bytes the runtime should load, and writes them —
//! plus the `.meta` that says how to load them — to the processed side of the source (see
//! [`AssetSource::processed_reader`], which
//! [`AssetServerMode::Processed`] reads).
//!
//! What lives here:
//!
//! - [`AssetProcessor`] reads a source asset through [`ProcessContext`] and writes the result;
//! - [`ErasedAssetProcessor`] is its type-erased mirror, for the code that drives processors
//!   without knowing their type;
//! - [`LoadTransformAndSave`] is a ready-made processor that composes a loader, a transformer and
//!   a saver;
//! - the registry the importer picks a processor from (by the type path a `.meta` names, by type
//!   name, or by the source file's extension);
//! - [`AssetProcessServer`]: the standalone importer. It owns a server configured for processed
//!   sources, walks the source side, runs each out-of-date asset through its processor (see
//!   [`AssetProcessServer::process_asset`]), and reports
//!   progress through [`ProcessorState`] / [`ProcessStatus`];
//! - the shared state behind [`AssetProcessServer`]: the sources it reads, the index and the coarse
//!   state a run goes through, the processors it can run, and the transaction log — it stays
//!   crate-internal, and the pieces an app needs from it (waiting for a run, replacing the transaction
//!   log factory) are reached through the importer;
//! - the index of the processed side the importer builds before every pass, which is what makes
//!   "is this still up to date" exact rather than conservative, and what carries a change down the
//!   chain of assets that depend on it;
//! - the transaction log the importer writes as it works, which is what tells an interrupted run
//!   apart from a finished one: it lives in [`crate::transaction`] (entries, traits, validation and
//!   the built-in file-backed logger).
//!
//! [`AssetSource::processed_reader`]: crate::source::AssetSource::processed_reader
//! [`AssetServerMode::Processed`]: crate::server::AssetServerMode
//! [`AssetProcessServer::process_asset`]: AssetProcessServer::process_asset

// - `server.rs`: the public surface — `AssetProcessServer` and the types an app observes;
// - `scan.rs`: the scan a run starts with, and the pruning of the processed side;
// - `pass.rs`: one pass — the queue of assets, the supervisor that drives it, and the startup job;
// - `step.rs`: one asset — what to do with it, and how it is written;
// - `watch.rs`: the listeners that follow the source side;
// - `state.rs`: the state every handle shares, and the processor registry;
// - `infos.rs`: the index of the processed side;
// - `transaction.rs`: the transaction log;
// - `processed.rs`: the processed files — what they say, and how one of them goes away;
// - `gated.rs`: the processed-side gate, which holds a read of the processed side until the run
//   that writes it is done;
// - `driver.rs`: running one processor over one path;
// - `context.rs` / `processor.rs` / `processors.rs` / `pipeline.rs`: the processing interfaces and
//   the registry.

mod context;
mod driver;
mod gated;
mod infos;
mod pass;
mod pipeline;
mod processed;
mod processor;
mod processors;
mod scan;
mod server;
mod state;
mod step;
mod transaction;
mod watch;

pub(crate) use gated::ProcessorGatedReader;
pub(crate) use pass::StartAssetProcessServer;
pub(crate) use processors::AssetProcessors;
pub(crate) use state::ProcessingState;

pub use context::ProcessContext;
pub use pipeline::{LoadTransformAndSave, LoadTransformAndSaveSettings};
pub use processor::{AssetProcessor, ErasedAssetProcessor};
pub use server::{AssetProcessServer, InitializeError, ProcessStatus};
pub use server::{ProcessorState, SetTransactionLoggerFailed};

#[cfg(test)]
mod tests;
