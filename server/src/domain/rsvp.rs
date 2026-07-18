//! Interpreting what a guest meant.
//!
//! An RSVP arrives one of two ways. The public link is unambiguous: the guest
//! taps a button. A WhatsApp or SMS reply is not — it's free text a person
//! thumbed on a phone, in English, Nigerian Pidgin, or a mix, maybe just "1".
//! This module turns that text into an intent, and is deliberately pure and
//! exhaustively tested: it is the part of the RSVP flow most likely to be
//! wrong, and a misread reply silently miscounts the headcount an organizer
//! caters to.

/// What a guest's reply resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    /// "1", "yes", "I'll be there", "abeg count me in".
    Confirm,
    /// "2", "no", "can't make it", "I no fit come".
    Decline,
    /// Anything we can't be sure about. Never guessed — an unclear reply is
    /// surfaced to the organizer, because acting on a wrong guess is worse than
    /// admitting we didn't understand.
    Unclear,
}

/// The instruction guests are given: "reply 1 to confirm, 2 to decline". The
/// numeric answer is the common case and must win cleanly.
fn numeric_reply(text: &str) -> Option<Reply> {
    match text {
        "1" => Some(Reply::Confirm),
        "2" => Some(Reply::Decline),
        _ => None,
    }
}

/// Whole-word affirmatives. Matched against tokens, never as substrings, so
/// "yes" doesn't fire inside "yesterday" and "no" doesn't fire inside "nobody".
const YES_WORDS: &[&str] = &[
    "yes", "yeah", "yep", "yup", "y", "sure", "ok", "okay", "confirm", "confirmed",
    "coming", "attend", "attending", "accept", "in", // Pidgin / colloquial:
    "abeg", "na", "dey", "go",
];

const NO_WORDS: &[&str] = &[
    "no", "nope", "nah", "n", "decline", "declined", "cant", "cannot", "wont",
    "unable", "sorry", "regret", "regrets", "absent", "out",
];

/// Phrases whose meaning flips a word inside them, checked before the word
/// scan. Kept specific on purpose: a bare "can't" would swallow "can't wait"
/// (enthusiasm, not a decline), so we match the actual decline constructions.
const DECLINE_PHRASES: &[&str] = &[
    "can't make", "cant make", "can not make", "cannot make",
    "can't come", "cant come", "cannot come",
    "can't attend", "cant attend", "cannot attend",
    "can't be", "cant be", "won't be", "wont be", "will not be",
    "won't make", "wont make", "won't come", "wont come",
    "not coming", "not attending", "not able", "not gonna",
    "no fit", "i no go", "no go fit", "unable",
];

const CONFIRM_PHRASES: &[&str] = &[
    "can't wait", "cant wait", // enthusiasm, the one "can't" that means yes
    "i'll be there", "ill be there", "will be there", "see you there",
    "count me in", "i dey come", "i go come",
];

/// Splits into lowercase word tokens, dropping punctuation. Keeps the digits so
/// a bare "1" survives, and an apostrophe inside a word ("can't") so phrase
/// matching can find it before this runs.
fn tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Interprets a free-text reply. See [`Reply`].
pub fn interpret(text: &str) -> Reply {
    let trimmed = text.trim();

    // A bare number is the instructed answer; honour it and nothing else, so
    // "1" is always confirm even though the sentence scan below might waver.
    if let Some(reply) = numeric_reply(trimmed) {
        return reply;
    }

    let lower = trimmed.to_lowercase();

    // Negation phrases first: they contain words that would otherwise read as
    // the opposite intent.
    let has_decline_phrase = DECLINE_PHRASES.iter().any(|p| lower.contains(p));
    let has_confirm_phrase = CONFIRM_PHRASES.iter().any(|p| lower.contains(p));
    match (has_confirm_phrase, has_decline_phrase) {
        (true, false) => return Reply::Confirm,
        (false, true) => return Reply::Decline,
        // Both or neither: fall through to the word scan, which may still break
        // the tie, and otherwise reports Unclear.
        _ => {}
    }

    let words = tokens(trimmed);
    let yes = words.iter().any(|w| YES_WORDS.contains(&w.as_str()));
    let no = words.iter().any(|w| NO_WORDS.contains(&w.as_str()));
    match (yes, no) {
        (true, false) => Reply::Confirm,
        (false, true) => Reply::Decline,
        // "yes and no", or nothing recognisable: don't guess.
        _ => Reply::Unclear,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_instructed_numbers_win() {
        assert_eq!(interpret("1"), Reply::Confirm);
        assert_eq!(interpret("2"), Reply::Decline);
        assert_eq!(interpret("  1  "), Reply::Confirm);
        // A number inside a sentence isn't the instructed answer.
        assert_eq!(interpret("1 of us"), Reply::Unclear);
    }

    #[test]
    fn plain_yes_and_no() {
        for yes in ["yes", "Yes", "YES", "yeah", "yep", "sure", "ok", "okay", "y"] {
            assert_eq!(interpret(yes), Reply::Confirm, "{yes:?}");
        }
        for no in ["no", "No", "nope", "nah", "n", "decline"] {
            assert_eq!(interpret(no), Reply::Decline, "{no:?}");
        }
    }

    #[test]
    fn sentences() {
        assert_eq!(interpret("Yes, I'll be there!"), Reply::Confirm);
        assert_eq!(interpret("count me in"), Reply::Confirm);
        assert_eq!(interpret("So sorry, can't make it"), Reply::Decline);
        assert_eq!(interpret("I won't be able to come"), Reply::Decline);
        assert_eq!(interpret("not coming"), Reply::Decline);
    }

    #[test]
    fn negation_is_not_read_as_the_opposite() {
        // Each contains an affirmative word but means the opposite.
        assert_eq!(interpret("not coming"), Reply::Decline);
        assert_eq!(interpret("can't attend"), Reply::Decline);
        assert_eq!(interpret("won't be there"), Reply::Decline);
    }

    #[test]
    fn cant_wait_is_enthusiasm_not_a_decline() {
        // The one "can't" that means yes. A coarse negation rule gets this
        // exactly backwards, so it's worth pinning.
        assert_eq!(interpret("can't wait!"), Reply::Confirm);
        assert_eq!(interpret("Can't wait, see you there"), Reply::Confirm);
    }

    #[test]
    fn pidgin() {
        assert_eq!(interpret("I dey come"), Reply::Confirm);
        assert_eq!(interpret("I go come"), Reply::Confirm);
        assert_eq!(interpret("I no fit come"), Reply::Decline);
        assert_eq!(interpret("I no go fit"), Reply::Decline);
    }

    #[test]
    fn substrings_do_not_trigger() {
        // "yesterday" contains "yes"; "nobody" contains "no". Neither is an
        // answer, and there's no other signal, so both are unclear.
        assert_eq!(interpret("maybe yesterday"), Reply::Unclear);
        assert_eq!(interpret("nobody told me"), Reply::Unclear);
    }

    #[test]
    fn genuinely_ambiguous_is_never_guessed() {
        assert_eq!(interpret(""), Reply::Unclear);
        assert_eq!(interpret("hmm"), Reply::Unclear);
        assert_eq!(interpret("what's the address?"), Reply::Unclear);
        assert_eq!(interpret("maybe"), Reply::Unclear);
        // Contradictory: contains both a yes-word and a no-word.
        assert_eq!(interpret("yes and no"), Reply::Unclear);
    }
}
