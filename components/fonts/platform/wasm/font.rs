/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Platform font backend for `wasm32-wasip2`.
//!
//! Servo's other font backends wrap a native library — FreeType, CoreText or
//! DirectWrite — and a wasm guest has none of them. This one is built on
//! `skrifa` and `read_fonts`, which are pure Rust and already in Servo's
//! dependency tree (the FreeType backend uses them for table access), so it
//! needs nothing from the host.
//!
//! It answers the two questions layout asks — which glyph, and how wide — plus
//! the face metrics that give a line its height. Drawing those glyphs is a
//! separate backend: see `wr_glyph_rasterizer`'s wasm platform module.

use app_units::Au;
use euclid::default::Rect;
use fonts_traits::{FontIdentifier, FontTemplateDescriptor, LocalFontIdentifier};
use read_fonts::TableProvider;
use read_fonts::types::Tag;
use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};
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

/// A font face.
///
/// Holds the face bytes and re-parses the table directory per query rather than
/// keeping a borrowed `FontRef` alongside them. Parsing is a bounds-checked walk
/// of a small directory, and Servo caches advances and glyph indices a level up
/// in `CachedShapeData`, so the repeated work does not reach shaping.
#[derive(Debug)]
pub struct PlatformFont {
    data: Vec<u8>,
    face_index: u32,
    /// The size this face was instantiated at. `None` means unscaled, in which
    /// case metrics come back in font units.
    pt_size: Option<Au>,
    variations: Vec<FontVariation>,
}

impl PlatformFont {
    fn font_ref(&self) -> Option<FontRef<'_>> {
        FontRef::from_index(&self.data, self.face_index).ok()
    }

    /// The size to scale metrics to, or unscaled when none was requested.
    fn size(&self) -> Size {
        match self.pt_size {
            Some(size) => Size::new(size.to_f32_px()),
            None => Size::unscaled(),
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
        let data_and_index = font_identifier
            .font_data_and_index()
            .ok_or("Could not read font face")?;
        Ok(PlatformFont {
            data: data_and_index.data.as_ref().to_vec(),
            face_index: data_and_index.index,
            pt_size,
            variations: variations.to_vec(),
        })
    }

    fn new_from_data(
        font_identifier: FontIdentifier,
        data: &FontData,
        pt_size: Option<Au>,
        variations: &[FontVariation],
        _synthetic_bold: bool,
    ) -> Result<PlatformFont, &'static str> {
        Ok(PlatformFont {
            data: data.as_ref().to_vec(),
            face_index: font_identifier.index(),
            pt_size,
            variations: variations.to_vec(),
        })
    }

    fn descriptor(&self) -> FontTemplateDescriptor {
        FontTemplateDescriptor::default()
    }

    fn glyph_index(&self, codepoint: char) -> Option<GlyphId> {
        let font = self.font_ref()?;
        let glyph = font.charmap().map(codepoint)?;
        Some(glyph.to_u32())
    }

    fn glyph_h_advance(&self, glyph: GlyphId) -> Option<FractionalPixel> {
        let font = self.font_ref()?;
        font.glyph_metrics(self.size(), LocationRef::default())
            .advance_width(skrifa::GlyphId::new(glyph))
            .map(|advance| advance as FractionalPixel)
    }

    /// Kerning is applied during shaping through the `kern`/`GPOS` tables, which
    /// Servo reads itself via [`Self::table_for_tag`]. Nothing to add here.
    fn glyph_h_kerning(&self, _first: GlyphId, _second: GlyphId) -> FractionalPixel {
        0.0
    }

    fn metrics(&self) -> FontMetrics {
        let Some(font) = self.font_ref() else {
            return FontMetrics::default();
        };
        let size = self.size();
        let metrics = font.metrics(size, LocationRef::default());
        let glyph_metrics = font.glyph_metrics(size, LocationRef::default());

        let px = |value: f32| Au::from_f32_px(value);
        let advance_of = |codepoint: char| {
            font.charmap()
                .map(codepoint)
                .and_then(|glyph| glyph_metrics.advance_width(glyph))
        };

        // `descent` is negative in font conventions and positive in Servo's.
        let ascent = metrics.ascent;
        let descent = -metrics.descent;
        let em_size = self.pt_size.map(|size| size.to_f32_px()).unwrap_or_else(|| {
            metrics.units_per_em as f32
        });
        let average_advance = advance_of('0').unwrap_or(em_size / 2.0);
        let space_advance = advance_of(' ').unwrap_or(average_advance);
        let underline_size = metrics.underline.map(|u| u.thickness).unwrap_or(em_size / 14.0);
        let underline_offset = metrics.underline.map(|u| u.offset).unwrap_or(-em_size / 7.0);
        let strikeout = metrics.strikeout;

        FontMetrics {
            underline_size: px(underline_size),
            underline_offset: px(underline_offset),
            strikeout_size: px(strikeout.map(|s| s.thickness).unwrap_or(underline_size)),
            strikeout_offset: px(strikeout.map(|s| s.offset).unwrap_or(em_size / 3.0)),
            leading: px(metrics.leading),
            x_height: px(metrics.x_height.unwrap_or(em_size / 2.0)),
            em_size: px(em_size),
            ascent: px(ascent),
            descent: px(descent),
            max_advance: px(metrics.max_width.unwrap_or(em_size)),
            average_advance: px(average_advance),
            line_gap: px(ascent + descent + metrics.leading),
            zero_horizontal_advance: advance_of('0').map(px),
            ic_horizontal_advance: advance_of('\u{6C34}').map(px),
            space_advance: px(space_advance),
        }
    }

    fn table_for_tag(&self, tag: Tag) -> Option<FontTable> {
        let font = self.font_ref()?;
        let data = font.table_data(tag)?;
        Some(FontTable {
            buffer: data.as_bytes().to_vec(),
        })
    }

    fn typographic_bounds(&self, glyph_id: GlyphId) -> Rect<f32> {
        let Some(font) = self.font_ref() else {
            return Rect::zero();
        };
        let bounds = font
            .glyph_metrics(self.size(), LocationRef::default())
            .bounds(skrifa::GlyphId::new(glyph_id));
        match bounds {
            Some(bounds) => Rect::new(
                euclid::default::Point2D::new(bounds.x_min, bounds.y_min),
                euclid::default::Size2D::new(
                    bounds.x_max - bounds.x_min,
                    bounds.y_max - bounds.y_min,
                ),
            ),
            None => Rect::zero(),
        }
    }

    fn webrender_font_instance_flags(&self) -> FontInstanceFlags {
        FontInstanceFlags::empty()
    }

    fn variations(&self) -> &[FontVariation] {
        &self.variations
    }
}
