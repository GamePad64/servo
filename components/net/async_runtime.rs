/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use futures::Future;
use net_traits::AsyncRuntime;
use tokio::runtime::{Builder, Handle, Runtime};

/// The actual runtime,
/// to be used as part of shut-down.
pub struct AsyncRuntimeHolder {
    runtime: Option<Runtime>,
}

impl AsyncRuntimeHolder {
    pub(crate) fn new(runtime: Runtime) -> Self {
        Self {
            runtime: Some(runtime),
        }
    }
}

impl AsyncRuntime for AsyncRuntimeHolder {
    fn shutdown(&mut self) {
        // On wasip2 there is no runtime of our own to shut down: we borrow the
        // embedder's. See `init_async_runtime`.
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(Duration::from_millis(100))
        }
    }
}

/// A shared handle to the runtime,
/// to be initialized on start-up.
static ASYNC_RUNTIME_HANDLE: OnceLock<Handle> = OnceLock::new();

pub fn init_async_runtime() -> Box<dyn AsyncRuntime> {
    // wasip2: there is one thread, and exactly one runtime — the one the
    // embedder is already driving. Building a second `current_thread` runtime
    // here compiles and accepts spawns, but nothing ever polls it, so every
    // fetch would be queued and never run. Borrow the running runtime instead.
    #[cfg(target_family = "wasm")]
    {
        ASYNC_RUNTIME_HANDLE
            .set(Handle::current())
            .expect("Runtime handle should be initialized once on start-up");
        return Box::new(AsyncRuntimeHolder { runtime: None });
    }

    // Initialize a tokio runtime.
    #[cfg(not(target_family = "wasm"))]
    let runtime = Builder::new_multi_thread()
        .thread_name_fn(|| {
            static ATOMIC_ID: AtomicUsize = AtomicUsize::new(0);
            let id = ATOMIC_ID.fetch_add(1, Ordering::Relaxed);
            format!("tokio-runtime-{}", id)
        })
        .worker_threads(
            thread::available_parallelism()
                .map(|i| i.get())
                .unwrap_or(servo_config::pref!(thread_pool_fallback_workers) as usize)
                .min(servo_config::pref!(thread_pool_async_runtime_workers_max).max(1) as usize),
        )
        .enable_io()
        .enable_time()
        .build()
        .expect("Unable to build tokio-runtime runtime");

    // Make the runtime available to users inside this crate.
    #[cfg(not(target_family = "wasm"))]
    ASYNC_RUNTIME_HANDLE
        .set(runtime.handle().clone())
        .expect("Runtime handle should be initialized once on start-up");

    // Return an async runtime for use in shutdown.
    #[cfg(not(target_family = "wasm"))]
    Box::new(AsyncRuntimeHolder::new(runtime))
}

pub fn async_runtime_initialized() -> bool {
    ASYNC_RUNTIME_HANDLE.get().is_some()
}

/// Spawn a task using the handle to the runtime.
pub fn spawn_task<F>(task: F)
where
    F: Future + 'static + std::marker::Send,
    F::Output: Send + 'static,
{
    ASYNC_RUNTIME_HANDLE
        .get()
        .expect("Runtime handle should be initialized on start-up")
        .spawn(task);
}

/// Spawn a blocking task using the handle to the runtime.
pub fn spawn_blocking_task<F, R>(task: F) -> F::Output
where
    F: Future,
{
    ASYNC_RUNTIME_HANDLE
        .get()
        .expect("Runtime handle should be initialized on start-up")
        .block_on(task)
}
