use super::*;
use crate::security::detector::regex_forms::{anchored_pattern_regex, pattern_nfa};
use crate::security::patterns::PATTERNS;

fn pattern(name: &str) -> usize {
    PATTERNS
        .iter()
        .position(|pattern| pattern.name == name)
        .expect("a registered pattern")
}

/// Pieces of comment lines: keys, operators, values, quotes and the marks
/// the tail patterns read.
const PIECES: [&str; 22] = [
    "password",
    "token",
    "secret",
    "api_key",
    "x",
    "Zq7xK2mPlanted9",
    "=",
    ":",
    " ",
    "/",
    "[0]",
    ".",
    "-",
    "_",
    "\"",
    "'",
    "`",
    "#",
    " # ",
    "+",
    "é",
    "\\",
];

/// The slots the regex crate reports for the window `text[from..to]`.
fn regex_slots(idx: usize, text: &str, from: usize, to: usize) -> Option<Slots> {
    let window = &text[from..to];
    let regex = anchored_pattern_regex(idx, window.is_ascii());
    let captures = regex.captures(window)?;
    Some(
        (0..captures.len())
            .flat_map(|group| {
                let span = captures.get(group).map(|found| found.range());
                [
                    span.clone().map(|span| span.start + from),
                    span.map(|span| span.end + from),
                ]
            })
            .collect(),
    )
}

/// Every walk of a line, in line order with one memo as the tails read them,
/// answers what the regex answers for its window: the same groups at the
/// same places, or no match.
#[test]
fn the_walks_of_a_line_capture_what_the_regex_captures() {
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut next = |bound: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        usize::try_from(state % u64::try_from(bound).expect("small")).expect("in range")
    };
    let mut matched = 0;
    for name in [
        "yamlSecretValue",
        "dotenvSecretAssignment",
        "dotenvFileCredential",
        "propertiesFileCredential",
    ] {
        let idx = pattern(name);
        let mut walks = TailCaptures::new(pattern_nfa(idx));
        for _ in 0..1_500 {
            let pieces = 1 + next(24);
            let line: String = (0..pieces).map(|_| PIECES[next(PIECES.len())]).collect();
            let text = format!("// {line}");
            walks.begin_line();
            for from in (3..text.len()).filter(|&at| text.is_char_boundary(at)) {
                let expected = regex_slots(idx, &text, from, text.len());
                matched += usize::from(expected.is_some());
                assert_eq!(
                    walks.find(text.as_bytes(), from, text.len()),
                    expected,
                    "{name} {text:?} from {from}"
                );
            }
        }
    }
    assert!(matched > 1_000, "only {matched} windows matched");
}

/// A line of keys each in the value of the one before is walked once per
/// list of threads: doubling the line doubles the places stepped.
#[test]
fn the_walks_of_a_line_of_keys_step_each_place_a_bounded_number_of_times() {
    for (name, unit) in [
        ("dotenvSecretAssignment", "token=`a"),
        ("dotenvSecretAssignment", "token="),
        ("yamlSecretValue", "token: "),
        ("propertiesFileCredential", "token[0]."),
    ] {
        let steps = |n: usize| {
            let text = format!("// {}", unit.repeat(n));
            let mut walks = TailCaptures::new(pattern_nfa(pattern(name)));
            for from in (3..text.len()).step_by(unit.len()) {
                walks.find(text.as_bytes(), from, text.len());
            }
            walks.steps()
        };
        let (small, large) = (steps(1_000), steps(2_000));
        assert!(
            large <= 3 * small,
            "{name} {unit:?}: {small} places, then {large}"
        );
    }
}
