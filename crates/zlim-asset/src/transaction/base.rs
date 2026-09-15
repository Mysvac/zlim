//! The transaction log: what tells an interrupted import run apart from a finished one.

use zlim_core::derive::Error;
use zlim_utils::hash::HashSet;

use crate::utils::BoxedFuture;

// -----------------------------------------------------------------------------
// Keywords

/// The log keyword marking that a run started working on an asset.
pub const START: &str = "start";
/// The log keyword marking that a run finished working on an asset.
pub const FINISH: &str = "finish";
/// The log keyword marking that a run hit something it cannot recover from.
pub const UNRECOVERABLE: &str = "unrecoverable";

// -----------------------------------------------------------------------------
// LogEntry

/// One step of a run, as it is recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogEntry {
    /// The run started working on an asset.
    ProcessingStarted(String),
    /// The run finished working on an asset.
    ProcessingFinished(String),
    /// The run hit something it cannot recover from; everything is reprocessed next time.
    UnrecoverableError,
}

/// One asset's transactions do not add up.
#[derive(Error, Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum LogEntryError {
    /// The same asset was started twice without finishing in between.
    #[error("Encountered a duplicate process asset transaction: {_0}")]
    DuplicateTransaction(String),

    /// An asset finished without ever having started.
    #[error("A asset finished processing but never started: {_0}")]
    UnstartedTransaction(String),

    /// An asset started but never finished: the run was interrupted.
    #[error("An asset started processing but never finished: {_0}")]
    UnfinishedTransaction(String),
}

/// Something went wrong while reading or writing a transaction log.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum TransactionError {
    /// A line of the log is not something this format can produce.
    #[error("Encountered an invalid log line: '{_0}'")]
    InvalidLine(Box<str>),

    /// The log file could not be read or written.
    #[error("Encountered a transaction io error: '{_0}'")]
    Io(std::io::Error),

    /// The log implementation failed for a reason of its own.
    #[error(transparent)]
    Other(Box<dyn ::core::error::Error + Send + 'static>),
}

impl From<std::io::Error> for TransactionError {
    #[cold]
    #[inline]
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

// -----------------------------------------------------------------------------
// TransactionLog

/// Writes the log of the run that is happening.
///
/// The log travels with the importer, which is an app resource, so implementations have to be
/// `Send + Sync + 'static` just like the [`TransactionLogger`] that hands them out.
pub trait TransactionLog: Send + Sync + 'static {
    /// Records that the run started working on `asset`.
    fn start<'a>(&'a mut self, asset: &'a str) -> BoxedFuture<'a, Result<(), TransactionError>>;

    /// Records that the run finished working on `asset`.
    fn finish<'a>(&'a mut self, asset: &'a str) -> BoxedFuture<'a, Result<(), TransactionError>>;

    /// Records that the run cannot recover; the next one reprocesses everything.
    fn unrecoverable(&mut self) -> BoxedFuture<'_, Result<(), TransactionError>>;
}

// -----------------------------------------------------------------------------
// TransactionLogger

/// Opens the log of a run, and reads back the one the previous run left behind.
pub trait TransactionLogger: Send + Sync + 'static {
    /// Reads every entry of the previous run's log.
    fn read(&self) -> BoxedFuture<'_, Result<Vec<LogEntry>, TransactionError>>;

    /// Starts a fresh log for the run that is about to happen.
    fn new_log(&self) -> BoxedFuture<'_, Result<Box<dyn TransactionLog>, TransactionError>>;
}

// -----------------------------------------------------------------------------

/// Why the previous run's log cannot be trusted.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ValidateLogError {
    /// The log was read, but its transactions do not add up.
    #[error("Encountered broken process asset transactions: {_0:?}")]
    LogEntryErrors(Vec<LogEntryError>),
    /// The log itself could not be read.
    #[error("Failed to read the transaction log: {_0}")]
    TransactionError(TransactionError),
    /// The run that wrote the log reported that it was unrecoverable.
    #[error("Encountered an unrecoverable error. All assets will be reprocessed.")]
    UnrecoverableError,
}

impl From<TransactionError> for ValidateLogError {
    #[cold]
    #[inline]
    fn from(value: TransactionError) -> Self {
        Self::TransactionError(value)
    }
}

impl From<std::io::Error> for ValidateLogError {
    #[cold]
    #[inline]
    fn from(value: std::io::Error) -> Self {
        Self::TransactionError(TransactionError::Io(value))
    }
}

// -----------------------------------------------------------------------------
// validate

/// Reads `logger`'s previous log and reports whether it describes a complete run.
///
/// Every `start` has to be paired with a `finish`: an unpaired `start` is exactly what an
/// interrupted run leaves behind, and the importer has to reprocess everything in that case.
pub async fn validate_transaction(logger: &dyn TransactionLogger) -> Result<(), ValidateLogError> {
    let mut started: HashSet<String> = HashSet::new();
    let mut errors: Vec<LogEntryError> = Vec::new();

    for entry in logger.read().await? {
        match entry {
            LogEntry::ProcessingStarted(path) => {
                if let Some(dup) = started.replace(path) {
                    errors.push(LogEntryError::DuplicateTransaction(dup));
                }
            }
            LogEntry::ProcessingFinished(path) => {
                if !started.remove(&path) {
                    errors.push(LogEntryError::UnstartedTransaction(path));
                }
            }
            LogEntry::UnrecoverableError => return Err(ValidateLogError::UnrecoverableError),
        }
    }

    for path in started {
        errors.push(LogEntryError::UnfinishedTransaction(path));
    }

    if !errors.is_empty() {
        return Err(ValidateLogError::LogEntryErrors(errors));
    }

    Ok(())
}

// -----------------------------------------------------------------------------
