//! Many secrets on one line cost what their bytes cost, with the results of
//! finding each one's line start by searching back from it.

use super::*;

/// `count` tokens on a single line.
fn line_of_tokens(count: usize) -> String {
    let token = format!("ghp_{}", "Z9x8C7v6B5n4".repeat(3));
    (0..count)
        .map(|_| format!("{} t=\"{token}\" ", "p".repeat(53)))
        .collect()
}

fn fastest_redact(content: &str) -> std::time::Duration {
    (0..3)
        .map(|_| {
            let started = std::time::Instant::now();
            std::hint::black_box(redact(content, Scope::Pathless));
            started.elapsed()
        })
        .min()
        .expect("three runs")
}

/// Eight times the tokens: a linear scan takes about eight times as long, one
/// that walks back to the line start from each match about sixty-four.
#[test]
fn tokens_on_one_line_scan_in_linear_time() {
    let (few, many) = (line_of_tokens(4_000), line_of_tokens(32_000));
    // A busy machine can inflate one measurement; a scan that walks back misses
    // on every one.
    let mut measured = Vec::new();
    for _ in 0..3 {
        let (few, many) = (fastest_redact(&few), fastest_redact(&many));
        let ratio = many.as_secs_f64() / few.as_secs_f64().max(f64::EPSILON);
        if ratio < 24.0 {
            return;
        }
        measured.push(format!("{few:?} to {many:?}: {ratio:.1}x"));
    }
    panic!("{measured:?}");
}

/// The measured form of a scan: eight times the input in about eight times the
/// time. A busy machine can inflate one measurement; a scan that walks back
/// misses on every one.
fn assert_scales_linearly(few: &str, many: &str) {
    let mut measured = Vec::new();
    for _ in 0..3 {
        let (few, many) = (fastest_redact(few), fastest_redact(many));
        let ratio = many.as_secs_f64() / few.as_secs_f64().max(f64::EPSILON);
        if ratio < 24.0 {
            return;
        }
        measured.push(format!("{few:?} to {many:?}: {ratio:.1}x"));
    }
    panic!("{measured:?}");
}

/// Object nesting costs what its bytes cost: to the end of the deepest object,
/// back out of it, and with every object left open.
#[test]
fn deeply_nested_objects_scan_in_linear_time() {
    const VALUE: &str = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIz";
    let nested = |depth: usize| {
        format!(
            r#"{{"kind":"Secret",{}"x":1{},"data":{{"k":"{VALUE}"}}}}"#,
            r#""m":{"#.repeat(depth),
            "}".repeat(depth)
        )
    };
    let open = |depth: usize| format!(r#"{{"kty":"EC",{}"#, r#""m":{"d":[{"#.repeat(depth));
    let (few, many) = (nested(40_000), nested(320_000));
    assert!(
        !redact(&few, Scope::Pathless)
            .expect("hidden")
            .sanitized
            .contains(VALUE)
    );
    assert_scales_linearly(&few, &many);
    assert_scales_linearly(&open(40_000), &open(320_000));
}

/// As many mappings as lines, each closing the last, and a block that holds them.
#[test]
fn yaml_mappings_scan_in_linear_time() {
    const VALUE: &str = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIz";
    let items = |count: usize| {
        format!(
            "kind: Secret\nitems:\n{}data:\n  k: {VALUE}\n",
            "- a: b\n  c:\n    d: e\n".repeat(count)
        )
    };
    let (few, many) = (items(40_000), items(320_000));
    let scan = |text: &str| {
        (0..3)
            .map(|_| {
                let started = std::time::Instant::now();
                let found = redact(text, Scope::File("a.yaml")).expect("hidden");
                assert!(!found.sanitized.contains(VALUE));
                started.elapsed()
            })
            .min()
            .expect("three runs")
    };
    let mut measured = Vec::new();
    for _ in 0..3 {
        let (few, many) = (scan(&few), scan(&many));
        let ratio = many.as_secs_f64() / few.as_secs_f64().max(f64::EPSILON);
        if ratio < 24.0 {
            return;
        }
        measured.push(format!("{few:?} to {many:?}: {ratio:.1}x"));
    }
    panic!("{measured:?}");
}

/// What the replacement took before it carried the line start forward: each
/// match's line start searched back from it. The value's extent
/// ([`continued_end`]) is the replacement's own, the same in both.
fn replace_searching_back(
    regex: &regex::Regex,
    content: &str,
    replacement: &str,
    redacts: impl Fn(&str, &str, &std::ops::Range<usize>) -> bool,
) -> String {
    let groups: Vec<usize> = regex
        .capture_names()
        .enumerate()
        .filter_map(|(index, name)| {
            name.filter(|name| name.starts_with(SECRET_GROUP))
                .map(|_| index)
        })
        .collect();
    let diff_lines = DiffLines::new(regex, content);
    let (mut redacted, mut from) = (String::new(), 0);
    for captures in regex.captures_iter(content) {
        let whole = captures.get(0).expect("whole match");
        let secret = groups
            .iter()
            .find_map(|&index| captures.get(index))
            .unwrap_or(whole);
        let line_start = content[..whole.start()]
            .rfind(LINE_END_CHARS)
            .map_or(0, |at| at + 1);
        if secret.start() < from
            || diff_lines.outside_hunk(diff_lines.prefixed(&captures), line_start)
            || !redacts(
                &content[line_start..secret.start()],
                secret.as_str(),
                &whole.range(),
            )
        {
            continue;
        }
        let end = continued_end(content, secret.end());
        redacted.push_str(&content[from..secret.start()]);
        redacted.push_str(replacement);
        redacted.push_str(&kept_line_breaks(&content[secret.start()..end]));
        from = end;
    }
    redacted.push_str(&content[from..]);
    redacted
}

/// Every pattern on text it matches, with secrets on the same line and on
/// later lines: the same output, and the same key text handed to the filter.
#[test]
fn carrying_the_line_start_forward_replaces_as_searching_back_does() {
    use super::sampling::{Lcg, sample};
    let mut rng = Lcg(0xfeed);
    type KeyFilter<'a> = &'a dyn Fn(&str, &str, &std::ops::Range<usize>) -> bool;
    type Replacer<'a> = &'a dyn Fn(&regex::Regex, &str, KeyFilter<'_>) -> String;
    let keys_seen = |regex: &regex::Regex, text: &str, replace_with: Replacer<'_>| {
        let seen = std::cell::RefCell::new(Vec::new());
        let out = replace_with(regex, text, &|key, secret, _| {
            seen.borrow_mut().push((key.to_owned(), secret.to_owned()));
            secret.len() % 2 == 0
        });
        (out, seen.into_inner())
    };
    for (idx, pattern) in PATTERNS.iter().enumerate() {
        let hir = regex_syntax::parse(pattern.regex).expect(pattern.name);
        let regex = pattern_regex(idx);
        for _ in 0..8 {
            let mut text = String::from("lead ");
            for at in 0..4 {
                sample(&hir, &mut rng, &mut text);
                text.push_str(if at % 2 == 0 { " mid " } else { "\nnext " });
            }
            let carried = keys_seen(regex, &text, &|regex, text, redacts| {
                replace_secrets(regex, text, "[R]", redacts).0
            });
            let searched = keys_seen(regex, &text, &|regex, text, redacts| {
                replace_searching_back(regex, text, "[R]", redacts)
            });
            assert_eq!(carried, searched, "{}: {text:?}", pattern.name);
        }
    }
}
