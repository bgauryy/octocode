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
    let (few, many) = (fastest_redact(&few), fastest_redact(&many));
    let ratio = many.as_secs_f64() / few.as_secs_f64().max(f64::EPSILON);
    assert!(ratio < 24.0, "{few:?} to {many:?}: {ratio:.1}x");
}

/// What the replacement took before it carried the line start forward.
fn replace_searching_back(
    regex: &regex::Regex,
    content: &str,
    replacement: &str,
    redacts: impl Fn(&str, &str) -> bool,
) -> String {
    let groups: Vec<usize> = regex
        .capture_names()
        .enumerate()
        .filter_map(|(index, name)| {
            name.filter(|name| name.starts_with(SECRET_GROUP))
                .map(|_| index)
        })
        .collect();
    regex
        .replace_all(content, |captures: &regex::Captures<'_>| {
            let whole = captures.get(0).expect("whole match");
            let secret = groups
                .iter()
                .find_map(|&index| captures.get(index))
                .unwrap_or(whole);
            let line_start = content[..whole.start()].rfind('\n').map_or(0, |at| at + 1);
            if !redacts(&content[line_start..secret.start()], secret.as_str()) {
                return whole.as_str().to_owned();
            }
            let line_breaks = secret
                .as_str()
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            let mut redacted = String::with_capacity(whole.len() + replacement.len());
            redacted.push_str(&content[whole.start()..secret.start()]);
            redacted.push_str(replacement);
            redacted.extend(std::iter::repeat_n('\n', line_breaks));
            redacted.push_str(&content[secret.end()..whole.end()]);
            redacted
        })
        .into_owned()
}

/// Every pattern on text it matches, with secrets on the same line and on
/// later lines: the same output, and the same key text handed to the filter.
#[test]
fn carrying_the_line_start_forward_replaces_as_searching_back_does() {
    use super::sampling::{Lcg, sample};
    let mut rng = Lcg(0xfeed);
    let keys_seen = |regex: &regex::Regex,
                     text: &str,
                     replace_with: &dyn Fn(
        &regex::Regex,
        &str,
        &dyn Fn(&str, &str) -> bool,
    ) -> String| {
        let seen = std::cell::RefCell::new(Vec::new());
        let out = replace_with(regex, text, &|key, secret| {
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
                replace_secrets(regex, text, "[R]", redacts)
            });
            let searched = keys_seen(regex, &text, &|regex, text, redacts| {
                replace_searching_back(regex, text, "[R]", redacts)
            });
            assert_eq!(carried, searched, "{}: {text:?}", pattern.name);
        }
    }
}
