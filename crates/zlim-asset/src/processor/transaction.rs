//! The transaction log: the state a run opens it from, and the entries a run writes to it.
//!
//! Every asset a run writes is bracketed by a `start`/`finish` pair. That bracket is what tells an
//! interrupted run apart from a finished one: the next run validates the log, and a log that does not
//! validate makes it distrust the processed side and process everything again.
//!
//! Two pieces of state belong to the log, and both are fields of the importer's shared data: the
//! *factory* the log is opened from ([`LogFactoryState`], which also remembers that a run has claimed
//! it), and the log itself ([`LogLock`], shared by every task of a run and locked for the few
//! instructions a `start`/`finish` takes — never across the processing itself).

use core::sync::atomic::Ordering;
use std::sync::{Arc, PoisonError};

use crate::path::AssetPath;
use crate::processor::server::AssetProcessServer;
use crate::transaction::{
    TransactionError, TransactionLog, TransactionLogger, validate_transaction,
};

// -----------------------------------------------------------------------------
// The state of the log

/// The importer's transaction log, shared by every task of a pass: locked for the few instructions
/// a `start`/`finish` takes, never held across the processing itself.
pub(super) type LogLock = Arc<async_lock::Mutex<Option<Box<dyn TransactionLog>>>>;

/// The transaction log factory, and whether a run has claimed it.
///
/// The split exists because a run takes the factory out while it logs: `Claimed` is what keeps it
/// for the *next* run, and it is also what tells
/// [`AssetProcessServer::set_transaction_logger`] that processing has started, so replacing the
/// factory would no longer take effect.
pub(crate) enum LogFactoryState {
    /// No run has started: a caller may still replace the factory (there may be none at all).
    Pending(Option<Box<dyn TransactionLogger>>),

    /// A run has started: the factory (if there is one) is fixed, and kept for the runs after it.
    Claimed(Option<Box<dyn TransactionLogger>>),
}

// -----------------------------------------------------------------------------
// AssetProcessServer: the log of a run

impl AssetProcessServer {
    /// Opens the transaction log of a run, and reports whether the processed side can be trusted.
    ///
    /// The factory is taken out while the log is opened, and put back as [`LogFactoryState::Claimed`]
    /// before any asset is processed: a second run therefore keeps its log (and a test can watch what
    /// a run wrote). Without a factory there is no log at all, and the run is treated as recoverable —
    /// nothing says otherwise.
    ///
    /// Claiming the factory is also what tells [`AssetProcessServer::set_transaction_logger`] that
    /// processing has started: from here on, replacing it would no longer take effect.
    pub(super) async fn open_transaction_log(&self) -> bool {
        let factory = {
            let mut guard = self
                .data
                .logger
                .lock()
                .unwrap_or_else(PoisonError::into_inner);

            let factory = match &mut *guard {
                LogFactoryState::Pending(factory) | LogFactoryState::Claimed(factory) => {
                    factory.take()
                }
            };

            *guard = LogFactoryState::Claimed(None);

            factory
        };

        let Some(factory) = factory else {
            return true;
        };

        let recoverable = validate_transaction(&*factory).await.is_ok();

        let log = match factory.new_log().await {
            Ok(log) => Some(log),
            Err(error) => {
                ::core::hint::cold_path();
                zlim_log::error!("Failed to start the transaction log: {error}");
                None
            }
        };

        if !recoverable {
            ::core::hint::cold_path();
            zlim_log::warn!(
                "The transaction log of the previous import run is not usable; \
                 every asset will be processed again."
            );
        }

        {
            let mut guard = self.data.log.lock().await;
            *guard = log;
        }

        self.data
            .skip_up_to_date
            .store(recoverable, Ordering::SeqCst);

        // Back for the next run: a repeated `run` writes a log of its own instead of losing it.
        {
            let mut guard = self
                .data
                .logger
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            *guard = LogFactoryState::Claimed(Some(factory));
        }

        recoverable
    }

    /// Writes the `start` half of the transaction of `path`, when a log is configured.
    pub(super) async fn log_start(&self, path: &AssetPath<'static>) {
        let mut guard = self.data.log.lock().await;

        if let Some(log) = guard.as_deref_mut() {
            log_start(log, path).await;
        }
    }

    /// Writes the `finish` half of the transaction of `path`, when a log is configured.
    pub(super) async fn log_finish(&self, path: &AssetPath<'static>) {
        let mut guard = self.data.log.lock().await;

        if let Some(log) = guard.as_deref_mut() {
            log_finish(log, path).await;
        }
    }

    /// Marks the run unrecoverable: the next one has to process everything again.
    pub(super) async fn log_unrecoverable(&self) {
        let mut guard = self.data.log.lock().await;

        if let Some(log) = guard.as_deref_mut() {
            log_unrecoverable(log).await;
        }
    }
}

// -----------------------------------------------------------------------------
// Writing one entry

/// Writes the `start` half of `path`'s transaction.
///
/// A failure is logged and otherwise ignored: the entry is a hint to the next run, and the asset it
/// belongs to is still reported on its own.
async fn log_start(log: &mut dyn TransactionLog, path: &AssetPath<'static>) {
    if let Err(error) = log.start(&path.to_string()).await {
        log_transaction_error("start", path, error);
    }
}

/// Writes the `finish` half of `path`'s transaction.
///
/// A failure is logged and otherwise ignored: the asset itself is still reported on its own.
async fn log_finish(log: &mut dyn TransactionLog, path: &AssetPath<'static>) {
    if let Err(error) = log.finish(&path.to_string()).await {
        log_transaction_error("finish", path, error);
    }
}

/// Marks the log unrecoverable, so the next run processes everything again.
///
/// A failure is logged: the run itself is already in trouble, and there is nothing better to do with
/// it here.
async fn log_unrecoverable(log: &mut dyn TransactionLog) {
    if let Err(error) = log.unrecoverable().await {
        zlim_log::error!("Failed to write to the transaction log: {error}");
    }
}

/// Reports that the `entry` half of `path`'s transaction could not be written.
fn log_transaction_error(entry: &str, path: &AssetPath<'static>, error: TransactionError) {
    ::core::hint::cold_path();
    zlim_log::error!("Failed to write the `{entry}` transaction for '{path}': {error}");
}
