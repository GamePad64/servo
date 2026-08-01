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
//! This backend is built on `ab_glyph`, which parses faces and fills outlines in
//! pure Rust. That matters more than convenience here: FreeType does build for
//! `wasm32-wasip2` (given `-mllvm -wasm-enable-sjlj -fwasm-exceptions`, since it
//! uses `setjmp`/`longjmp`), but `platform/unix/font.rs` also resolves optional
//! FreeType symbols through `dlsym(RTLD_DEFAULT, …)`, which has no meaning under
//! wasm and would have to be replaced with direct linkage.
//!
//! What is deliberately not implemented: subpixel anti-aliasing, gamma
//! correction, embedded colour bitmaps, synthetic bold and glyph transforms.
//! Every glyph comes back as an 8-bit coverage mask expanded to BGRA, which is
//! what `GlyphFormat::Alpha` asks for.

#![allow(non_camel_case_types)]

use std::collections::HashMap;
use std::sync::Arc;

use ab_glyph::{Font, FontVec, GlyphId as AbGlyphId, point};
use api::{FontInstanceFlags, FontKey, GlyphDimensions, NativeFontHandle};

use crate::rasterizer::{FontInstance, GlyphKey, GlyphRasterError, GlyphRasterResult, RasterizedGlyph};

pub struct FontContext {
    fonts: HashMap<FontKey, FontVec>,
}

impl FontContext {
    /// Single-threaded: wasip2 has no threads, so rasterization cannot be
    /// distributed no matter how capable this backend is.
    pub fn distribute_across_threads() -> bool {
        false
    }

    pub fn new() -> FontContext {
        FontContext {
            fonts: HashMap::new(),
        }
    }

    pub fn add_raw_font(&mut self, font_key: &FontKey, bytes: Arc<Vec<u8>>, index: u32) {
        if self.fonts.contains_key(font_key) {
            return;
        }
        match FontVec::try_from_vec_and_index(bytes.as_ref().clone(), index) {
            Ok(font) => {
                self.fonts.insert(*font_key, font);
            },
            Err(error) => {
                log::warn!("Failed to parse font {font_key:?}: {error}");
            },
        }
    }

    /// A native font handle names a file for the host's font library to open.
    /// There is no such library here, and `add_raw_font` is the path Servo
    /// actually uses on this target.
    pub fn add_native_font(&mut self, _font_key: &FontKey, _handle: NativeFontHandle) {}

    pub fn delete_font(&mut self, font_key: &FontKey) {
        self.fonts.remove(font_key);
    }

    /// Instances hold no per-instance state here: size is applied per glyph.
    pub fn delete_font_instance(&mut self, _instance: &FontInstance) {}

    pub fn get_glyph_index(&mut self, font_key: FontKey, ch: char) -> Option<u32> {
        let font = self.fonts.get(&font_key)?;
        let glyph = font.glyph_id(ch);
        (glyph.0 != 0).then_some(glyph.0 as u32)
    }

    pub fn get_glyph_dimensions(
        &mut self,
        font: &FontInstance,
        key: &GlyphKey,
    ) -> Option<GlyphDimensions> {
        let face = self.fonts.get(&font.font_key)?;
        let size = font.size.to_f32_px();
        let advance = {
            use ab_glyph::ScaleFont;
            face.as_scaled(size)
                .h_advance(AbGlyphId(key.index() as u16))
        };

        // A glyph with no outline — a space — still has an advance, and layout
        // needs it, so report zero extent rather than nothing.
        let Some(outlined) = outline(face, key, size) else {
            return Some(GlyphDimensions {
                left: 0,
                top: 0,
                width: 0,
                height: 0,
                advance,
            });
        };
        let bounds = outlined.px_bounds();
        Some(GlyphDimensions {
            left: bounds.min.x as i32,
            top: -bounds.min.y as i32,
            width: (bounds.max.x - bounds.min.x) as i32,
            height: (bounds.max.y - bounds.min.y) as i32,
            advance,
        })
    }

    /// Subpixel positioning is not honoured, so do not let callers ask for it.
    pub fn prepare_font(font: &mut FontInstance) {
        font.flags &= !FontInstanceFlags::SUBPIXEL_POSITION;
    }

    pub fn begin_rasterize(_font: &FontInstance) {}

    pub fn end_rasterize(_font: &FontInstance) {}

    pub fn rasterize_glyph(&mut self, font: &FontInstance, key: &GlyphKey) -> GlyphRasterResult {
        let face = self
            .fonts
            .get(&font.font_key)
            .ok_or(GlyphRasterError::LoadFailed)?;
        let size = font.size.to_f32_px();
        let outlined = outline(face, key, size).ok_or(GlyphRasterError::LoadFailed)?;

        let bounds = outlined.px_bounds();
        let width = (bounds.max.x - bounds.min.x) as i32;
        let height = (bounds.max.y - bounds.min.y) as i32;
        if width <= 0 || height <= 0 {
            // A blank glyph. `LoadFailed` is how the other backends report
            // "nothing to draw"; the caller treats it as an empty glyph.
            return Err(GlyphRasterError::LoadFailed);
        }

        // WebRender uploads glyphs as four bytes per pixel whatever the format;
        // for an alpha mask the coverage goes in every channel.
        let mut bytes = vec![0u8; (width * height * 4) as usize];
        outlined.draw(|x, y, coverage| {
            if x >= width as u32 || y >= height as u32 {
                return;
            }
            let value = (coverage * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
            let offset = ((y * width as u32 + x) * 4) as usize;
            bytes[offset..offset + 4].copy_from_slice(&[value; 4]);
        });

        Ok(RasterizedGlyph {
            left: bounds.min.x,
            // `top` is measured up from the baseline; ab_glyph's y grows down.
            top: -bounds.min.y,
            width,
            height,
            scale: 1.0,
            format: font.get_alpha_glyph_format(),
            bytes,
            is_packed_glyph: false,
        })
    }
}

/// Outline one glyph of `face` at `size` pixels, positioned at the origin.
fn outline(face: &FontVec, key: &GlyphKey, size: f32) -> Option<ab_glyph::OutlinedGlyph> {
    let glyph = AbGlyphId(key.index() as u16).with_scale_and_position(size, point(0.0, 0.0));
    face.outline_glyph(glyph)
}

impl Default for FontContext {
    fn default() -> Self {
        Self::new()
    }
}
