//! Running a pass: the queue of assets, the supervisor that keeps one task per asset in flight, and the
//! service that keeps doing that as the sources change.
//!
//! The supervisor is what turns "process these paths" into concurrency with a defined end: one task per
//! asset on the IO task pool, so a task that waits — a processor reading a process dependency waits on
//! the processed-side gate — parks instead of blocking the others, and the pass is over exactly when
//! the last task is. That is what makes a tree that has never been processed go through in a single
//! pass, in dependency order, with no ordering and no retries of its own.
//!
//! [`SupervisorMode`] says which of the two shapes a supervisor is running in: a single pass, or the
//! importer's service.

use core::future::poll_fn;
use core::sync::atomic::Ordering;
use core::task::Poll;

use zlim_core::borrow::Res;
use zlim_core::derive::job_fn;
use zlim_task::IoTaskPool;
use zlim_utils::mpmc;

use crate::path::AssetPath;
use crate::processor::infos::{Task, TaskReceiver, TaskSender, queued_task};
use crate::processor::server::{AssetProcessServer, ProcessorState};

// -----------------------------------------------------------------------------
// The supervisor's vocabulary

/// How the supervisor treats its task channel.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SupervisorMode {
    /// One pass: the supervisor returns once nothing is queued and nothing is running.
    OneOff,
    /// The importer's service: the supervisor keeps running, because source changes keep queueing
    /// new work.
    LongRunning,
}

/// One thing the supervisor observes: a task to run, or a task that finished.
///
/// Whichever side reports a receive error ends the supervisor: the error means every sender is gone,
/// so no work can arrive or report in any more, and waiting on would never end.
enum TaskEvent {
    /// A task taken off the queue, or the error saying no task can be taken any more.
    Queued(Result<Task, mpmc::RecvError>),
    /// A spawned task reporting in, or the error saying no task can report in any more.
    Finished(Result<(), mpmc::RecvError>),
}

// -----------------------------------------------------------------------------
// AssetProcessServer: one pass

impl AssetProcessServer {
    /// One pass over `paths`: run them all, and wait for that to settle.
    ///
    /// The caller has already scanned the sources, so every asset is registered and the index
    /// describes the processed side.
    ///
    /// `recoverable == false` means the processed side cannot be trusted (an interrupted previous
    /// run): then nothing is compared against it and every asset is processed.
    pub(super) async fn process_paths(&self, paths: Vec<AssetPath<'static>>, recoverable: bool) {
        self.data
            .skip_up_to_date
            .store(recoverable, Ordering::SeqCst);

        let (tasks, receiver) = mpmc::channel::<Task>();
        for path in &paths {
            let _ = tasks.send(queued_task(path));
        }

        self.execute_processing_tasks(tasks, receiver, SupervisorMode::OneOff)
            .await;

        self.data.state.set_state(ProcessorState::Finished).await;
    }

    /// Runs the queued tasks, one task per asset, until the work settles.
    ///
    /// The tasks are spawned on the IO task pool, so a task that waits — a processor reading a
    /// dependency waits on the processed-side gate — parks instead of blocking the others. That is
    /// what makes a fresh tree resolve in dependency order: the asset whose status is being waited
    /// for is a task of its own, and it runs while the waiter is parked.
    ///
    /// The overall state follows the tasks: `Processing` while any is in flight, `Finished` when the
    /// last one ends. In [`SupervisorMode::OneOff`] the loop also ends there; in
    /// [`SupervisorMode::LongRunning`] it keeps waiting for new work (source changes queue it).
    pub(super) async fn execute_processing_tasks(
        &self,
        tasks: TaskSender,
        receiver: TaskReceiver,
        mode: SupervisorMode,
    ) {
        let (finished_sender, finished_receiver) = mpmc::channel::<()>();
        let mut pending = 0usize;

        // A second handle onto the same queue, used only to ask whether work is left: the handle the
        // loop receives on is borrowed by that receive future.
        let probe = receiver.clone();

        if probe.is_empty() {
            self.data.state.set_state(ProcessorState::Finished).await;
        }

        let mut receiver = receiver;
        let mut queued = Box::pin(receiver.recv());
        let mut finished_receiver = finished_receiver;
        let mut finished = Box::pin(finished_receiver.recv());

        loop {
            // Three things can end (or continue) a step: nothing is left to do, a task is queued, or
            // a task finished. The queued branch is polled before the finished one on purpose: a task
            // that is ready to start must be started before a finished task can be taken as "nothing
            // left to do". That is also why an empty queue is checked *inside* the poll: with a live
            // sender around, waiting for the next task would otherwise park forever.
            let event = poll_fn(|context| {
                if mode == SupervisorMode::OneOff && pending == 0 && probe.is_empty() {
                    return Poll::Ready(None);
                }

                if let Poll::Ready(result) = queued.as_mut().poll(context) {
                    return Poll::Ready(Some(TaskEvent::Queued(result)));
                }

                if let Poll::Ready(result) = finished.as_mut().poll(context) {
                    return Poll::Ready(Some(TaskEvent::Finished(result)));
                }

                Poll::Pending
            })
            .await;

            let Some(event) = event else {
                break;
            };

            match event {
                TaskEvent::Queued(Ok((source_id, path))) => {
                    pending += 1;

                    let this = self.clone();
                    let tasks = tasks.clone();
                    let finished_sender = finished_sender.clone();

                    IoTaskPool::get()
                        .spawn(async move {
                            let path = AssetPath::from(path).with_source_id(source_id);

                            this.process_asset_task(&path, &tasks).await;

                            // Even a task that could not start has to report in, or the importer
                            // would stay `Processing` forever.
                            let _ = finished_sender.send(());
                        })
                        .detach();

                    self.data.state.set_state(ProcessorState::Processing).await;

                    queued = Box::pin(receiver.recv());
                }
                TaskEvent::Queued(Err(_)) => break,
                TaskEvent::Finished(Ok(())) => {
                    pending -= 1;

                    if pending == 0 {
                        // The importer's server belongs to no world, so no typed drop job drains the
                        // handles it released: without this, the metadata of every asset the importer
                        // loaded would stay behind. The reference implementation does the same here.
                        self.consume_handle_drop_events();

                        self.data.state.set_state(ProcessorState::Finished).await;
                    }

                    finished = Box::pin(finished_receiver.recv());
                }
                TaskEvent::Finished(Err(_)) => break,
            }
        }

        drop(finished_sender);
    }

    /// Processes one asset as a task of a pass, and records what it did.
    ///
    /// The result is recorded through [`ProcessorAssetInfos::finish_processing`], which
    /// is also what queues the assets that depend on this one: a change to this asset's
    /// `full_hash` is what makes them out of date, so they are looked at again.
    ///
    /// [`ProcessorAssetInfos::finish_processing`]: crate::processor::infos::ProcessorAssetInfos::finish_processing
    async fn process_asset_task(&self, path: &AssetPath<'static>, tasks: &TaskSender) {
        let result = self.process_asset_internal(path).await;

        self.data
            .state
            .write_infos()
            .await
            .finish_processing(path.clone(), result, Some(tasks))
            .await;
    }

    /// Drops the metadata of every handle the importer's server has seen dropped.
    ///
    /// The importer's server belongs to no world, so no typed drop job drains those queues for it:
    /// without this, the metadata of every asset the importer loaded would stay behind. (A world's
    /// own server is drained by [`HandleAssetDropEvents`] instead, which is why this is only used
    /// here.)
    ///
    /// [`HandleAssetDropEvents`]: crate::jobs::HandleAssetDropEvents
    fn consume_handle_drop_events(&self) {
        self.server.0.write_infos().process_handle_drop_events();
    }
}

// -----------------------------------------------------------------------------
// Startup job

/// Starts the importer of an app.
///
/// This is the job [`AssetPlugin`] inserts into `Startup` in [`AssetServerMode::Processed`]:
/// [`AssetProcessServer::start`] hands the run to the IO task pool, so the schedule does not block
/// on the import, and the processed-side gate is what makes a read wait for it instead.
///
/// [`AssetPlugin`]: crate::plugin::AssetPlugin
/// [`AssetServerMode::Processed`]: crate::server::AssetServerMode::Processed
#[job_fn(type = StartAssetProcessServer)]
pub(crate) fn start_asset_process_server(server: Res<AssetProcessServer>) {
    server.start();
}
