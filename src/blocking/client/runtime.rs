use std::thread;

use futures_core::Future;
use log::{error, trace};
use tokio::sync::{mpsc, oneshot};

use crate::{async_impl, blocking::executor, Request};

use super::event_loop_panicked;

/// A one-shot channel to send the response from the client runtime thread back to the original thread.
type OneshotResponder = oneshot::Sender<crate::Result<async_impl::Response>>;
/// The input for the runtime's task queue. Each task is a request and a channel to respond with.
type TaskQueueSender = mpsc::UnboundedSender<(Request, OneshotResponder)>;

pub struct ClientRuntime {
    /// The sender is an option for the purposes of shutting down the runtime, but should always be Some.
    pub(super) task_queue_sender: Option<TaskQueueSender>,
    /// The runtime is an option to take ownership and join the thread when dropping the runtime.
    runtime_thread: Option<thread::JoinHandle<()>>,
}

impl Drop for ClientRuntime {
    fn drop(&mut self) {
        let id = self.runtime_thread
            .as_ref()
            .expect("thread not dropped yet")
            .thread().id();

        trace!("closing runtime thread ({:?})", id);
        self.task_queue_sender.take();
        trace!("signaled close for runtime thread ({:?})", id);
        self.runtime_thread.take().map(|h| h.join());
        trace!("closed runtime thread ({:?})", id);
    }
}

impl ClientRuntime {
    pub fn new(client: async_impl::Client) -> crate::Result<ClientRuntime> {
        let (task_queue_sender, mut task_queue_receiver) = mpsc::unbounded_channel::<(Request, OneshotResponder)>();
        let (runtime_startup_indicator_tx, runtime_startup_indicator_rx) = oneshot::channel::<crate::Result<()>>();
        let runtime_thread = thread::Builder::new()
            .name("reqwest-internal-sync-runtime".into())
            .spawn(move || {
                let mut tokio_runtime = match tokio::runtime::Builder::new().basic_scheduler().enable_all().build() {
                    Err(err) => {
                        if let Err(send_err) = runtime_startup_indicator_tx.send(Err(crate::error::builder(err))) {
                            error!("Failed to communicate runtime creation failure: {:?}", send_err);
                        }
                        return;
                    }
                    Ok(value) => value,
                };
                if let Err(send_err) = runtime_startup_indicator_tx.send(Ok(())) {
                    error!("Failed to communicate runtime creation success: {:?}", send_err);
                    return;
                }

                trace!("({:?}) start runtime::block_on", thread::current().id());
                tokio_runtime.block_on(async move {
                    // Continue receiving tasks from the queue until the sender is dropped (indicated by
                    // receiving a None).
                    while let Some((req, responder)) = task_queue_receiver.recv().await {
                        tokio::spawn(await_while_open(client.send(req), responder));
                    }

                    trace!("({:?}) Receiver is shutdown", thread::current().id());
                });
                trace!("({:?}) end runtime::block_on", thread::current().id());
                drop(tokio_runtime);
                trace!("({:?}) finished", thread::current().id());
            })
            .map_err(crate::error::builder)?;

        // Wait for the runtime thread to start up...
        match executor::execute_blocking(runtime_startup_indicator_rx) {
            Ok(Ok(_)) => (),
            Ok(Err(err)) => return Err(err),
            Err(_cancelled) => event_loop_panicked(),
        }

        Ok(ClientRuntime {
            task_queue_sender: Some(task_queue_sender),
            runtime_thread: Some(runtime_thread),
        })
    }
}

/// Await the future for a response as long as the receiving end of the responder is still available.
/// If the receiver of the response is dropped, then simply do nothing since the request is cancelled.
async fn await_while_open<F>(request_future: F, mut responder: OneshotResponder)
    where
        F: Future<Output=crate::Result<async_impl::Response>>,
{
    use std::task::Poll;

    futures_util::pin_mut!(request_future);

    // "select" on the sender being canceled, and the future completing
    let res = futures_util::future::poll_fn(|cx| {
        match request_future.as_mut().poll(cx) {
            Poll::Ready(val) => Poll::Ready(Some(val)),
            // check if the callback is canceled
            Poll::Pending => responder.poll_closed(cx).map(|_| None)
        }
    })
        .await;

    if let Some(res) = res {
        let _ = responder.send(res);
    }
    // else request is canceled
}
