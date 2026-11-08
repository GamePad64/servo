/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use log::debug;
use servo_config::pref;

/// The state of the thread-pool used by CoreResource.
struct ThreadPoolState {
    /// The number of active workers.
    active_workers: u32,
    /// Whether the pool can spawn additional work.
    active: bool,
}

impl ThreadPoolState {
    pub fn new() -> ThreadPoolState {
        ThreadPoolState {
            active_workers: 0,
            active: true,
        }
    }

    /// Is the pool still able to spawn new work?
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// How many workers are currently active?
    pub fn active_workers(&self) -> u32 {
        self.active_workers
    }

    /// Prevent additional work from being spawned.
    pub fn switch_to_inactive(&mut self) {
        self.active = false;
    }

    /// Add to the count of active workers.
    pub fn increment_active(&mut self) {
        self.active_workers += 1;
    }

    /// Subtract from the count of active workers.
    pub fn decrement_active(&mut self) {
        self.active_workers -= 1;
    }
}

/// The thread pools used throughout Servo, apart from those used by Layout and WebRender which
/// are handled separately.
pub struct ThreadPool {
    /// wasip2: there are no worker threads to build a pool from, so work runs
    /// inline on the caller. Callers already treat completion as asynchronous,
    /// so running sooner is safe; what is lost is parallelism, not ordering.
    #[cfg(not(target_family = "wasm"))]
    pool: rayon::ThreadPool,
    state: Arc<Mutex<ThreadPoolState>>,
}

static GLOBAL_THREAD_POOL: OnceLock<Arc<ThreadPool>> = OnceLock::new();

impl ThreadPool {
    /// Get the global thread pool for the process.
    pub fn global() -> Arc<Self> {
        let pool = GLOBAL_THREAD_POOL.get_or_init(|| {
            let paralellism = thread::available_parallelism()
                .map(|parallelism| parallelism.get())
                .unwrap_or(pref!(thread_pool_fallback_workers) as usize)
                .min(pref!(thread_pool_workers_max) as usize);
            #[cfg(not(target_family = "wasm"))]
            let pool = rayon::ThreadPoolBuilder::new()
                .thread_name(move |i| format!("GlobalPool#{i}"))
                .num_threads(paralellism)
                .build()
                .unwrap();
            let _ = paralellism;
            Arc::new(Self {
                #[cfg(not(target_family = "wasm"))]
                pool,
                state: Arc::new(Mutex::new(ThreadPoolState::new())),
            })
        });
        pool.clone()
    }

    /// Spawn work on the thread-pool, if still active.
    ///
    /// There is no need to give feedback to the caller,
    /// because if we do not perform work,
    /// it is because the system as a whole is exiting.
    pub fn spawn<OP>(&self, work: OP)
    where
        OP: FnOnce() + Send + 'static,
    {
        {
            let mut state = self.state.lock().unwrap();
            if state.is_active() {
                state.increment_active();
            } else {
                // Don't spawn any work.
                return;
            }
        }

        let state = self.state.clone();

        let run = move || {
            {
                let mut state = state.lock().unwrap();
                if !state.is_active() {
                    // Decrement number of active workers and return,
                    // without doing any work.
                    return state.decrement_active();
                }
            }
            // Perform work.
            work();
            {
                // Decrement number of active workers.
                let mut state = state.lock().unwrap();
                state.decrement_active();
            }
        };

        #[cfg(not(target_family = "wasm"))]
        self.pool.spawn(run);
        // wasip3 has real threads, but rayon is not part of the wasm
        // dependency set; a bare thread per work item preserves the native
        // "not on the caller's stack" ordering that the wasip2 tokio-task
        // deferral provided.
        #[cfg(target_family = "wasm")]
        thread::Builder::new()
            .name("GlobalPool".to_owned())
            .spawn(run)
            .expect("Failed to spawn thread-pool worker");
    }

    /// Prevent further work from being spawned,
    /// and wait until all workers are done,
    /// or a timeout of roughly one second has been reached.
    pub fn exit(&self) {
        {
            let mut state = self.state.lock().unwrap();
            state.switch_to_inactive();
        }
        let mut rounds = 0;
        loop {
            rounds += 1;
            {
                let state = self.state.lock().unwrap();
                let still_active = state.active_workers();

                if still_active == 0 || rounds == 10 {
                    if still_active > 0 {
                        debug!(
                            "Exiting ThreadPool with {:?} still working(should be zero)",
                            still_active
                        );
                    }
                    break;
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
}
