//! host-grammar — the shared rules for valid agentic-host names and numbers.
//!
//! Both the host-lint detector (the *checker*) and host-lifecycle (the
//! *generator*) depend on this crate, so that what the generator emits is
//! exactly what the checker accepts. The number is identity; the slug is
//! content; ordering lives in the index, never in the name.
//!
//! It also hosts the agentic-tell prose engine (`tells`): a token-free
//! adaptation of tropes.fyi catalogued as lexical phrases and structural
//! equations, scored by density. The checker (`host-lint`) calls it.

mod tells;
pub use tells::{
    scan_chunked, scan_prose, scan_prose_markdown, scan_prose_parallel, tell_score,
    tell_score_markdown, Kind, Score, Tell,
};

/// Zero-padded width for the monotonic registers (milestones, decisions).
pub const PAD_WIDTH: usize = 4;

/// Format a register number as a zero-padded string: `1` -> `"0001"`.
pub fn format_number(n: u32) -> String {
    format!("{n:0PAD_WIDTH$}")
}

/// A valid number is the form `format_number` emits: a run of ASCII digits, zero-
/// padded to a *minimum* of `PAD_WIDTH`. Below the pad width it is exactly
/// `PAD_WIDTH` digits; at or past it the natural overflow (`"10000"`) carries no
/// leading zero. An over-padded number (more than `PAD_WIDTH` digits with a leading
/// zero) is never produced, so the checker accepts exactly the producible set.
fn is_valid_number(num: &str) -> bool {
    if num.is_empty() || !num.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    match num.len().cmp(&PAD_WIDTH) {
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => true,
        std::cmp::Ordering::Greater => !num.starts_with('0'),
    }
}

/// A valid name is `NNNN-slug`: a register number, a hyphen, then a slug.
pub fn is_valid_name(name: &str) -> bool {
    match name.split_once('-') {
        Some((num, slug)) => is_valid_number(num) && is_valid_slug(slug),
        None => false,
    }
}

/// A slug is lowercase letters, digits, and single internal hyphens: non-empty,
/// no leading/trailing hyphen, no doubled hyphen.
pub fn is_valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && !slug.contains("--")
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Whether a token reads as a numeral: an Arabic integer (`"5"`) or single
/// decimal (`"5.5"`), where a version-like form with two or more dots (`"1.2.3"`)
/// is not, or a canonical Roman numeral (`"II"`, `"IV"`). This is the numeral the
/// checker (`host-lint`) flags after a tell-noun and the generator pads into a
/// register number, defined once, here.
pub fn is_numeral(word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let parts: Vec<&str> = word.split('.').collect();
    if parts.len() <= 2 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
        return true;
    }
    is_canonical_roman(&word.to_uppercase())
}

/// Render `n` (1..=3999) as a canonical Roman numeral in standard subtractive form.
fn to_roman(mut n: u32) -> String {
    const TABLE: &[(u32, &str)] = &[
        (1000, "M"), (900, "CM"), (500, "D"), (400, "CD"),
        (100, "C"), (90, "XC"), (50, "L"), (40, "XL"),
        (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I"),
    ];
    let mut out = String::new();
    for &(v, sym) in TABLE {
        while n >= v {
            out.push_str(sym);
            n -= v;
        }
    }
    out
}

/// A canonical Roman numeral, validated by round-trip: the uppercased token must
/// re-render to itself, so non-canonical forms (`"IIII"`, `"VV"`) and ordinary
/// words built from the seven letters (`"LID"`, `"MID"`) are rejected. The bare
/// charset-and-length gate this replaces accepted all of those.
///
/// Standard ASCII Roman numerals span 1..=3999; 4000 and up need the vinculum (an
/// overlined numeral), which has no ASCII spelling, so `"MMMM"` is not canonical.
/// The `1..=3999` bound below is load-bearing, not a sanity check: `to_roman`
/// otherwise emits `"MMMM"` for 4000 and that string would round-trip to itself.
fn is_canonical_roman(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let value = |c: char| -> u32 {
        match c {
            'I' => 1, 'V' => 5, 'X' => 10, 'L' => 50, 'C' => 100, 'D' => 500, 'M' => 1000,
            _ => 0,
        }
    };
    let vals: Vec<u32> = s.chars().map(value).collect();
    if vals.contains(&0) {
        return false;
    }
    let mut total: i64 = 0;
    for i in 0..vals.len() {
        if i + 1 < vals.len() && vals[i] < vals[i + 1] {
            total -= vals[i] as i64;
        } else {
            total += vals[i] as i64;
        }
    }
    (1..=3999).contains(&total) && to_roman(total as u32) == s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pads_to_width() {
        assert_eq!(format_number(1), "0001");
        assert_eq!(format_number(42), "0042");
        assert_eq!(format_number(1234), "1234");
        assert_eq!(format_number(10000), "10000"); // overflow: minimum width, no truncation
    }

    #[test]
    fn accepts_well_formed_names() {
        assert!(is_valid_name("0000-use-markdown-decision-records"));
        assert!(is_valid_name("0001-example-milestone"));
        // The checker accepts exactly what the generator emits, including overflow.
        assert!(is_valid_name("10000-overflow")); // five-digit overflow, no leading zero
    }

    #[test]
    fn rejects_malformed_names() {
        assert!(!is_valid_name("1-x")); // unpadded number
        assert!(!is_valid_name("0001-Bad_Slug")); // uppercase + underscore
        assert!(!is_valid_name("0001-")); // empty slug
        assert!(!is_valid_name("0001--double")); // doubled hyphen
        assert!(!is_valid_name("example")); // no number
        assert!(!is_valid_name("00001-overpadded")); // over-padded, never generated
    }

    #[test]
    fn name_round_trips_through_generator() {
        // What the generator emits, the checker accepts — across the pad boundary.
        for n in [0u32, 1, 42, 9999, 10000, 12345] {
            assert!(is_valid_name(&format!("{}-slug", format_number(n))), "n={n}");
        }
    }

    #[test]
    fn numerals() {
        assert!(is_numeral("5"));
        assert!(is_numeral("5.5")); // single decimal
        assert!(is_numeral("II")); // roman
        assert!(is_numeral("IV")); // subtractive roman
        assert!(is_numeral("MMM")); // 3000
        assert!(!is_numeral("1.2.3")); // version string, not a numeral
        assert!(!is_numeral("first")); // ordinal word
        assert!(!is_numeral("")); // empty
        // The Roman arm validates canonically, not by charset alone.
        assert!(!is_numeral("lid")); // ordinary word built from roman letters
        assert!(!is_numeral("mid"));
        assert!(!is_numeral("mild"));
        assert!(!is_numeral("IIII")); // non-canonical (should be IV)
        assert!(!is_numeral("VV")); // non-canonical
        assert!(!is_numeral("MMMM")); // 4000, out of canonical range
    }
}
