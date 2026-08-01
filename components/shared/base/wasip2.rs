/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Waiting, on a target that cannot park a thread.
//!
//! wasip2 has no threads: every Servo actor is a task on one runtime. A
//! blocking wait there stalls all of them — including whichever actor produces
//! the thing being waited for, which turns an ordinary hand-off into a deadlock.
//! Every wait in this port is therefore a poll with a yield between attempts,
//! and this is the one place that shape is written down.

/// Await a value that is only obtainable by polling.
///
/// `poll` is retried until it returns `Some`, yielding between attempts so the
/// other actors on this runtime get their turn.
///
/// **Provisional.** A cooperative spin burns CPU while idle. The proper fix is a
/// channel variant backed by `tokio::sync::mpsc`, which parks the task instead;
/// that reaches every construction site, so it is deferred rather than done
/// here. Note also that a spin only lets *other tasks* run — it does not let the
/// runtime park, and socket readiness is delivered only when it parks. Anything
/// waiting on network IO must sleep rather than call this.
pub async fn poll_until<T>(mut poll: impl FnMut() -> Option<T>) -> T {
    loop {
        if let Some(value) = poll() {
            return value;
        }
        tokio::task::yield_now().await;
    }
}
