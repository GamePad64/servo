/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Glyph rasterization backend for `wasm32-wasip2`.
//!
//! WebRender selects a font backend by target: CoreText on Apple, DirectWrite on
//! Windows, FreeType under `unix`. A wasm target matches none of them — `unix` is
//! false when `target_family = "wasm"` — which is a hard compile error, not a
//! missing feature.
//!
//! This backend exists so the crate **compiles** for wasm. It rasterizes nothing:
//! every entry point answers with a value (`None`, empty, `Err(LoadFailed)`)
//! rather than panicking, per the port's degradation rule.
//!
//! Why a stub rather than a FreeType port: the Servo-on-wasip2 port does not
//! render through WebRender at all. The plan is to intercept Servo's display list
//! and rasterize it with `vello_cpu`, so a real WebRender font backend is very
//! likely never needed. Should that change, FreeType itself does build for
//! `wasm32-wasip2` — it needs `-mllvm -wasm-enable-sjlj -fwasm-exceptions`,
//! because it uses `setjmp`/`longjmp` — but `platform/unix/font.rs` also resolves
//! optional FreeType symbols through `dlsym(RTLD_DEFAULT, …)`, which has no
//! meaning under wasm and would have to be replaced with direct linkage.

#![allow(non_camel_case_types)]

use std::sync::Arc;

use api::{FontInstanceFlags, FontKey, GlyphDimensions, NativeFontHandle};

use crate::rasterizer::{FontInstance, GlyphKey, GlyphRasterError, GlyphRasterResult};

pub struct FontContext {
    // No state: nothing is loaded and nothing is rasterized.
}

impl FontContext {
    /// Single-threaded: wasip2 has no threads, so rasterization is never
    /// distributed regardless of this backend being a stub.
    pub fn distribute_across_threads() -> bool {
        false
    }

    pub fn new() -> FontContext {
        FontContext {}
    }

    pub fn add_raw_font(&mut self, _font_key: &FontKey, _bytes: Arc<Vec<u8>>, _index: u32) {}

    pub fn add_native_font(&mut self, _font_key: &FontKey, _handle: NativeFontHandle) {}

    pub fn delete_font(&mut self, _font_key: &FontKey) {}

    pub fn delete_font_instance(&mut self, _instance: &FontInstance) {}

    /// No font is ever loaded, so no character maps to a glyph.
    pub fn get_glyph_index(&mut self, _font_key: FontKey, _ch: char) -> Option<u32> {
        None
    }

    pub fn get_glyph_dimensions(
        &mut self,
        _font: &FontInstance,
        _key: &GlyphKey,
    ) -> Option<GlyphDimensions> {
        None
    }

    /// Mirrors the other backends' behaviour of forcing subpixel positioning off
    /// when the backend cannot honour it.
    pub fn prepare_font(font: &mut FontInstance) {
        font.flags &= !FontInstanceFlags::SUBPIXEL_POSITION;
    }

    pub fn begin_rasterize(_font: &FontInstance) {}

    pub fn end_rasterize(_font: &FontInstance) {}

    pub fn rasterize_glyph(
        &mut self,
        _font: &FontInstance,
        _key: &GlyphKey,
    ) -> GlyphRasterResult {
        Err(GlyphRasterError::LoadFailed)
    }
}

impl Default for FontContext {
    fn default() -> Self {
        Self::new()
    }
}
