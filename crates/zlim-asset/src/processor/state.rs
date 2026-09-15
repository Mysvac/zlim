//! The state every handle of the importer shares: what a run is doing, and what it knows about every
//! asset.
//!
//! [`ProcessingState`] is the one piece of the importer that outlives a single task. It holds the
//! coarse state a run goes through, the channels whoever waits on a run listens to, and the index of
//! the processed side — with the lock that keeps a reader of an asset's files out while they are
//! written.
//!
//! It is `pub(crate)` because two things outside the importer need it: the processed-side gate
//! (`gated.rs`), which makes a read wait for the run that writes what is being read, and the source
//! builders, which hand it to that gate.
//!
//! The last section is about where the *processors* live: on the importer's shared data, not on the
//! `AssetServer`, because loading never needs one.

use core::sync::atomic::AtomicU8;
use core::sync::atomic::Ordering::{AcqRel, Acquire};
use std::sync::PoisonError;

use async_broadcast::{Receiver, Sender};
use zlim_utils::ext::CachePadded;

use crate::error::AssetReaderError;
use crate::path::AssetPath;
use crate::processor::AssetProcessors;
use crate::processor::infos::ProcessorAssetInfos;
use crate::processor::server::{AssetProcessServer, ProcessStatus, ProcessorState};

// -----------------------------------------------------------------------------
// The state a run goes through

const INITIALIZING: u8 = ProcessorState::Initializing as u8;
const PROCESSING: u8 = ProcessorState::Processing as u8;
const FINISH: u8 = ProcessorState::Finished as u8;

/// The state a run goes through, plus what the importer knows about every asset.
///
/// The processed-side gate (gated.rs) waits on this too, which is why it is visible to the rest
/// of the crate.
pub(crate) struct ProcessingState {
    /// The coarse state of the processor.
    ///
    /// An atomic, unlike the asset map below: it is read by [`AssetProcessServer::state`], which is
    /// not `async`, and a load has no guard that could be held across an `await`.
    state: CachePadded<AtomicU8>,

    /// The per-asset state: what the processed side says, who depends on it, how it came out, the
    /// transaction lock over its files, and the channel its status is broadcast on.
    asset_infos: CachePadded<async_lock::RwLock<ProcessorAssetInfos>>,

    /// Announced when the processor leaves [`ProcessorState::Initializing`] and when it reaches
    /// [`ProcessorState::Finished`]. One slot each, with overflow: whoever waits is interested in
    /// the state, not in every transition.
    started_sender: Sender<()>,
    started_receiver: Receiver<()>,
    finished_sender: Sender<()>,
    finished_receiver: Receiver<()>,
}

impl ProcessingState {
    /// Creates the state a run starts from: [`ProcessorState::Initializing`], with no asset known yet
    /// and the channels a waiter listens on.
    pub(crate) fn new() -> Self {
        let (mut started_sender, started_receiver) = async_broadcast::broadcast(1);
        let (mut finished_sender, finished_receiver) = async_broadcast::broadcast(1);
        started_sender.set_overflow(true);
        finished_sender.set_overflow(true);

        Self {
            state: CachePadded::new(AtomicU8::new(INITIALIZING)),
            started_sender,
            started_receiver,
            finished_sender,
            finished_receiver,
            asset_infos: CachePadded::new(async_lock::RwLock::new(ProcessorAssetInfos::default())),
        }
    }

    /// Moves to `state`, announcing it to whoever waits.
    pub(super) async fn set_state(&self, state: ProcessorState) {
        let previous = match self.state.swap(state as u8, AcqRel) {
            INITIALIZING => ProcessorState::Initializing,
            PROCESSING => ProcessorState::Processing,
            FINISH => ProcessorState::Finished,
            _ => unreachable!("processor state is polluted"),
        };

        if previous == state {
            return;
        }

        if state == ProcessorState::Processing {
            let _ = self.started_sender.broadcast(()).await;
        }

        if state == ProcessorState::Finished {
            let _ = self.finished_sender.broadcast(()).await;
        }
    }

    /// The coarse state of the processor, loaded without waiting.
    pub(crate) fn state(&self) -> ProcessorState {
        match self.state.load(Acquire) {
            INITIALIZING => ProcessorState::Initializing,
            PROCESSING => ProcessorState::Processing,
            FINISH => ProcessorState::Finished,
            _ => unreachable!("processor state is polluted"),
        }
    }

    /// The per-asset state, for reading.
    pub(crate) async fn read_infos(&self) -> async_lock::RwLockReadGuard<'_, ProcessorAssetInfos> {
        self.asset_infos.read().await
    }

    /// The per-asset state, for writing.
    pub(crate) async fn write_infos(
        &self,
    ) -> async_lock::RwLockWriteGuard<'_, ProcessorAssetInfos> {
        self.asset_infos.write().await
    }

    /// Returns the lock that keeps this asset's processed files from being read while they are
    /// written, held for reading.
    ///
    /// The lock is cloned out before it is taken: taking it while the map is locked would let one
    /// asset's lock block every other asset's.
    ///
    /// # Errors
    ///
    /// An asset the index does not know is [`AssetReaderError::NotFound`]: there is nothing to read.
    /// That is the answer the processed-side gate relies on for an asset that was forgotten.
    pub(crate) async fn transaction_lock(
        &self,
        path: &AssetPath<'static>,
    ) -> Result<async_lock::RwLockReadGuardArc<()>, AssetReaderError> {
        let lock = {
            let infos = self.read_infos().await;
            let info = infos.get(path).ok_or_else(|| {
                ::core::hint::cold_path();
                AssetReaderError::NotFound(path.path().to_owned())
            })?;

            info.file_transaction_lock.clone()
        };

        Ok(lock.read_arc().await)
    }

    /// Returns the lock over this asset's processed files, held for writing.
    ///
    /// The importer takes this before it rewrites the files, so a reader that got past the gate
    /// reads one whole revision rather than a half-written one.
    pub(crate) async fn transaction_lock_for_write(
        &self,
        path: &AssetPath<'static>,
    ) -> async_lock::RwLockWriteGuardArc<()> {
        let lock = {
            let mut infos = self.write_infos().await;
            infos.get_or_insert(path).file_transaction_lock.clone()
        };

        lock.write_arc().await
    }

    /// Waits until every run has finished.
    pub(crate) async fn wait_until_finished(&self) {
        let receiver = match self.state() {
            ProcessorState::Finished => None,
            // The clone inherits the cursor of the receiver this state holds, and the channels keep
            // their last message: a `Finished` announced in between is therefore delivered at once
            // instead of being missed.
            _ => Some(self.finished_receiver.clone()),
        };

        if let Some(mut receiver) = receiver {
            let _ = receiver.recv().await;
        }
    }

    /// Waits until the processor has been initialized, returning immediately when it already has.
    pub(super) async fn wait_until_initialized(&self) {
        let receiver = match self.state() {
            ProcessorState::Initializing => Some(self.started_receiver.clone()),
            _ => None,
        };

        if let Some(mut receiver) = receiver {
            let _ = receiver.recv().await;
        }
    }

    /// Waits for `path`'s result.
    ///
    /// A path the run never registered is [`ProcessStatus::NonExistent`] — the run knows every
    /// source file before it announces that it started, so this means "not an asset" rather than
    /// "not yet". The status channel of an asset that has no result yet is cloned while the map is
    /// locked, so a status recorded in between is not missed.
    pub(crate) async fn wait_until_processed(&self, path: AssetPath<'static>) -> ProcessStatus {
        self.wait_until_initialized().await;

        let mut receiver = {
            let infos = self.read_infos().await;

            let Some(info) = infos.get(&path) else {
                return ProcessStatus::NonExistent;
            };

            match info.status {
                Some(status) => return status,
                None => info.status_receiver.clone(),
            }
        };

        receiver.recv().await.unwrap_or(ProcessStatus::NonExistent)
    }
}

// -----------------------------------------------------------------------------
// The processor registry, as the importer reaches it

impl AssetProcessServer {
    /// The processors the importer can run, for reading.
    #[inline]
    pub(super) fn read_processors(&self) -> std::sync::RwLockReadGuard<'_, AssetProcessors> {
        self.data
            .processors
            .read()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The processors the importer can run, for writing.
    #[inline]
    pub(super) fn write_processors(&self) -> std::sync::RwLockWriteGuard<'_, AssetProcessors> {
        self.data
            .processors
            .write()
            .unwrap_or_else(PoisonError::into_inner)
    }
}
