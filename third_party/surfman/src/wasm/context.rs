// surfman/surfman/src/wasm/context.rs
//
//! OpenGL context types for a target with no OpenGL.
//!
//! These types exist so the crate's public API is complete on wasm. No value of
//! `Context` can be constructed through the public API — `Device::create_context`
//! always fails — so the uninhabited-in-practice fields are never observed.

use crate::ContextAttributes;

/// An OpenGL context. Never successfully created on this platform.
#[derive(Debug)]
pub struct Context {
    pub(crate) id: crate::ContextID,
}

/// The immutable descriptor a context is created from.
#[derive(Clone, Debug)]
pub struct ContextDescriptor {
    pub(crate) attributes: ContextAttributes,
}

/// A native OpenGL context handle. There is nothing to hold.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeContext;
