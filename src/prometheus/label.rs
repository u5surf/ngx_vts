//! Escaping for Prometheus label values.
//!
//! Most of the labels here are literals chosen by this module (`direction`,
//! `status`, `le`), but five carry names from outside it: the host name, the
//! server zone, the cache zone, the upstream group and the peer address. Those
//! come from the configuration, and nginx's configuration parser accepts a
//! quoted token containing anything at all - `server_name 'a"b';` is a valid
//! line, and so is a unix socket path with a backslash in it.
//!
//! The exposition format has no way to represent such a character literally: a
//! `"` inside a label value ends the value, and a scraper that hits one rejects
//! the whole response, not just that line. One awkward name would take every
//! other metric down with it.
//!
//! The format defines three escapes for label values, and only three: `\\`,
//! `\"` and `\n`.

use std::borrow::Cow;

/// Escapes a label value for the Prometheus text exposition format.
///
/// Borrows when there is nothing to escape, which is every real name.
pub fn escape(value: &str) -> Cow<'_, str> {
    if !value.contains(['\\', '"', '\n']) {
        return Cow::Borrowed(value);
    }

    let mut out = String::with_capacity(value.len() + 8);
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_names_are_borrowed_untouched() {
        for name in ["localhost", "10.0.0.1:80", "backend", "example.test"] {
            assert!(matches!(escape(name), Cow::Borrowed(_)), "{name}");
            assert_eq!(escape(name), name);
        }
    }

    #[test]
    fn the_three_escapes_are_applied() {
        assert_eq!(escape(r#"a"b"#), r#"a\"b"#);
        assert_eq!(escape(r"a\b"), r"a\\b");
        assert_eq!(escape("a\nb"), r"a\nb");
    }

    #[test]
    fn a_backslash_before_a_quote_stays_two_separate_escapes() {
        // Escaping the quote first and the backslash second would produce
        // \\" - a literal backslash followed by an unescaped quote, which
        // ends the label value early.
        assert_eq!(escape(r#"\""#), r#"\\\""#);
    }

    #[test]
    fn a_carriage_return_is_left_alone() {
        // The format defines exactly three escapes. \r is not one of them, and
        // inventing a fourth would produce something no scraper reads back.
        assert_eq!(escape("a\rb"), "a\rb");
    }
}
