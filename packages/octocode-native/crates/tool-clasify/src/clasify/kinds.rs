//! A repetitive text (a log) as its kinds of line. Lines that differ only in
//! numbers that vary widely (timestamps, ids, durations) are one kind: its
//! first line stands for all of them, with how many lines it stands for. A
//! number that takes few distinct values across such lines (a status, a
//! level, an exit code) tells kinds apart: a status that is `200` on a
//! thousand lines and `500` on one is two kinds, each with its own first
//! line. Nothing is guessed about what a line means: digits are the only
//! thing compared.
use serde_json::{Value, json};
use std::collections::HashMap;

/// A number with at most this many distinct values across the lines of one
/// shape tells kinds apart; one with more is an identifier or a measure.
const DISTINCT_VALUES: usize = 8;

/// One kind of line.
pub(super) struct Kind {
    /// The 1-based line the kind first appears on.
    pub line: usize,
    /// What the judge reads of it.
    pub state: Value,
}

/// A line with every run of ASCII digits as one `#`, and the runs.
fn masked(line: &str) -> (String, Vec<&str>) {
    let mut shape = String::with_capacity(line.len());
    let mut numbers = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find(|c: char| c.is_ascii_digit()) {
        let (before, digits) = rest.split_at(start);
        shape.push_str(before);
        shape.push('#');
        let end = digits
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(digits.len());
        let (number, after) = digits.split_at(end);
        numbers.push(number);
        rest = after;
    }
    shape.push_str(rest);
    (shape, numbers)
}

/// The kinds of line of `text`, in the order they first appear; blank lines
/// are no kind.
pub(super) fn kinds(text: &str) -> Vec<Kind> {
    let lines = text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(at, line)| (at + 1, line, masked(line)))
        .collect::<Vec<_>>();
    // Per shape and number: its distinct values, or `None` once it took
    // more than [`DISTINCT_VALUES`].
    let mut values = HashMap::<&str, Vec<Option<Vec<&str>>>>::new();
    for (_, _, (shape, numbers)) in &lines {
        let slots = values
            .entry(shape.as_str())
            .or_insert_with(|| vec![Some(Vec::new()); numbers.len()]);
        for (slot, number) in slots.iter_mut().zip(numbers) {
            if let Some(seen) = slot
                && !seen.contains(number)
            {
                if seen.len() == DISTINCT_VALUES {
                    *slot = None;
                } else {
                    seen.push(number);
                }
            }
        }
    }
    // A kind is a shape and the values of its few-valued numbers.
    let mut order: Vec<(usize, &str, usize)> = Vec::new();
    let mut known = HashMap::<(&str, Vec<&str>), usize>::new();
    for (line, text, (shape, numbers)) in &lines {
        let telling = values[shape.as_str()]
            .iter()
            .zip(numbers)
            .filter(|(slot, _)| slot.is_some())
            .map(|(_, number)| *number)
            .collect::<Vec<_>>();
        match known.get(&(shape.as_str(), telling.clone())) {
            Some(&index) => order[index].2 += 1,
            None => {
                known.insert((shape.as_str(), telling), order.len());
                order.push((*line, text, 1));
            }
        }
    }
    order
        .into_iter()
        .map(|(line, text, count)| {
            let mut state = json!({"line": line, "text": text});
            if count > 1 {
                state["lines"] = json!(count);
            }
            Kind { line, state }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every line belongs to exactly one kind, and a kind is named by the
    /// first line it appears on: lines whose number varies widely are one
    /// kind, a blank line is none.
    #[test]
    fn lines_that_differ_only_in_a_widely_varying_number_are_one_kind() {
        let many = DISTINCT_VALUES * 3;
        let mut text = String::from("start\n\n");
        for id in 0..many {
            text.push_str(&format!("ok {id}\n"));
        }
        text.push_str("fail at 7\n");
        let kinds = kinds(&text);
        let lines = text.lines().filter(|line| !line.trim().is_empty()).count();
        let counted: u64 = kinds
            .iter()
            .map(|kind| kind.state["lines"].as_u64().unwrap_or(1))
            .sum();
        assert_eq!(counted as usize, lines, "every line is counted once");
        // start (line 1), the ok lines (first on line 3), the failure.
        assert_eq!(
            kinds.iter().map(|kind| kind.line).collect::<Vec<_>>(),
            [1, 3, many + 3]
        );
        assert_eq!(kinds[1].state["lines"], json!(many));
    }

    /// A number with few distinct values tells kinds apart, so a rare value
    /// is its own kind, named by the line it sits on; a number with many
    /// values (an id) never splits one.
    #[test]
    fn a_rare_value_of_a_number_is_its_own_kind_with_its_line() {
        let (rare, total) = (31, 40);
        let mut text = String::new();
        for request in 0..total {
            let status = if request == rare { 500 } else { 200 };
            text.push_str(&format!("GET /item/{request} -> {status}\n"));
        }
        let kinds = kinds(&text);
        assert_eq!(kinds.len(), 2, "the common status and the rare one");
        let odd = kinds
            .iter()
            .find(|kind| kind.line == rare + 1)
            .expect("the rare line is a kind");
        assert!(odd.state["text"].as_str().unwrap().ends_with("500"));
        assert!(odd.state.get("lines").is_none(), "it stands for itself");
        let counted: u64 = kinds
            .iter()
            .map(|kind| kind.state["lines"].as_u64().unwrap_or(1))
            .sum();
        assert_eq!(counted as usize, total);
    }
}
