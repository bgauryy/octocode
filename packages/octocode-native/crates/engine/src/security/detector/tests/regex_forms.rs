//! The ASCII form of a secret pattern may answer only for all-ASCII text, and
//! there it must answer exactly as the full form does.
use super::super::regex_forms::{
    ascii_classes, ascii_pattern_regex, pattern_regex, pattern_regex_for,
};
use super::*;

/// Each line holds a secret; the marked ones hold a non-ASCII byte in the token,
/// the key name or the spacing, where an ASCII class stops matching. The first
/// three are the same secrets in ASCII.
fn leak_cases() -> Vec<(&'static str, String)> {
    let run = "a1B2".repeat(12);
    let okta = format!("00{}", "a1B2".repeat(10));
    let anthropic = "a1B2".repeat(24);
    vec![
        ("ascii scalingo", format!("scalingo tk-us-{run}")),
        ("ascii anthropic", format!("anthropic sk-ant-api03-{anthropic}")),
        ("ascii okta", format!("okta_token = \"{okta}\"")),
        ("case1", "scalingo tk-us-a1B2a1B2a1B2a1B2a1B2a1B2éa1B2a1B2a1B2a1B2a1B2a1B".to_owned()),
        ("case2", "anthropic sk-ant-api03-a1B2a1B2a1B2a1B2a1B2a1B2a1B2a1B2a1B2a1B2éa1B2a1B2a1B2a1B2a1B2a1B2a1B2a1B2a1B2a1B2a1B2a1B2a".to_owned()),
        ("case3", "okta_clé = \"00a1B2a1B2a1B2a1B2a1B2a1B2a1B2a1B2a1B2a1B2\"".to_owned()),
        ("case4", "okta token = \"00a1B2a1B2a1B2a1B2a1B2a1B2a1B2a1B2a1B2a1B2\"".to_owned()),
        ("case5", "okta_token = \"00a1B2a1B2a1B2a1B2a1B2éa1B2a1B2a1B2a1B2a1B\"".to_owned()),
    ]
}

/// Whichever form serves a text, each of these lines has a secret redacted. An
/// ASCII form serving the non-ASCII lines would leave them verbatim.
#[test]
fn a_secret_redacts_alike_whatever_bytes_surround_it() {
    for (label, line) in leak_cases() {
        let result = detect(&line, None);
        assert!(
            !result.secrets_detected.is_empty(),
            "{label} went verbatim: {line}"
        );
        assert_ne!(result.sanitized, line, "{label}");
    }
}

#[test]
fn non_ascii_text_never_reaches_the_ascii_form() {
    for idx in 0..PATTERNS.len().min(8) {
        assert!(std::ptr::eq(
            pattern_regex_for(idx, "caf\u{e9}"),
            pattern_regex(idx)
        ));
        assert!(std::ptr::eq(
            pattern_regex_for(idx, "cafe"),
            ascii_pattern_regex(idx)
        ));
    }
}

/// The proof behind the ASCII form: on every ASCII character each rewritten class
/// answers as the Unicode class does, in and out of a set.
#[test]
fn each_ascii_class_holds_exactly_the_unicode_classes_ascii_members() {
    for class in [r"\w", r"\d", r"\s"] {
        for pattern in [format!("^{class}$"), format!("^[{class}-]$")] {
            let full = regex::Regex::new(&pattern).expect("full");
            let ascii = regex::Regex::new(&ascii_classes(&pattern)).expect("ascii");
            for byte in 0u8..128 {
                let text = char::from(byte).to_string();
                assert_eq!(
                    full.is_match(&text),
                    ascii.is_match(&text),
                    "{pattern} on {byte}"
                );
            }
        }
    }
}

#[test]
fn classes_spell_their_ascii_members_in_and_out_of_a_class() {
    assert_eq!(ascii_classes(r"\w+"), "[0-9A-Za-z_]+");
    assert_eq!(ascii_classes(r"[\w-]{8}"), "[0-9A-Za-z_-]{8}");
    assert_eq!(ascii_classes(r"[^\d]"), "[^0-9]");
    assert_eq!(ascii_classes(r"\s"), r"[\t\n\x0B\x0C\r ]");
    assert_eq!(ascii_classes(r"\\w"), r"\\w", "an escaped backslash");
    assert_eq!(ascii_classes(r"[\s\S]*?"), r"[\t\n\x0B\x0C\r \S]*?");
    assert_eq!(ascii_classes(r"\W\D\S"), r"\W\D\S");
}

/// Every pattern, on ASCII text full of secret shapes: both forms find the same spans.
#[test]
fn on_ascii_text_both_forms_find_the_same_spans() {
    let mut corpus = String::new();
    let security = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/security");
    let mut dirs = vec![security];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).expect("dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path).expect("source");
                for line in text.lines().filter(|line| line.is_ascii()) {
                    corpus.push_str(line);
                    corpus.push('\n');
                }
            }
        }
    }
    for (_, line) in leak_cases() {
        if line.is_ascii() {
            corpus.push_str(&line);
            corpus.push('\n');
        }
    }
    assert!(corpus.is_ascii());
    let spans = |regex: &regex::Regex| {
        regex
            .find_iter(&corpus)
            .map(|found| found.range())
            .collect::<Vec<_>>()
    };
    let mut matched = 0;
    for (idx, pattern) in PATTERNS.iter().enumerate() {
        let full = spans(pattern_regex(idx));
        assert_eq!(spans(ascii_pattern_regex(idx)), full, "{}", pattern.name);
        matched += usize::from(!full.is_empty());
    }
    assert!(matched > 10, "the corpus exercises the patterns: {matched}");
}
