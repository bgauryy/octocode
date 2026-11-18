use super::*;

/// The shipped automaton and a contiguous-NFA twin, for the differential
/// test.
pub(in crate::security::detector) fn shipped_and_reference() -> (&'static AhoCorasick, AhoCorasick)
{
    (&PRESCAN, automaton(AhoCorasickKind::ContiguousNFA))
}

/// A text selects a fallback pattern exactly when it holds one of that
/// pattern's weak-gate needles byte for byte, as a lookup of each needle on its
/// own finds: other letter cases do not count, and needles that touch or
/// overlap each still count.
#[test]
fn weak_gates_select_what_a_lookup_of_each_needle_selects() {
    let needles: Vec<&str> = gates::FALLBACK_GATES
        .iter()
        .flat_map(|(_, needles)| needles.iter().copied())
        .collect();
    let mut texts = vec![String::new(), "plain words only".to_owned()];
    for &needle in &needles {
        texts.push(format!("x{needle}y"));
        texts.push(needle.to_ascii_lowercase());
        texts.push(needle.to_ascii_uppercase());
        for &other in &needles {
            texts.push(format!("{needle}{other}"));
            for shared in 1..needle.len().min(other.len()) {
                if other.is_char_boundary(shared)
                    && let Some(head) = other.get(..shared)
                    && let Some(tail) = other.get(shared..)
                    && needle.ends_with(head)
                {
                    texts.push(format!("{needle}{tail}"));
                }
            }
        }
    }
    for text in &texts {
        let picked = prescan_candidates(text);
        for &(idx, needles) in gates::FALLBACK_GATES {
            let held = needles.is_empty() || needles.iter().any(|needle| text.contains(needle));
            let found = picked.iter().any(|found| found == idx);
            assert_eq!(found, held, "pattern {idx} on {text:?}");
        }
    }
}

/// A labelled credential needs its label word in the text: text that holds
/// none of them selects no label pattern, so no labelled regex runs over it,
/// however long the text is.
#[test]
fn text_without_a_label_word_selects_no_label_pattern() {
    let labelled: Vec<usize> = crate::security::patterns::PATTERNS
        .iter()
        .enumerate()
        .filter(|(_, pattern)| pattern.name.ends_with("Assignment"))
        .map(|(idx, _)| idx)
        .collect();
    assert!(!labelled.is_empty());
    for text in [
        "lorem ipsum dolor sit amet consectetur ".repeat(64),
        r#"{"name":"x","version":"1.0.0","keys_url":"https://api.github.com/repos/o/r/keys{/key_id}"}"#
            .to_owned(),
        "fn main() { let total = items.iter().sum::<u64>(); }".to_owned(),
    ] {
        let picked = prescan_candidates(&text);
        for &idx in &labelled {
            assert!(
                !picked.iter().any(|found| found == idx),
                "{} runs on {:?}",
                crate::security::patterns::PATTERNS[idx].name,
                text.get(..48).unwrap_or(&text)
            );
        }
    }
}
