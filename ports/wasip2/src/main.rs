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

use servo::RenderingContext as _;

/// Minimal stderr logger. Servo logs through the `log` facade; without a logger
/// installed its startup is silent, which makes a hang impossible to place.
struct Stderr;
impl log::Log for Stderr {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        eprintln!("[{}] {}: {}", record.level(), record.target(), record.args());
    }
    fn flush(&self) {}
}
static LOGGER: Stderr = Stderr;

fn main() {
    log::set_logger(&LOGGER).ok();
    // Quiet by default; `SERVO_LOG=debug` (or trace/info) opens it up without a
    // rebuild, which matters when each rebuild links a 119MB binary.
    log::set_max_level(
        std::env::var("SERVO_LOG")
            .ok()
            .and_then(|level| level.parse().ok())
            .unwrap_or(log::LevelFilter::Warn),
    );

    let mut html = String::new();
    std::io::stdin()
        .read_to_string(&mut html)
        .expect("failed to read HTML from stdin");

    // Single-threaded by necessity, not by choice — see the module docs.
    let runtime = tokio::runtime::Builder::new_current_thread()
        // IO as well as time: `tokio::net::TcpStream` needs the IO driver, and
        // that is how this build reaches the network (see `net::connector`).
        .enable_all()
        .build()
        .expect("failed to build current_thread runtime");

    let local = tokio::task::LocalSet::new();
    let dump = local.block_on(&runtime, run(&html));
    println!("{dump}");
}

async fn run(html: &str) -> String {
    // `inventory` collects nothing under wasip2, so register the reader
    // explicitly rather than relying on life-before-main.
    servo::resources::set_resource_reader(&servo_default_resources::DefaultResourceReader);

    eprintln!("[harness] calling ServoBuilder::build()");
    let servo = servo::ServoBuilder::default().build();
    eprintln!("[harness] build() returned");

    // CPU rasterisation: swgl gives WebRender a real GL implementation that
    // writes into ordinary memory. There is no GPU inside a component.
    let size = servo::PhysicalSize::new(WIDTH, HEIGHT);
    let rendering_context = std::rc::Rc::new(
        servo::SoftwareRenderingContext::new(size).expect("software rendering context"),
    );
    eprintln!("[harness] software rendering context created {WIDTH}x{HEIGHT}");

    // A line starting with a scheme is loaded as-is; anything else is treated as
    // a document to render inline.
    let trimmed = html.trim();
    let url = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_owned()
    } else {
        format!("data:text/html;charset=utf-8,{}", percent_encode(html))
    };
    let webview = servo::WebViewBuilder::new(&servo, rendering_context.clone())
        .url(servo::ServoUrl::parse(&url).expect("data url").into_url())
        .build();
    eprintln!("[harness] webview built, loading");

    // Drive the event loop until the load completes.
    //
    // The wait sleeps rather than spinning on `yield_now`. Socket readiness is
    // only delivered when the runtime parks, and a runtime whose queue is never
    // empty does not park — so a busy wait starves exactly the network IO this
    // loop is waiting for.
    let deadline = std::time::Instant::now() + LOAD_TIMEOUT;
    loop {
        servo.spin_event_loop();
        tokio::time::sleep(POLL_INTERVAL).await;
        if webview.load_status() == servo::LoadStatus::Complete {
            eprintln!("[harness] load complete");
            break;
        }
        if std::time::Instant::now() >= deadline {
            eprintln!("[harness] load timed out, status={:?}", webview.load_status());
            break;
        }
    }

    // One paint is not enough: layout, display-list building, scene building and
    // frame building each hand off to a different actor, and each hand-off needs
    // a turn of the loop before the result reaches the framebuffer.
    let settle_turns = std::env::var("SERVO_SETTLE_MS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(SETTLE_SPINS);
    for _ in 0..settle_turns {
        webview.paint();
        servo.spin_event_loop();
        // Sleeping rather than yielding here for the same reason as the load
        // wait: anything still arriving over the network — a late image, a
        // lazily-fetched font — needs the runtime to park to be noticed.
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    // Interaction probe: click where SERVO_CLICK says, then let the page react.
    if let Ok(spec) = std::env::var("SERVO_CLICK") {
        let mut parts = spec.split(',');
        let x: f32 = parts.next().and_then(|v| v.trim().parse().ok()).unwrap_or(0.0);
        let y: f32 = parts.next().and_then(|v| v.trim().parse().ok()).unwrap_or(0.0);
        eprintln!("[harness] clicking ({x}, {y})");
        let point = servo::WebViewPoint::Page(euclid::Point2D::new(x, y));
        webview.notify_input_event(servo::InputEvent::MouseMove(servo::MouseMoveEvent::new(point)));
        eprintln!("[harness] mouse move delivered");
        webview.notify_input_event(servo::InputEvent::MouseButton(
            servo::MouseButtonEvent::new(
                servo::MouseButtonAction::Down,
                servo::MouseButton::Left,
                point,
            ),
        ));
        eprintln!("[harness] mouse down delivered");
        webview.notify_input_event(servo::InputEvent::MouseButton(
            servo::MouseButtonEvent::new(
                servo::MouseButtonAction::Up,
                servo::MouseButton::Left,
                point,
            ),
        ));
        eprintln!("[harness] mouse up delivered");
        for _ in 0..500 {
            servo.spin_event_loop();
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        eprintln!("[harness] settled after click");
    }

    // The rendered DOM, read back through the same JavaScript path an embedder
    // would use. This doubles as proof that scripts run: a page whose content is
    // built by JS shows up here and nowhere else.
    let dom: std::rc::Rc<std::cell::RefCell<Option<String>>> = Default::default();
    let sink = dom.clone();
    webview.evaluate_javascript("document.documentElement.outerHTML", move |result| {
        *sink.borrow_mut() = Some(match result {
            Ok(servo::JSValue::String(html)) => html,
            Ok(other) => format!("<unexpected JS value: {other:?}>"),
            Err(error) => format!("<evaluation failed: {error:?}>"),
        });
    });
    let mut waited = 0u32;
    while dom.borrow().is_none() && waited < MAX_SPINS {
        servo.spin_event_loop();
        tokio::task::yield_now().await;
        waited += 1;
    }
    match dom.borrow().as_deref() {
        Some(html) => eprintln!("[harness] DOM ({} bytes after {waited} spins):\n{html}", html.len()),
        None => eprintln!("[harness] DOM unavailable after {waited} spins"),
    }

    let rect = servo::DeviceIntRect::from_origin_and_size(
        servo::DeviceIntPoint::new(0, 0),
        servo::DeviceIntSize::new(WIDTH as i32, HEIGHT as i32),
    );
    let shot = rendering_context.read_to_image(rect);
    let summary = match &shot {
        Some(image) => {
            let non_white = image
                .pixels()
                .filter(|p| p.0[0] != 255 || p.0[1] != 255 || p.0[2] != 255)
                .count();
            std::fs::write("/tmp/servo/screenshot.raw", image.as_raw()).ok();
            format!(
                "SCREENSHOT {}x{}, {} non-white pixels, raw written",
                image.width(),
                image.height(),
                non_white
            )
        },
        None => "SCREENSHOT unavailable".to_string(),
    };

    eprintln!("[harness] {summary}");

    // Servo's teardown is a blocking round-trip: `RenderApi::shut_down` waits
    // for the render backend, which is a task on this same runtime and so can
    // never answer while we block. Leak the whole graph instead — the process
    // is about to exit.
    std::mem::forget(webview);
    std::mem::forget(rendering_context);
    std::mem::forget(servo);
    summary
}

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;
const MAX_SPINS: u32 = 3_000;
/// How long to wait for a load before giving up on it.
const LOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// Gap between turns of the event loop while waiting.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1);
/// Turns of the event loop given to the paint pipeline after the load completes.
const SETTLE_SPINS: u32 = 300;

/// Percent-encode just enough for a `data:` URL.
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            },
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
