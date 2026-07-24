// surfman/surfman/src/wasm/surface.rs
//
//! Surface types for a target with no GPU surfaces.
//!
//! As with contexts, these exist to complete the public API. No `Surface` can be
//! created through the public API, because every path to one goes through a
//! `Device`, which cannot be opened.

use euclid::default::Size2D;

use crate::ContextID;

/// A GPU surface. Never successfully created on this platform.
#[derive(Debug)]
pub struct Surface {
    pub(crate) size: Size2D<i32>,
    pub(crate) context_id: ContextID,
}

/// A surface that has been bound as a texture. Never created on this platform.
#[derive(Debug)]
pub struct SurfaceTexture {
    pub(crate) surface: Surface,
}

/// A native window handle. There are no windows in a wasm guest.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeWidget;
