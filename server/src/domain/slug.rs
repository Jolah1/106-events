/// Folds a Latin letter with diacritics down to its ASCII base, so that names
/// in the languages this product actually serves survive the trip into a URL:
/// Yoruba (Ọláṣubọmi → olasubomi), Igbo (Chinụa → chinua), Hausa (Ɓala → bala),
/// alongside the European accents.
///
/// Generated from Unicode NFD decompositions of Latin-1 Supplement, Latin
/// Extended-A/B and Latin Extended Additional, plus the non-decomposing hooked
/// consonants and ligatures. Scripts with no Latin base (Arabic, Han, …) are
/// not transliterated: those titles fall back to a random slug, which beats a
/// wrong guess at romanisation.
fn fold_to_ascii(ch: char) -> Option<&'static str> {
    Some(match ch {
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' | 'Ă' | 'Ą' | 'Ǎ' | 'Ǟ' | 'Ǡ' | 'Ǻ' | 'Ȁ' | 'Ȃ' | 'Ȧ' | 'Ḁ' | 'Ạ' | 'Ả' | 'Ấ' | 'Ầ' | 'Ẩ' | 'Ẫ' | 'Ậ' | 'Ắ' | 'Ằ' | 'Ẳ' | 'Ẵ' | 'Ặ' => "A",
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' | 'ǎ' | 'ǟ' | 'ǡ' | 'ǻ' | 'ȁ' | 'ȃ' | 'ȧ' | 'ḁ' | 'ạ' | 'ả' | 'ấ' | 'ầ' | 'ẩ' | 'ẫ' | 'ậ' | 'ắ' | 'ằ' | 'ẳ' | 'ẵ' | 'ặ' => "a",
        'Æ' => "AE",
        'æ' => "ae",
        'Ɓ' | 'Ḃ' | 'Ḅ' | 'Ḇ' => "B",
        'ɓ' | 'ḃ' | 'ḅ' | 'ḇ' => "b",
        'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' | 'Ḉ' => "C",
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' | 'ḉ' => "c",
        'Ð' | 'Ď' | 'Đ' | 'Ɖ' | 'Ɗ' | 'Ḋ' | 'Ḍ' | 'Ḏ' | 'Ḑ' | 'Ḓ' => "D",
        'ð' | 'ď' | 'đ' | 'ɖ' | 'ɗ' | 'ḋ' | 'ḍ' | 'ḏ' | 'ḑ' | 'ḓ' => "d",
        'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' | 'Ȅ' | 'Ȇ' | 'Ȩ' | 'Ḕ' | 'Ḗ' | 'Ḙ' | 'Ḛ' | 'Ḝ' | 'Ẹ' | 'Ẻ' | 'Ẽ' | 'Ế' | 'Ề' | 'Ể' | 'Ễ' | 'Ệ' => "E",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' | 'ǝ' | 'ȅ' | 'ȇ' | 'ȩ' | 'ə' | 'ḕ' | 'ḗ' | 'ḙ' | 'ḛ' | 'ḝ' | 'ẹ' | 'ẻ' | 'ẽ' | 'ế' | 'ề' | 'ể' | 'ễ' | 'ệ' => "e",
        'Ƒ' | 'Ḟ' => "F",
        'ƒ' | 'ḟ' => "f",
        'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' | 'Ǧ' | 'Ǵ' | 'Ḡ' => "G",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' | 'ǧ' | 'ǵ' | 'ḡ' => "g",
        'Ĥ' | 'Ħ' | 'Ȟ' | 'Ḣ' | 'Ḥ' | 'Ḧ' | 'Ḩ' | 'Ḫ' => "H",
        'ĥ' | 'ħ' | 'ȟ' | 'ḣ' | 'ḥ' | 'ḧ' | 'ḩ' | 'ḫ' | 'ẖ' => "h",
        'Ì' | 'Í' | 'Î' | 'Ï' | 'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'İ' | 'Ǐ' | 'Ȉ' | 'Ȋ' | 'Ḭ' | 'Ḯ' | 'Ỉ' | 'Ị' => "I",
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' | 'ǐ' | 'ȉ' | 'ȋ' | 'ḭ' | 'ḯ' | 'ỉ' | 'ị' => "i",
        'Ĵ' => "J",
        'ĵ' | 'ǰ' | 'ȷ' => "j",
        'Ķ' | 'Ƙ' | 'Ǩ' | 'Ḱ' | 'Ḳ' | 'Ḵ' => "K",
        'ķ' | 'ƙ' | 'ǩ' | 'ḱ' | 'ḳ' | 'ḵ' => "k",
        'Ĺ' | 'Ļ' | 'Ľ' | 'Ł' | 'Ḷ' | 'Ḹ' | 'Ḻ' | 'Ḽ' => "L",
        'ĺ' | 'ļ' | 'ľ' | 'ł' | 'ḷ' | 'ḹ' | 'ḻ' | 'ḽ' => "l",
        'Ḿ' | 'Ṁ' | 'Ṃ' => "M",
        'ḿ' | 'ṁ' | 'ṃ' => "m",
        'Ñ' | 'Ń' | 'Ņ' | 'Ň' | 'Ŋ' | 'Ǹ' | 'Ṅ' | 'Ṇ' | 'Ṉ' | 'Ṋ' => "N",
        'ñ' | 'ń' | 'ņ' | 'ň' | 'ŋ' | 'ǹ' | 'ṅ' | 'ṇ' | 'ṉ' | 'ṋ' => "n",
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' | 'Ơ' | 'Ǒ' | 'Ǫ' | 'Ǭ' | 'Ȍ' | 'Ȏ' | 'Ȫ' | 'Ȭ' | 'Ȯ' | 'Ȱ' | 'Ṍ' | 'Ṏ' | 'Ṑ' | 'Ṓ' | 'Ọ' | 'Ỏ' | 'Ố' | 'Ồ' | 'Ổ' | 'Ỗ' | 'Ộ' | 'Ớ' | 'Ờ' | 'Ở' | 'Ỡ' | 'Ợ' => "O",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' | 'ơ' | 'ǒ' | 'ǫ' | 'ǭ' | 'ȍ' | 'ȏ' | 'ȫ' | 'ȭ' | 'ȯ' | 'ȱ' | 'ṍ' | 'ṏ' | 'ṑ' | 'ṓ' | 'ọ' | 'ỏ' | 'ố' | 'ồ' | 'ổ' | 'ỗ' | 'ộ' | 'ớ' | 'ờ' | 'ở' | 'ỡ' | 'ợ' => "o",
        'Œ' => "OE",
        'œ' => "oe",
        'Ṕ' | 'Ṗ' => "P",
        'ṕ' | 'ṗ' => "p",
        'Ŕ' | 'Ŗ' | 'Ř' | 'Ȑ' | 'Ȓ' | 'Ṙ' | 'Ṛ' | 'Ṝ' | 'Ṟ' => "R",
        'ŕ' | 'ŗ' | 'ř' | 'ȑ' | 'ȓ' | 'ṙ' | 'ṛ' | 'ṝ' | 'ṟ' => "r",
        'Ś' | 'Ŝ' | 'Ş' | 'Š' | 'Ș' | 'Ṡ' | 'Ṣ' | 'Ṥ' | 'Ṧ' | 'Ṩ' => "S",
        'ś' | 'ŝ' | 'ş' | 'š' | 'ș' | 'ṡ' | 'ṣ' | 'ṥ' | 'ṧ' | 'ṩ' => "s",
        'ß' => "ss",
        'Ţ' | 'Ť' | 'Ŧ' | 'Ț' | 'Ṫ' | 'Ṭ' | 'Ṯ' | 'Ṱ' => "T",
        'ţ' | 'ť' | 'ŧ' | 'ț' | 'ṫ' | 'ṭ' | 'ṯ' | 'ṱ' | 'ẗ' => "t",
        'Þ' => "TH",
        'þ' => "th",
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' | 'Ư' | 'Ǔ' | 'Ǖ' | 'Ǘ' | 'Ǚ' | 'Ǜ' | 'Ȕ' | 'Ȗ' | 'Ṳ' | 'Ṵ' | 'Ṷ' | 'Ṹ' | 'Ṻ' | 'Ụ' | 'Ủ' | 'Ứ' | 'Ừ' | 'Ử' | 'Ữ' | 'Ự' => "U",
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' | 'ư' | 'ǔ' | 'ǖ' | 'ǘ' | 'ǚ' | 'ǜ' | 'ȕ' | 'ȗ' | 'ṳ' | 'ṵ' | 'ṷ' | 'ṹ' | 'ṻ' | 'ụ' | 'ủ' | 'ứ' | 'ừ' | 'ử' | 'ữ' | 'ự' => "u",
        'Ṽ' | 'Ṿ' => "V",
        'ṽ' | 'ṿ' => "v",
        'Ŵ' | 'Ẁ' | 'Ẃ' | 'Ẅ' | 'Ẇ' | 'Ẉ' => "W",
        'ŵ' | 'ẁ' | 'ẃ' | 'ẅ' | 'ẇ' | 'ẉ' | 'ẘ' => "w",
        'Ẋ' | 'Ẍ' => "X",
        'ẋ' | 'ẍ' => "x",
        'Ý' | 'Ŷ' | 'Ÿ' | 'Ƴ' | 'Ȳ' | 'Ẏ' | 'Ỳ' | 'Ỵ' | 'Ỷ' | 'Ỹ' => "Y",
        'ý' | 'ÿ' | 'ŷ' | 'ƴ' | 'ȳ' | 'ẏ' | 'ẙ' | 'ỳ' | 'ỵ' | 'ỷ' | 'ỹ' => "y",
        'Ź' | 'Ż' | 'Ž' | 'Ẑ' | 'Ẓ' | 'Ẕ' => "Z",
        'ź' | 'ż' | 'ž' | 'ẑ' | 'ẓ' | 'ẕ' => "z",
        // Combining marks left over from decomposed input: drop them, so
        // "Ọ" typed as O + U+0323 folds the same as the precomposed form.
        '\u{0300}'..='\u{036F}' | '\u{1AB0}'..='\u{1AFF}' | '\u{20D0}'..='\u{20F0}' => "",
        _ => return None,
    })
}

/// URL-slug from a human title: lowercase ASCII alphanumerics, hyphens between
/// word runs, trimmed. Accented Latin letters are folded to their ASCII base
/// (see `fold_to_ascii`); anything else is dropped, and callers fall back to a
/// random suffix when the result is empty or taken.
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_hyphen = true; // suppress leading hyphen
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_hyphen = false;
        } else if let Some(folded) = fold_to_ascii(ch) {
            // A fold can be empty (a stray combining mark): that is neither a
            // character nor a word break, so leave the hyphen state alone.
            for f in folded.chars() {
                out.push(f.to_ascii_lowercase());
                last_was_hyphen = false;
            }
        } else if !last_was_hyphen {
            out.push('-');
            last_was_hyphen = true;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    out.chars().take(60).collect::<String>()
        .trim_end_matches('-')
        .to_string()
}

/// Short random suffix for slug collisions. Lowercase base32-style alphabet,
/// unambiguous characters only.
pub fn random_suffix(len: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut bytes = vec![0u8; len];
    getrandom::fill(&mut bytes).expect("os rng unavailable");
    bytes
        .iter()
        .map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugifies_titles() {
        assert_eq!(slugify("Adaeze & Tunde's Wedding"), "adaeze-tunde-s-wedding");
        assert_eq!(slugify("  White   Wedding!  "), "white-wedding");
        assert_eq!(slugify("Reception 2026"), "reception-2026");
        assert_eq!(slugify("!!!"), "");
    }

    #[test]
    fn folds_nigerian_names_to_readable_slugs() {
        // Yoruba: dot-below vowels and tone marks.
        assert_eq!(slugify("Ọláṣubọmi & Ṣadé"), "olasubomi-sade");
        assert_eq!(slugify("Ìkẹjà Owambe"), "ikeja-owambe");
        // Igbo: dot-below i/u.
        assert_eq!(slugify("Chinụa na Ngọzị"), "chinua-na-ngozi");
        // Hausa: hooked consonants, which have no decomposition.
        assert_eq!(slugify("Ɓala da Ƙasim"), "bala-da-kasim");
    }

    #[test]
    fn folds_european_accents_and_ligatures() {
        assert_eq!(slugify("Café Crème"), "cafe-creme");
        assert_eq!(slugify("Straße"), "strasse");
        assert_eq!(slugify("Œuvre Æther"), "oeuvre-aether");
        assert_eq!(slugify("Łódź"), "lodz");
    }

    #[test]
    fn folds_decomposed_input_like_precomposed() {
        // "Ọlá" typed as O + combining dot below, a + combining acute: the
        // same slug as the precomposed spelling, not "ol" with marks dropped.
        assert_eq!(slugify("O\u{0323}la\u{0301}"), slugify("Ọlá"));
        assert_eq!(slugify("O\u{0323}la\u{0301}"), "ola");
    }

    #[test]
    fn untransliterable_scripts_fall_back_to_empty() {
        // No romanisation is better than a wrong one; callers then use a
        // random slug rather than inventing a spelling.
        assert_eq!(slugify("婚礼"), "");
        assert_eq!(slugify("حفل"), "");
        // Mixed input keeps the part that does romanise.
        assert_eq!(slugify("婚礼 Reception"), "reception");
    }

    #[test]
    fn suffix_has_requested_length() {
        assert_eq!(random_suffix(4).len(), 4);
        assert_ne!(random_suffix(8), random_suffix(8));
    }
}
