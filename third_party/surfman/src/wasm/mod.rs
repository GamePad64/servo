// surfman/surfman/src/wasm/mod.rs
//
//! A null surfman backend for wasm targets without a browser (`wasm32-wasip2`).
//!
//! surfman dispatches to a native GPU backend per platform — CGL, WGL, EGL/X11,
//! Wayland, ANGLE. A wasm guest has none of them: there is no display server, no
//! GL driver, and no `wasi-gpu` proposal to bind to. Without this module the
//! crate does not compile for the target at all, because `pub use … as default`
//! matches no arm.
//!
//! Every entry point therefore answers with a value — `Error::UnsupportedOnThisPlatform`
//! or an empty result — rather than panicking. Nothing here rasterizes; a headless
//! Servo build reaches DOM and layout without ever presenting a surface.
//!
//! The Servo-on-wasip2 port does not intend to render through WebRender at all —
//! the design intercepts Servo's display list and rasterizes it with `vello_cpu`
//! on the CPU — so this backend is expected to stay a stub permanently rather
//! than grow into a real implementation.

pub mod connection;
pub mod context;
pub mod device;
pub mod surface;
