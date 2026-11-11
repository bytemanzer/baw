use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use smol::Executor;
use smol::channel::{self, Receiver, RecvError, SendError, Sender};
use smol::stream::Stream;
use thiserror::Error;
use tracing::{Instrument, Level, instrument, trace, trace_span, warn};

pub use smol::future::YieldNow;

/// Processes a set of operations in parallel.
pub struct Processor<'p> {
    executor: Arc<Executor<'p>>,
    num_threads: NonZeroUsize,
}

impl<'p> Processor<'p> {
    /// Create a new executor.
    pub fn new() -> Self {
        Self {
            executor: Arc::new(Executor::new()),
            num_threads: NonZeroUsize::new(1).unwrap(),
        }
    }

    /// Set the number of threads to run.
    pub fn num_threads(mut self, n: NonZeroUsize) -> Self {
        self.num_threads = n;
        self
    }

    /// Starts the processor with the provided root operation. The processor will run until the root
    /// operation and all sub-tasks it spawns have completed.
    pub fn run<F, Fut, T>(self, func: F) -> anyhow::Result<T>
    where
        F: 'p + FnOnce(ProcessorContext<'p>) -> Fut + Send,
        Fut: 'p + Future<Output = anyhow::Result<T>> + Send,
        T: 'p + Send,
    {
        let (keep_alive, shutdown) = smol::channel::unbounded::<()>();
        let (error_reporter, error_receiver) = smol::channel::bounded::<anyhow::Error>(1);
        let (initial_task_sender, initial_task_receiver) = smol::channel::bounded::<T>(1);

        let ctx = ProcessorContext {
            executor: self.executor.clone(),
            error_reporter: error_reporter.clone(),
        };
        self.executor
            .spawn(wrap_task_wait(func(ctx), error_reporter, initial_task_sender).in_current_span())
            .detach();

        let ((), res) = easy_parallel::Parallel::new()
            .each(0..self.num_threads.get(), |id| {
                let _span = trace_span!("Running worker thread", id).entered();
                // Each worker thread runs executor tasks until the shutdown signal completes. We
                // never send anything on the shutdown signal, this will always receive a 'channel
                // closed' notification, which we can ignore.
                let res = smol::future::block_on(self.executor.run(shutdown.recv()));
                // Nothing should ever actually be sent on the keep-alive/shutdown channel.
                debug_assert!(matches!(res, Err(RecvError)));
                trace!("Worker {id} shutting down");
            })
            .finish_in(move || {
                let _span = trace_span!("Waiting for workers").entered();
                // Hold the keep-alive guard here, so it will be dropped whenever the finisher
                // completes with either an error from one of the tasks or
                let _keep_alive = keep_alive;
                match error_receiver.recv_blocking() {
                    Ok(error_from_operation) => {
                        trace!("Received an error from an operation: {error_from_operation:?}");
                        Err(error_from_operation)
                    }
                    Err(RecvError) => {
                        trace!("All operations completed successfully.");
                        match initial_task_receiver.recv_blocking() {
                            Ok(res) => Ok(res),
                            Err(RecvError) => {
                                panic!(
                                    "All tasks completed successfully but initial task never sent \
                                    its result"
                                );
                            }
                        }
                    }
                }
            });
        res
    }
}

/// Context passed to processor operations.
#[derive(Clone)]
pub struct ProcessorContext<'p> {
    executor: Arc<Executor<'p>>,
    /// Signal to use to report when an operation fails. This also keeps the executor alive until
    /// either one operation fails or all operations complete.
    error_reporter: Sender<anyhow::Error>,
}

impl<'p> ProcessorContext<'p> {
    /// Spawn a detached operation. The operation will run fully independently, with no way to check
    /// up on it.
    ///
    /// The spawned future must return a result. If it fails, it will abort the content
    /// [`Processor`] run.
    ///
    /// You should generally yield when spawning new tasks to give the newly spawned task a chance
    /// to run. To help with this, we return a [`YieldNow`] future which you can await to yield
    /// once. You can safely ignore the return value if you really don't want to yield.
    pub fn spawn<F>(&self, fut: F) -> YieldNow
    where
        F: 'p + Future<Output = anyhow::Result<()>> + Send,
    {
        self.executor
            .spawn(wrap_task(fut, self.error_reporter.clone()).in_current_span())
            .detach();
        smol::future::yield_now()
    }

    /// Spawn a task to run independently, but get a handle to wait for it to complete.
    ///
    /// Like [`spawn`][ProcessorContext::spawn], the spawned future must return a result. If it
    /// fails, it will abort the [`Processor`] run.
    ///
    /// There is no way for the caller to handle errors produced by the future after this point.
    pub fn spawn_wait<F, T>(&self, fut: F) -> SpawnWait<T>
    where
        F: 'p + Future<Output = anyhow::Result<T>> + Send,
        T: 'p + Send,
    {
        let (send, recv) = channel::bounded(1);
        self.executor
            .spawn(wrap_task_wait(fut, self.error_reporter.clone(), send).in_current_span())
            .detach();
        SpawnWait { recv: Some(recv) }
    }
}

/// Wrap the future for a task in a future that will keep the executor alive (by holding the
/// error_reporter channel open) and report any error emitted by the provided future.
#[instrument(skip_all, level = Level::TRACE)]
async fn wrap_task<'p, F>(fut: F, error_reporter: Sender<anyhow::Error>)
where
    F: 'p + Future<Output = anyhow::Result<()>> + Send,
{
    // We must hold the error_reporter until the inner future completes, otherwise the
    // executor could shutdown before it does. This also allows us to report if the task
    // fails.
    match fut.await {
        Ok(()) => {}
        Err(err) => {
            if let Err(SendError(_)) = error_reporter.send(err).await {
                trace!(
                    "Unable to send error report. Executor is probably already shutting down due \
                    to another error"
                );
            }
        }
    }
}

/// Wrap the future for a task in a future that will keep the executor alive (by holding the
/// error_reporter channel open) and report any error emitted by the provided future.
///
/// It will also pass any success result back to the given waiter.
///
/// We do this in a single async funtion so that on a failure, we send the error report to the
/// error_reporter *before* we close the waiter. Otherwise, we'd create a race condition where the
/// `TaskFailed` received by the waiter might end up being reported *before* our failure, which
/// would result in a vague and useless final error message.
#[instrument(skip_all, level = Level::TRACE)]
async fn wrap_task_wait<'p, F, T>(fut: F, error_reporter: Sender<anyhow::Error>, waiter: Sender<T>)
where
    F: 'p + Future<Output = anyhow::Result<T>> + Send,
    T: 'p + Send,
{
    // We must hold the error_reporter until the inner future completes, otherwise the
    // executor could shutdown before it does. This also allows us to report if the task
    // fails.
    match fut.await {
        Ok(value) => {
            if let Err(_) = waiter.send(value).await {
                trace!("SpawnWait detached before receiving result");
            }
        }
        Err(err) => {
            if let Err(SendError(_)) = error_reporter.send(err).await {
                trace!(
                    "Unable to send error report. Executor is probably already shutting down due \
                    to another error"
                );
            }
            // Ensure the error_reporter is also dropped before the waiter.
            drop(error_reporter);
        }
    }
}

/// Future type returned from [`spawn_wait`][ProcessorContext::spawn_wait].
///
/// Allows waiting on the result of a spawned task. If dropped, the spawned task will complete
/// independently.
///
/// If the inner future fails, you will get a [`TaskFailed`] error. There is no way to handle or
/// recover from inner task failures with the [`SpawnWait`] future; if you get a [`TaskFailed`], the
/// executor will be shutting down anyway, so you should just stop.
pub struct SpawnWait<T> {
    recv: Option<Receiver<T>>,
}

impl<T> SpawnWait<T> {
    /// Pin-project the 'recv' field.
    fn recv(self: Pin<&mut Self>) -> Option<Pin<&mut Receiver<T>>> {
        // SAFETY: We're not moving anything out of self, so everything will stay pinned where it
        // is.
        unsafe { self.get_unchecked_mut() }
            .recv
            .as_mut()
            // SAFETY: recv is pinned because `self` is pinned, so we can safely pin-project to it.
            // Nothing will move it util it is destroyed.
            .map(|recv| unsafe { Pin::new_unchecked(recv) })
    }

    /// Clear the receive field.
    fn clear(self: Pin<&mut Self>) {
        // SAFETY: we're destroying a pinned value in-place by overwriting it, so this should
        // satisfy pinning requirements.
        unsafe { self.get_unchecked_mut() }.recv = None;
    }
}

impl<T> Future for SpawnWait<T> {
    type Output = Result<T, TaskFailed>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().recv() {
            Some(recv) => match recv.poll_next(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(res) => {
                    // We expect to get exactly 1 message, either a result or None if the channel
                    // closed early. Either way, clear the receiver to fuse the future.
                    self.clear();
                    Poll::Ready(res.ok_or(TaskFailed { _private: () }))
                }
            },
            None => {
                warn!("SpawnWait future polled after completion");
                Poll::Pending
            }
        }
    }
}

/// A task created with [`spawn_wait`][ProcessorContext::spawn_wait] failed internally. When this
/// happens it usually means the executor is shutting down, so you should not both continuing work.
#[derive(Error, Debug)]
#[error("Spawned task failed.")]
pub struct TaskFailed {
    _private: (),
}
