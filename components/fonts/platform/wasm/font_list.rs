/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! System font enumeration for `wasm32-wasip2`.
//!
//! There is no system font database inside a wasm guest: no fontconfig, no
//! CoreText collection, no DirectWrite. Enumeration therefore yields nothing and
//! the callbacks are simply never invoked — which is a truthful answer, not a
//! failure.
//!
//! Fonts still reach a wasm build, just by a different route: embedded in the
//! component, or read from a preopened directory as a web font. Those paths go
//! through `FontIdentifier::Web`/`ArrayBuffer` and never consult this module.

use fonts_traits::FontTemplate;
use style::values::computed::font::GenericFontFamily;

use crate::{FallbackFontSelectionOptions, LowercaseFontFamilyName};

/// Enumerate installed font families. There are none.
pub(crate) fn for_each_available_family<F>(_callback: F)
where
    F: FnMut(String),
{
}

/// Enumerate the faces of an installed family. There are none.
pub(crate) fn for_each_variation<F>(_family_name: &str, _callback: F)
where
    F: FnMut(FontTemplate),
{
}

/// Fallback families to try when a character has no glyph in the requested font.
///
/// Empty: naming families that are certain not to be installed would only add
/// failed lookups.
pub fn fallback_font_families(_options: FallbackFontSelectionOptions) -> Vec<&'static str> {
    Vec::new()
}

/// The family name backing each CSS generic.
///
/// Nothing is installed, so these are names rather than resolvable families.
/// They are returned anyway because callers use them as map keys and expect a
/// stable, distinct name per generic.
pub(crate) fn default_system_generic_font_family(
    generic: GenericFontFamily,
) -> LowercaseFontFamilyName {
    match generic {
        GenericFontFamily::None | GenericFontFamily::Serif => "serif",
        GenericFontFamily::SansSerif => "sans-serif",
        GenericFontFamily::Monospace => "monospace",
        GenericFontFamily::Cursive => "cursive",
        GenericFontFamily::Fantasy => "fantasy",
        GenericFontFamily::SystemUi => "system-ui",
    }
    .into()
}
