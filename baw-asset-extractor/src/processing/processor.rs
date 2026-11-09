use std::num::NonZeroUsize;
use std::sync::Arc;

use smol::Executor;
use smol::channel::{RecvError, SendError, Sender};
use tracing::{Level, debug, instrument, trace, trace_span, warn};

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
    pub fn run<F, Fut>(self, func: F) -> anyhow::Result<()>
    where
        F: 'p + FnOnce(ProcessorContext<'p>) -> Fut + Send,
        Fut: 'p + Future<Output = anyhow::Result<()>> + Send,
    {
        let (keep_alive, shutdown) = smol::channel::unbounded::<()>();
        let (error_reporter, error_receiver) = smol::channel::bounded::<anyhow::Error>(1);

        let ctx = ProcessorContext {
            executor: self.executor.clone(),
            error_reporter: error_reporter.clone(),
        };
        self.executor
            .spawn(wrap_task(func(ctx), error_reporter))
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
                        debug!("Received an error from an operation: {error_from_operation:?}");
                        Err(error_from_operation)
                    }
                    Err(RecvError) => {
                        trace!("All operations completed successfully.");
                        Ok(())
                    }
                }
            });
        res
    }
}

/// Context passed to processor operations.
#[derive(Clone)]
pub struct ProcessorContext<'a> {
    executor: Arc<Executor<'a>>,
    /// Signal to use to report when an operation fails. This also keeps the executor alive until
    /// either one operation fails or all operations complete.
    error_reporter: Sender<anyhow::Error>,
}

impl<'a> ProcessorContext<'a> {
    /// Spawn an operation. The returned future waits for the task to be spawned, but cannot wait
    /// for it to complete.
    pub fn spawn<F>(&self, fut: F) -> impl Future<Output = ()> + Send
    where
        F: 'a + Future<Output = anyhow::Result<()>> + Send,
    {
        self.executor
            .spawn(wrap_task(fut, self.error_reporter.clone()))
            .detach();
        // We force a yield in order to ensure that spawned tasks get a chance at the executor.
        smol::future::yield_now()
    }
}

/// Wrap the future for a task in a future that will keep the executor alive (by holding the
/// error_reporter channel open) and report any error emitted by the provided future.
#[instrument(skip_all, level = Level::TRACE)]
async fn wrap_task<'a, F>(fut: F, error_reporter: Sender<anyhow::Error>)
where
    F: 'a + Future<Output = anyhow::Result<()>> + Send,
{
    // We must hold the error_reporter until the inner future completes, otherwise the
    // executor could shutdown before it does. This also allows us to report if the task
    // fails.
    match fut.await {
        Ok(()) => {}
        Err(err) => {
            if let Err(SendError(err)) = error_reporter.send(err).await {
                warn!(
                    "Unable to report operation failure. This may indicate executor has \
                            already begun shutdown due to another error. Unreported error: {err:?}"
                );
            }
        }
    }
}
