/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Minimal single-threaded Servo host for `wasm32-wasip2`.
//!
//! Reads HTML from stdin, drives Servo far enough to produce a parsed document,
//! and prints a DOM dump. No network, no rendering — this is the G1 gate.
//!
//! # Why the runtime is shaped this way
//!
//! `std::thread::spawn` does not fail to compile on wasip2, it fails at *run
//! time* with `os error 58`. Every Servo actor must therefore live as a task on
//! one runtime rather than on its own OS thread.
//!
//! It has to be a `LocalSet`, not plain `tokio::spawn`: the script thread owns
//! SpiderMonkey's `JSContext` and the DOM, neither of which is `Send`.
//!
//! The corresponding hazard is that a blocking `recv()` inside a task on a
//! single-threaded runtime blocks the whole runtime. The census in
//! `docs/plans/servo-port-notes.md` found exactly one such site in the core
//! (`script_thread.rs`, `ask_constellation_for_top_level_info`), reachable only
//! through `window.opener` — which this gate does not exercise.

use std::io::Read;

fn main() {
    let mut html = String::new();
    std::io::stdin()
        .read_to_string(&mut html)
        .expect("failed to read HTML from stdin");

    // Single-threaded by necessity, not by choice — see the module docs.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("failed to build current_thread runtime");

    let local = tokio::task::LocalSet::new();
    let dump = local.block_on(&runtime, run(&html));
    println!("{dump}");
}

async fn run(html: &str) -> String {
    // Grows in Task 5 as Servo's actors are converted one at a time. Until then
    // this proves the runtime scaffold links against libservo and starts under
    // wasmtime — which is itself worth confirming before touching any actor,
    // since a wasip2 runtime that cannot even start would invalidate the plan.
    let parallelism = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(0);
    format!(
        "BOOT OK: {} bytes of HTML received, available_parallelism={}",
        html.len(),
        parallelism
    )
}
