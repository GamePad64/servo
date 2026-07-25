/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Platform font backend for `wasm32-wasip2`.
//!
//! Servo's font backends wrap a native library — FreeType, CoreText or
//! DirectWrite — none of which exists in a wasm guest. This backend holds the
//! font bytes and answers metric queries neutrally.
//!
//! **This is deliberately not a real implementation yet.** It exists to reach the
//! G0 gate ("libservo compiles for wasm32-wasip2") and to support G1 ("boot and
//! dump a DOM"), neither of which measures or shapes text. Text layout against
//! this backend will produce zero advances.
//!
//! The natural follow-up is a real backend over `read_fonts` + `skrifa`, which
//! are pure Rust and **already in Servo's dependency tree** — the FreeType
//! backend uses them for `font_ref()` and table access. That would supply glyph
//! indices, advances and metrics without any native library, and is the intended
//! path before layout fidelity matters.

use app_units::Au;
use euclid::default::Rect;
use fonts_traits::{FontIdentifier, FontTemplateDescriptor, LocalFontIdentifier};
use read_fonts::types::Tag;
use webrender_api::{FontInstanceFlags, FontVariation};

use crate::FontData;
use crate::font::{FontMetrics, FontTableMethods, FractionalPixel, PlatformFontMethods};
use crate::glyph::GlyphId;

/// A table read out of a font file.
#[derive(Debug)]
pub struct FontTable {
    buffer: Vec<u8>,
}

impl FontTableMethods for FontTable {
    fn buffer(&self) -> &[u8] {
        &self.buffer
    }
}

/// A font face. Holds its bytes but does not yet interpret them.
#[derive(Debug)]
pub struct PlatformFont {
    /// The face data, kept so a future `read_fonts`-backed implementation has it
    /// available without changing how fonts are loaded.
    _data: Option<Vec<u8>>,
    /// The size this face was instantiated at, if any.
    _pt_size: Option<Au>,
    variations: Vec<FontVariation>,
}

impl PlatformFont {
    fn empty(pt_size: Option<Au>, variations: &[FontVariation]) -> PlatformFont {
        PlatformFont {
            _data: None,
            _pt_size: pt_size,
            variations: variations.to_vec(),
        }
    }
}

impl PlatformFontMethods for PlatformFont {
    fn new_from_local_font_identifier(
        font_identifier: LocalFontIdentifier,
        pt_size: Option<Au>,
        variations: &[FontVariation],
        _synthetic_bold: bool,
    ) -> Result<PlatformFont, &'static str> {
        // WASI has a filesystem, so a local face can still be read — from a
        // preopened directory. It resolves to nothing when the path is not
        // reachable, which is the common case in a sandboxed component.
        let data = font_identifier
            .font_data_and_index()
            .map(|data_and_index| data_and_index.data.as_ref().to_vec());
        Ok(PlatformFont {
            _data: data,
            _pt_size: pt_size,
            variations: variations.to_vec(),
        })
    }

    fn new_from_data(
        _font_identifier: FontIdentifier,
        data: &FontData,
        pt_size: Option<Au>,
        variations: &[FontVariation],
        _synthetic_bold: bool,
    ) -> Result<PlatformFont, &'static str> {
        Ok(PlatformFont {
            _data: Some(data.as_ref().to_vec()),
            _pt_size: pt_size,
            variations: variations.to_vec(),
        })
    }

    fn descriptor(&self) -> FontTemplateDescriptor {
        FontTemplateDescriptor::default()
    }

    /// No glyph is ever resolved: the face bytes are not parsed yet.
    fn glyph_index(&self, _codepoint: char) -> Option<GlyphId> {
        None
    }

    fn glyph_h_advance(&self, _glyph: GlyphId) -> Option<FractionalPixel> {
        None
    }

    fn glyph_h_kerning(&self, _first: GlyphId, _second: GlyphId) -> FractionalPixel {
        0.0
    }

    fn metrics(&self) -> FontMetrics {
        FontMetrics::default()
    }

    fn table_for_tag(&self, _tag: Tag) -> Option<FontTable> {
        None
    }

    fn typographic_bounds(&self, _glyph_id: GlyphId) -> Rect<f32> {
        Rect::zero()
    }

    fn webrender_font_instance_flags(&self) -> FontInstanceFlags {
        FontInstanceFlags::empty()
    }

    fn variations(&self) -> &[FontVariation] {
        &self.variations
    }
}

// Keep the constructor referenced so the helper does not warn when unused.
const _: fn() = || {
    let _ = PlatformFont::empty(None, &[]);
};
