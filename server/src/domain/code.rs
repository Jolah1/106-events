//! Short attendee codes: what the QR encodes, and what a human reads aloud
//! when a guest's phone is dead or the screenshot won't scan.
//!
//! Because these get read over the noise of a live event and typed by someone
//! standing at a door, the alphabet drops every character pair that gets
//! confused out loud or on screen: no O/0, no I/1/L, no S/5, no B/8, no U/V.
//! What's left is unambiguous spoken and written.

/// 23 characters: A-Z and 2-9, minus the confusable ones.
const ALPHABET: &str = "ACDEFGHJKMNPQRTWXY34679";

/// Codes are 8 characters, drawn uniformly from a 23-character alphabet:
/// roughly 23^8 ≈ 7.8e10 possibilities. At a few thousand attendees the
/// collision probability is negligible, and the unique index catches the rest.
const CODE_LEN: usize = 8;

pub fn generate() -> String {
    let alphabet: Vec<char> = ALPHABET.chars().collect();
    let mut bytes = [0u8; CODE_LEN];
    getrandom::fill(&mut bytes).expect("os rng unavailable");
    // Rejection-free modulo bias is irrelevant here: the alphabet is 23 and
    // the byte range is 256, so the bias is under 0.5% per character and has
    // no security consequence for a code that is also checked against the DB.
    bytes
        .iter()
        .map(|b| alphabet[*b as usize % alphabet.len()])
        .collect()
}

/// Normalizes what someone typed or scanned: uppercases, and drops the spaces
/// and dashes staff naturally add when grouping characters.
pub fn normalize(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// Whether a normalized code could plausibly be one of ours. Cheap guard so a
/// scan of some unrelated QR (a Wi-Fi code, a product barcode) is rejected
/// before it reaches the database.
pub fn is_plausible(code: &str) -> bool {
    code.len() == CODE_LEN && code.chars().all(|c| ALPHABET.contains(c))
}

/// Renders a code as an SVG QR square.
///
/// SVG rather than a raster: it stays sharp on a phone screen at any size and
/// when printed onto a paper invitation, and it's a few hundred bytes over the
/// kind of connection a guest is opening this on.
///
/// Colours are fixed black-on-white regardless of the brand, because scanners
/// need the contrast and a gold QR code is a QR code that doesn't scan.
pub fn qr_svg(code: &str) -> String {
    use qrcode::{EcLevel, QrCode, render::svg};

    // High error correction: these get screenshotted, cropped, printed, and
    // photographed off other screens. Eight characters is tiny either way.
    let qr = QrCode::with_error_correction_level(code, EcLevel::H)
        .expect("eight characters always fit in a QR code");
    qr.render()
        .min_dimensions(240, 240)
        .quiet_zone(true)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn codes_avoid_characters_that_get_misread() {
        // The whole point of the alphabet: none of these may appear.
        for banned in ['O', '0', 'I', '1', 'L', 'S', '5', 'B', '8', 'U', 'V', '2'] {
            assert!(
                !ALPHABET.contains(banned),
                "{banned} is confusable and must not be in the alphabet"
            );
        }
    }

    #[test]
    fn generated_codes_are_the_right_shape() {
        for _ in 0..100 {
            let code = generate();
            assert!(is_plausible(&code), "{code} should be a valid code");
        }
    }

    #[test]
    fn codes_do_not_obviously_collide() {
        // Not a statistical proof — a smoke test that the generator isn't
        // returning a constant, which would silently make every guest the
        // same person at the door.
        let codes: HashSet<String> = (0..1000).map(|_| generate()).collect();
        assert_eq!(codes.len(), 1000, "1000 codes should all be distinct");
    }

    #[test]
    fn staff_typing_is_forgiving_about_case_and_spacing() {
        assert_eq!(normalize("achd-4f7k"), "ACHD4F7K");
        assert_eq!(normalize("ACHD 4F7K"), "ACHD4F7K");
        assert_eq!(normalize("  achd4f7k  "), "ACHD4F7K");
    }

    #[test]
    fn a_code_renders_as_a_scannable_square() {
        let svg = qr_svg("ACHD4F7K");
        assert!(svg.starts_with("<?xml"), "an SVG document");
        assert!(svg.contains("#000000"), "black on white, whatever the brand is");
        // A QR of eight characters at level H is 25x25 modules or larger; the
        // rendered document must be at least the minimum we asked for.
        assert!(svg.contains("width=\"2"), "at least 240px wide: {}", &svg[..200]);
    }

    #[test]
    fn a_foreign_qr_code_is_rejected_before_the_database() {
        assert!(!is_plausible("WIFI:S:MyNetwork"), "a Wi-Fi QR");
        assert!(!is_plausible("ACHD4F7"), "too short");
        assert!(!is_plausible("ACHD4F7KK"), "too long");
        assert!(!is_plausible("ACHD4F7O"), "contains a banned character");
        assert!(!is_plausible(""), "empty");
    }
}
