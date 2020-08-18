//! This is an executor executes futures by parking the current thread until unparked by the future
//! waking it. It has an execution method for blocking indefinitely until the future unparks it, or
//! a method for accepting a timeout, after which the thread will wake itself up and return an Err

use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::thread::{self, Thread};
use std::time::Duration;

use tokio::time::Instant;

/// Convenience function for a commonly used pattern in the crate. Callers may have a fallible future,
/// along with a potential timeout.
///
/// If there is a timeout, then the future should execute with it. If the future times out, then callers
/// can specify a transformation on the `TimedOut` error (perhaps to wrap it in another error). If the
/// future succeeds in time, or if there was no timeout, then the result of the future is returned.
pub(crate) fn execute_blocking_flatten_timeout<F, O, M, E>(future: F, maybe_timeout: Option<Duration>, timeout_transform: M) -> Result<O, E>
where
    F: Future<Output = Result<O, E>>,
    M: FnOnce(crate::error::TimedOut) -> E,
{
    if let Some(timeout) = maybe_timeout {
        execute_blocking_timeout(future, timeout)
            .map_err(timeout_transform)?
    } else {
        execute_blocking(future)
    }
}

/// Execute a future on this thread by parking the thread indefinitely until the future wakes it up.
pub(crate) fn execute_blocking<F, O>(future: F) -> O
where
    F: Future<Output=O>
{
    enter();
    let waker = current_thread_waker();
    let mut cx = Context::from_waker(&waker);
    futures_util::pin_mut!(future);

    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return output
        }
        thread::park();
    }
}

/// Execute a future on this thread with a timeout. The thread will sleep for at most `timeout`, and
/// if the future has not woken us and returned by then, then the future will return `Err(TimedOut)`.
pub(crate) fn execute_blocking_timeout<F, O>(future: F, timeout: Duration) -> Result<O, crate::error::TimedOut>
where
    F: Future<Output=O>
{
    enter();
    let waker = current_thread_waker();
    let mut cx = Context::from_waker(&waker);
    futures_util::pin_mut!(future);

    let deadline = Instant::now() + timeout;

    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return Ok(output)
        }

        let now = Instant::now();
        if now >= deadline {
            return Err(crate::error::TimedOut)
        }

        thread::park_timeout(deadline - now);
    }
}

struct ThreadWaker(Thread);

impl futures_util::task::ArcWake for ThreadWaker {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.0.unpark();
    }
}

fn current_thread_waker() -> Waker {
    let thread = ThreadWaker(thread::current());
    // Arc shouldn't be necessary, since `Thread` is reference counted internally,
    // but let's just stay safe for now.
    futures_util::task::waker(Arc::new(thread))
}

fn enter() {
    // Check we aren't already in a runtime
    #[cfg(debug_assertions)]
    {
        tokio::runtime::Builder::new()
            .core_threads(1)
            .build()
            .expect("build shell runtime")
            .enter(|| {});
    }
}
