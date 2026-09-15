//! The file-backed transaction log: a [`FileTransactionLogger`] and the log it writes.
//!
//! The log is a plain text file — by default `imported_assets/default/log` under the asset root —
//! with one entry per line: `start <path>`, `finish <path>`, or `unrecoverable` (see [`LogEntry`]
//! for what they mean). Every line is flushed as it is written: a log that only lives in a buffer
//! would not survive the interruption it is there to record.

use std::path::{Path, PathBuf};

use crate::utils::BoxedFuture;

use super::*;

// -----------------------------------------------------------------------------
// FileTransactionLogger

/// A [`TransactionLogger`] that keeps the log in a file under the asset root.
///
/// The file is `log` inside the base directory given to [`new`](Self::new) —
/// the plugin hands it the processed-side path, so the log lands next to the
/// imported assets. Set [`file_path`](Self::file_path) to place it somewhere else.
///
/// # Example
///
/// ```ignore
/// use zlim_asset::transaction::file::FileTransactionLogger;
///
/// // `base` is the directory the log file (`base/log`) is written into.
/// let logger = FileTransactionLogger::new("imported_assets");
/// ```
#[derive(Debug, Clone)]
pub struct FileTransactionLogger {
    /// The file the log is written to.
    pub file_path: PathBuf,
}

impl FileTransactionLogger {
    /// Creates a logger that writes the log to `base/log`.
    pub fn new(base: impl AsRef<Path>) -> Self {
        Self {
            file_path: base.as_ref().join("log"),
        }
    }
}

// -----------------------------------------------------------------------------
// FileTransactionLog

struct FileTransactionLog {
    file: async_fs::File,
}

impl FileTransactionLog {
    #[inline]
    fn validate_path(prefix: &str, path: &str) -> Result<(), TransactionError> {
        if path.trim_ascii_start().is_empty() {
            // `trim_ascii_start` is faster than `trim_ascii`.
            let msg = format!("find a empty asset name begin with `{prefix}`");
            return Err(TransactionError::InvalidLine(msg.into()));
        }

        Ok(())
    }

    async fn write(&mut self, line: &str) -> Result<(), TransactionError> {
        use futures_lite::AsyncWriteExt;

        self.file.write_all(line.as_bytes()).await?;
        self.file.flush().await?;

        Ok(())
    }
}

impl TransactionLog for FileTransactionLog {
    fn start<'a>(&'a mut self, asset: &'a str) -> BoxedFuture<'a, Result<(), TransactionError>> {
        Box::pin(async move {
            Self::validate_path(START, asset)?;
            self.write(&format!("{START} {asset}\n")).await
        })
    }

    fn finish<'a>(&'a mut self, asset: &'a str) -> BoxedFuture<'a, Result<(), TransactionError>> {
        Box::pin(async move {
            Self::validate_path(FINISH, asset)?;
            self.write(&format!("{FINISH} {asset}\n")).await
        })
    }

    fn unrecoverable(&mut self) -> BoxedFuture<'_, Result<(), TransactionError>> {
        Box::pin(async move { self.write(&format!("{UNRECOVERABLE}\n")).await })
    }
}

// -----------------------------------------------------------------------------
// TransactionLogger

impl TransactionLogger for FileTransactionLogger {
    fn read(&self) -> BoxedFuture<'_, Result<Vec<LogEntry>, TransactionError>> {
        let file_path: &Path = &self.file_path;

        Box::pin(async move {
            use futures_lite::AsyncReadExt;

            let mut file = match async_fs::File::open(file_path).await {
                Ok(file) => file,
                // No log file means no previous run recorded anything, which is the same as an
                // empty log — not an error.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
                Err(error) => return Err(error.into()),
            };

            let mut text = String::new();
            file.read_to_string(&mut text).await?;

            let hint: usize = text.len() >> 6; // str_len / 64
            let mut entries: Vec<LogEntry> = Vec::with_capacity(hint);

            for line in text.lines() {
                if let Some(path) = line.strip_prefix(START) {
                    let path = path.trim_ascii();
                    entries.push(LogEntry::ProcessingStarted(path.into()));
                } else if let Some(path) = line.strip_prefix(FINISH) {
                    let path = path.trim_ascii();
                    entries.push(LogEntry::ProcessingFinished(path.into()));
                } else if line.starts_with(UNRECOVERABLE) {
                    entries.push(LogEntry::UnrecoverableError);
                } else if line.trim_ascii_start().is_empty() {
                    continue;
                } else {
                    return Err(TransactionError::InvalidLine(line.into()));
                }
            }

            Ok(entries)
        })
    }

    fn new_log(&self) -> BoxedFuture<'_, Result<Box<dyn TransactionLog>, TransactionError>> {
        let path: &Path = &self.file_path;

        Box::pin(async move {
            if let Err(error) = async_fs::remove_file(&path).await
                && error.kind() != std::io::ErrorKind::NotFound
            {
                zlim_log::error!("Failed to remove the previous transaction log: {error}");
            }

            if let Some(parent) = path.parent() {
                async_fs::create_dir_all(parent).await?;
            }

            let file = async_fs::File::create(path).await?;

            Ok(Box::new(FileTransactionLog { file }) as Box<dyn TransactionLog>)
        })
    }
}

// -----------------------------------------------------------------------------
