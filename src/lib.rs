//! host-grammar — the shared rules for valid agentic-host names and numbers.
//!
//! Both the host-lint detector (the *checker*) and host-lifecycle (the
//! *generator*) depend on this crate, so that what the generator emits is
//! exactly what the checker accepts. The number is identity; the slug is
//! content; ordering lives in the index, never in the name.

/// Zero-padded width for the monotonic registers (milestones, decisions).
pub const PAD_WIDTH: usize = 4;

/// Format a register number as a zero-padded string: `1` -> `"0001"`.
pub fn format_number(n: u32) -> String {
    format!("{n:0PAD_WIDTH$}")
}

/// A valid name is `NNNN-slug`: a zero-padded number, a hyphen, then a slug.
pub fn is_valid_name(name: &str) -> bool {
    match name.split_once('-') {
        Some((num, slug)) => {
            num.len() == PAD_WIDTH
                && num.chars().all(|c| c.is_ascii_digit())
                && is_valid_slug(slug)
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pads_to_width() {
        assert_eq!(format_number(1), "0001");
        assert_eq!(format_number(42), "0042");
        assert_eq!(format_number(1234), "1234");
    }

    #[test]
    fn accepts_well_formed_names() {
        assert!(is_valid_name("0000-use-markdown-decision-records"));
        assert!(is_valid_name("0001-example-milestone"));
    }

    #[test]
    fn rejects_malformed_names() {
        assert!(!is_valid_name("1-x")); // unpadded number
        assert!(!is_valid_name("0001-Bad_Slug")); // uppercase + underscore
        assert!(!is_valid_name("0001-")); // empty slug
        assert!(!is_valid_name("0001--double")); // doubled hyphen
        assert!(!is_valid_name("example")); // no number
    }
}
