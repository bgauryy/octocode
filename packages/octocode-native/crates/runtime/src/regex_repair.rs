//! Runnable repairs for an invalid regex `matchString`: closing a group left
//! open and escaping the alternatives that do not parse on their own.

/// `x_(a|b` → `x_(a|b)`: exactly one group is left open and everything after
/// it is two or more bare word alternatives. `None` otherwise, so call syntax
/// such as `f("a"|g(` keeps the per-alternative escape.
pub fn close_unclosed_group(text: &str, pcre2: bool) -> Option<String> {
    let mut open = Vec::new();
    let mut in_class = false;
    let mut chars = text.char_indices();
    while let Some((index, c)) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '[' if !in_class => in_class = true,
            ']' if in_class => in_class = false,
            '(' if !in_class => open.push(index),
            ')' if !in_class => {
                open.pop()?;
            }
            _ => {}
        }
    }
    let [start] = open.as_slice() else {
        return None;
    };
    let alternatives = text[start + 1..].split('|').collect::<Vec<_>>();
    let bare = |part: &&str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-'))
    };
    if alternatives.len() < 2 || !alternatives.iter().all(bare) {
        return None;
    }
    let closed = format!("{text})");
    octocode_engine::search::text_pattern::validate_text_pattern(&closed, false, pcre2)
        .valid
        .then_some(closed)
}

/// `a(|b|c[` → `a\(|b|c\[`: split on unescaped `|`, keep alternatives that
/// parse on their own, escape the rest. An empty alternative (`f(|`, `|g(`)
/// is dropped: it would match every line, which no failed search meant.
/// `None` for a single alternative, when nothing but empty ones remain, or
/// when the joined result still fails to parse.
pub fn repair_alternation(text: &str, pcre2: bool) -> Option<String> {
    let mut parts = vec![String::new()];
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let part = parts.last_mut()?;
                part.push(c);
                if let Some(next) = chars.next() {
                    part.push(next);
                }
            }
            '|' => parts.push(String::new()),
            _ => parts.last_mut()?.push(c),
        }
    }
    if parts.len() < 2 {
        return None;
    }
    let valid = |pattern: &str| {
        octocode_engine::search::text_pattern::validate_text_pattern(pattern, false, pcre2).valid
    };
    let repaired = parts
        .iter()
        .filter(|part| !part.is_empty())
        .map(|part| {
            if !part.is_empty() && valid(part) {
                part.clone()
            } else {
                regex::escape(part)
            }
        })
        .collect::<Vec<_>>()
        .join("|");
    (!repaired.is_empty() && valid(&repaired)).then_some(repaired)
}

/// Whether a failing Rust-engine pattern set is only a PCRE2 construct
/// (backreference, lookaround): every pattern compiles as PCRE2 and the
/// caller is not already running it. The same read with `regex:"pcre2"` is
/// then the repair.
pub fn only_pcre2_compiles<'a>(patterns: impl IntoIterator<Item = &'a str>, pcre2: bool) -> bool {
    !pcre2
        && patterns.into_iter().all(|pattern| {
            octocode_engine::search::text_pattern::validate_text_pattern(pattern, false, true).valid
        })
}

#[cfg(test)]
mod tests {
    use super::{close_unclosed_group, only_pcre2_compiles, repair_alternation};

    #[test]
    fn only_pcre2_compiles_names_the_constructs_the_rust_engine_lacks() {
        assert!(only_pcre2_compiles([r"(\w)\1"], false));
        assert!(only_pcre2_compiles(["^fn ", r"(?<=a)b"], false));
        // Already PCRE2, or invalid there too: no retry.
        assert!(!only_pcre2_compiles([r"(\w)\1"], true));
        assert!(!only_pcre2_compiles(["call("], false));
        assert!(!only_pcre2_compiles([r"(\w)\1", "call("], false));
    }

    #[test]
    fn an_unclosed_group_of_bare_alternatives_closes() {
        assert_eq!(
            close_unclosed_group("poll_(proceed|budget", false).as_deref(),
            Some("poll_(proceed|budget)")
        );
        // Call syntax, a single alternative or a balanced pattern is not closed.
        for text in [
            "capture_limited|hydrate_candidate|capture_resource(",
            "insert_header(\"retry-after\"|append_header(\"retry-after\"|set_body_json",
            "f(a",
            "(a|b)",
        ] {
            assert_eq!(close_unclosed_group(text, false), None, "{text}");
        }
    }

    #[test]
    fn broken_alternatives_are_escaped_and_valid_ones_kept() {
        for (text, expected) in [
            (
                "capture_limited|hydrate_candidate|capture_resource(",
                "capture_limited|hydrate_candidate|capture_resource\\(",
            ),
            (
                "insert_header(\"retry-after\"|append_header(\"retry-after\"|set_body_json",
                "insert_header\\(\"retry\\-after\"|append_header\\(\"retry\\-after\"|set_body_json",
            ),
        ] {
            assert_eq!(repair_alternation(text, false).as_deref(), Some(expected));
        }
    }

    /// A stray `|` leaves an empty branch, which matches every line; the
    /// repair keeps only the anchors the caller wrote.
    #[test]
    fn an_empty_alternative_is_dropped_not_kept_as_match_everything() {
        for (text, expected) in [
            ("fn (validate|", "fn \\(validate"),
            ("|call(", "call\\("),
            ("a(||b", "a\\(|b"),
            ("(|", "\\("),
        ] {
            let repaired = repair_alternation(text, false);
            assert_eq!(repaired.as_deref(), Some(expected), "{text}");
            let pattern = regex::Regex::new(expected).expect("valid");
            assert!(!pattern.is_match("an unrelated line"), "{text}");
        }
        assert_eq!(repair_alternation("|", false), None);
    }

    #[test]
    fn escaped_bars_do_not_split_alternatives() {
        assert_eq!(repair_alternation("a\\|b(", false), None, "one alternative");
        assert_eq!(repair_alternation("x[|y", false).as_deref(), Some("x\\[|y"));
    }
}
