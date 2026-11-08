//! A walk's memo against cache clears (B165): a lazy DFA whose cache holds a
//! handful of states clears it inside most walks, and each verdict must still
//! be the pattern's own.

use super::Reach;
use crate::security::detector::regex_forms::{anchored_pattern_regex, ascii_word_boundaries};
use crate::security::patterns::PATTERNS;

use regex_automata::hybrid::dfa::DFA;
use regex_automata::nfa::thompson::{self, WhichCaptures};
use regex_automata::{MatchKind, util::syntax};

const PIECES: [&str; 14] = [
    "password",
    "token",
    "secret/",
    "key",
    "x",
    "=",
    ": ",
    " ",
    "#",
    "\"",
    "'",
    "a_b",
    "/a/",
    "Zq7xK2mPl",
];

/// Pattern `idx`'s DFA with the smallest cache its build allows, cleared as
/// often as it fills and never given up on.
fn small_cache_dfa(idx: usize) -> &'static DFA {
    let dfa = DFA::builder()
        .configure(
            DFA::config()
                .match_kind(MatchKind::LeftmostFirst)
                .cache_capacity(0)
                .skip_cache_capacity_check(true)
                .minimum_cache_clear_count(None),
        )
        .syntax(syntax::Config::new().crlf(true))
        .thompson(thompson::Config::new().which_captures(WhichCaptures::None))
        .build(&ascii_word_boundaries(PATTERNS[idx].regex))
        .expect("a built-in pattern");
    Box::leak(Box::new(dfa))
}

#[test]
fn a_walk_across_cache_clears_answers_as_the_pattern() {
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut next = |bound: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        usize::try_from(state % u64::try_from(bound).expect("a small bound")).expect("in range")
    };
    for name in [
        "yamlSecretValue",
        "dotenvFileCredential",
        "dotenvSecretAssignment",
    ] {
        let idx = PATTERNS
            .iter()
            .position(|pattern| pattern.name == name)
            .expect("a registered pattern");
        let regex = anchored_pattern_regex(idx, true);
        let mut reach = Reach::new(small_cache_dfa(idx));
        let mut cleared_walks = 0;
        for _ in 0..400 {
            let line: String = (0..next(60)).map(|_| PIECES[next(PIECES.len())]).collect();
            reach.begin_line();
            for from in 0..line.len() {
                let clears = reach.clear_count();
                assert_eq!(
                    reach.matches(line.as_bytes(), from, line.len()),
                    regex.is_match(&line[from..]),
                    "{name} from {from} of {line:?}"
                );
                cleared_walks += usize::from(reach.clear_count() != clears);
            }
        }
        assert!(cleared_walks > 0, "{name}: no walk saw its cache cleared");
    }
}
