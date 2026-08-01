/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! System font enumeration for `wasm32-wasip2`.
//!
//! There is no system font database inside a wasm guest: no fontconfig, no
//! CoreText collection, no DirectWrite. The families below are the faces
//! compiled into the component itself (see [`BUILTIN_FONTS`]); without them a
//! page of text has nothing to shape against and renders blank.
//!
//! Fonts can still arrive by the other routes a wasm build supports — a web
//! font, or a face read from a preopened directory. Those go through
//! `FontIdentifier::Web`/`ArrayBuffer` and never consult this module.

use fonts_traits::{
    FontIdentifier, FontTemplate, FontTemplateDescriptor, LocalFontIdentifier, builtin_fonts,
};
use style::Atom;
use style::values::computed::font::GenericFontFamily;

use crate::{FallbackFontSelectionOptions, LowercaseFontFamilyName};

/// Enumerate installed font families: the ones carried by the component.
pub(crate) fn for_each_available_family<F>(mut callback: F)
where
    F: FnMut(String),
{
    for (family, _, _) in builtin_fonts() {
        callback((*family).to_owned());
    }
}

/// Enumerate the faces of a family. Each built-in family has exactly one face.
pub(crate) fn for_each_variation<F>(family_name: &str, mut callback: F)
where
    F: FnMut(FontTemplate),
{
    for (family, path, _) in builtin_fonts() {
        if !family.eq_ignore_ascii_case(family_name) {
            continue;
        }
        callback(FontTemplate::new(
            FontIdentifier::Local(LocalFontIdentifier {
                path: Atom::from(*path),
                face_index: 0,
                named_instance_index: 0,
            }),
            FontTemplateDescriptor::default(),
            None,
        ));
    }
}

/// Fallback families to try when a character has no glyph in the requested font.
pub fn fallback_font_families(_options: FallbackFontSelectionOptions) -> Vec<&'static str> {
    vec!["DejaVu Sans"]
}

/// The family name backing each CSS generic, mapped onto what is built in.
pub(crate) fn default_system_generic_font_family(
    generic: GenericFontFamily,
) -> LowercaseFontFamilyName {
    match generic {
        GenericFontFamily::Serif => "DejaVu Serif",
        GenericFontFamily::Monospace => "DejaVu Sans Mono",
        GenericFontFamily::None |
        GenericFontFamily::SansSerif |
        GenericFontFamily::Cursive |
        GenericFontFamily::Fantasy |
        GenericFontFamily::SystemUi => "DejaVu Sans",
    }
    .into()
}
