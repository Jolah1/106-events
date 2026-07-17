//! Phone numbers, normalized to E.164.
//!
//! Guest lists arrive as spreadsheets typed by humans: `0806 688 2563`,
//! `+234-806-688-2563`, `234 8066882563`. Inbound WhatsApp and SMS webhooks,
//! meanwhile, identify a sender as `+2348066882563` and nothing else. Matching
//! one to the other is only possible if what we store is canonical, so
//! normalization happens once, here, at the point of entry.

/// Nigeria. Bare national numbers are interpreted against this country.
const NG_CODE: &str = "234";

/// Nigerian mobile national significant numbers are 10 digits and begin with
/// 7, 8 or 9 (070/080/081/090/091… in local dialling form).
fn is_ng_mobile_nsn(nsn: &str) -> bool {
    nsn.len() == 10 && matches!(nsn.as_bytes()[0], b'7' | b'8' | b'9')
}

/// Extracts a Nigerian mobile national number from a run of digits, in any of
/// the forms people write: with the country code, with the trunk `0`, with
/// both (`+234 (0) 806…`, a common convention), or bare.
fn ng_mobile_nsn(digits: &str) -> Option<&str> {
    let national = digits.strip_prefix(NG_CODE).unwrap_or(digits);
    let national = national.strip_prefix('0').unwrap_or(national);
    is_ng_mobile_nsn(national).then_some(national)
}

/// Normalizes a phone number to E.164 (`+2348066882563`).
///
/// Bare national numbers are read as Nigerian, since that is who the product
/// is for. Anything written with an explicit `+` is kept as dialled, so a
/// guest flying in from Accra or London still imports cleanly — we check the
/// shape but don't pretend to know every country's numbering plan.
///
/// Returns `None` for input that isn't a phone number we're confident about.
/// Guessing here would mean silently texting a stranger.
pub fn normalize(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    // "00" is the international access prefix in Nigeria and much of the
    // world; it means the same thing as a leading "+".
    let (explicit_intl, rest) = match trimmed.strip_prefix('+') {
        Some(rest) => (true, rest),
        None => match trimmed.strip_prefix("00") {
            Some(rest) => (true, rest),
            None => (false, trimmed),
        },
    };

    // Spreadsheets carry spaces, hyphens, dots and (0) inside numbers; none of
    // it is dialling information.
    let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || rest.chars().any(|c| !is_ignorable(c)) {
        return None;
    }

    // Nigerian first, and regardless of the `+`: it is the one numbering plan
    // we know well enough to repair a trunk `0` or a missing country code.
    if let Some(nsn) = ng_mobile_nsn(&digits) {
        return Some(format!("+{NG_CODE}{nsn}"));
    }

    if explicit_intl {
        // E.164 allows 15 digits; a country code plus a subscriber number is
        // never shorter than 8 in practice.
        if !(8..=15).contains(&digits.len()) {
            return None;
        }
        return Some(format!("+{digits}"));
    }

    None
}

/// Characters that carry no dialling information and may appear between digits.
fn is_ignorable(c: char) -> bool {
    c.is_ascii_digit() || matches!(c, ' ' | '-' | '.' | '(' | ')' | '\u{a0}' | '\u{2013}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_nigerian_numbers_however_they_are_written() {
        for input in [
            "08066882563",
            "0806 688 2563",
            "0806-688-2563",
            "+2348066882563",
            "+234 806 688 2563",
            "2348066882563",
            "234 806 688 2563",
            "8066882563",
            "+234 (0) 806 688 2563",
            "+234 0806 688 2563",
            "  08066882563  ",
            "002348066882563",
        ] {
            assert_eq!(
                normalize(input).as_deref(),
                Some("+2348066882563"),
                "{input:?}"
            );
        }
    }

    #[test]
    fn accepts_every_nigerian_mobile_prefix() {
        for input in ["07011111111", "08111111111", "09011111111", "09111111111"] {
            assert!(normalize(input).is_some(), "{input:?}");
        }
    }

    #[test]
    fn keeps_foreign_numbers_as_dialled() {
        // A Ghanaian and a UK guest, written internationally.
        assert_eq!(normalize("+233 24 123 4567").as_deref(), Some("+233241234567"));
        assert_eq!(normalize("+44 7700 900123").as_deref(), Some("+447700900123"));
    }

    #[test]
    fn rejects_numbers_it_cannot_place() {
        for input in [
            "",
            "   ",
            "not a phone",
            "0806688256",     // 10 digits with trunk 0: one short
            "080668825634",   // one long
            "01234567",       // a Lagos landline, unusable for SMS
            "+1",             // too short to be dialled
            "+1234567890123456", // beyond E.164's 15 digits
            "0106688256",     // national, but no mobile starts with 1
            "0806688256a",
            "+234806688256$",
        ] {
            assert_eq!(normalize(input), None, "{input:?} should be rejected");
        }
    }

    #[test]
    fn a_normalized_number_is_stable() {
        let once = normalize("0806 688 2563").unwrap();
        assert_eq!(normalize(&once).as_deref(), Some(once.as_str()));
    }
}
