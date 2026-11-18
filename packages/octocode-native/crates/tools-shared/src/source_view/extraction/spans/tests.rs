use super::*;
use crate::page_memo::{sized_test_budget, test_budget};

const SPAN: usize = std::mem::size_of::<(usize, usize)>();

/// Whenever the vector reallocates, the budget already holds the old
/// block and the new one together.
#[test]
fn a_growing_span_vector_is_reserved_before_it_is_allocated() {
    let budget = test_budget!(1 << 20);
    let mut sink = HeldSpans::new(budget);
    let mut reallocs = 0;
    for n in 0..200 {
        let before = sink.spans.capacity();
        let mut observed = None;
        assert!(sink.accept_observed((n, n + 1), || observed = Some(budget.used())));
        if let Some(used) = observed {
            reallocs += 1;
            assert!(
                used >= (before + sink.spans.capacity()) * SPAN,
                "reserved {used} while {before} + {} spans are allocated",
                sink.spans.capacity()
            );
        }
        assert!(budget.used() >= sink.spans.capacity() * SPAN);
    }
    assert!(reallocs > 1);
}

/// What the copy holds is what `extend` allocated: the vector's capacity
/// (it rounds a small first block up and doubles a full one), seen
/// through the budget, however many appends built it.
#[test]
fn an_appended_copy_holds_the_capacity_it_allocated() {
    let budget = test_budget!(1 << 20);
    let _scope = SpanBudgetOverride::of(budget);
    let mut found = RegexSpans::default();
    for round in 1..40 {
        let batch: Vec<_> = (0..round).map(|n| (n, n + 1)).collect();
        assert!(append_held(&mut found, &batch));
        assert_eq!(
            budget.used(),
            found.spans.capacity() * SPAN,
            "{} spans held after {round} appends",
            found.spans.len()
        );
    }
    drop(found);
    assert_eq!(budget.used(), 0);
}

fn literal(word: &str) -> LocalFetchQuery {
    serde_json::from_value(serde_json::json!({"path":"f.txt","matchString":[word]})).expect("query")
}

/// The copy a literal scan makes into the result is held: a budget that
/// cannot hold it refuses the copy and the read says it is partial.
#[test]
fn a_span_copy_past_the_budget_is_refused_and_disclosed() {
    let content = "x ".repeat(10);
    let budget = test_budget!(SPAN * 3);
    let _scope = SpanBudgetOverride::of(budget);
    let found = match_spans(&literal("x"), &content, &["x"]).expect("scan");
    assert!(found.truncated);
    assert!(found.spans.is_empty());
    assert!(budget.used() <= budget.limit());
}

fn regex(engine: &str) -> LocalFetchQuery {
    serde_json::from_value(serde_json::json!({
        "path": "f.txt", "matchString": ["x"], "regexEngine": engine
    }))
    .expect("query")
}

/// One pattern's spans are held once: under the least budget its scan
/// completes in, the read returns every span, holding what the vector
/// allocated. Holding a copy beside the scan's own spans needed more
/// than half the budget twice, and refused the read.
#[test]
fn one_patterns_spans_are_held_once() {
    let content = "x ".repeat(10);
    for engine in ["rust", "pcre2"] {
        let scan = |budget: &'static MemoryBudget| {
            let _scope = SpanBudgetOverride::of(budget);
            let spans = if engine == "rust" {
                rust_ranges("x", true, &content)
            } else {
                pcre2_ranges("x", true, &content)
            };
            !spans.expect("scan").truncated
        };
        let least = (1..)
            .map(|spans| spans * SPAN)
            .find(|bytes| scan(sized_test_budget(*bytes)))
            .expect("a budget the scan completes in");
        let budget = sized_test_budget(least);
        let _scope = SpanBudgetOverride::of(budget);
        let found = match_spans(&regex(engine), &content, &["x"]).expect("read");
        assert!(!found.truncated, "{engine}: refused under {least} bytes");
        assert_eq!(found.spans.len(), 10, "{engine}");
        assert_eq!(budget.used(), found.spans.capacity() * SPAN, "{engine}");
        drop(found);
        assert_eq!(budget.used(), 0, "{engine}");
    }
}

#[test]
fn a_span_copy_within_the_budget_is_held_until_the_result_drops() {
    let content = "x ".repeat(10);
    let budget = test_budget!(SPAN * 64);
    let _scope = SpanBudgetOverride::of(budget);
    let found = match_spans(&literal("x"), &content, &["x"]).expect("scan");
    assert!(!found.truncated);
    assert_eq!(found.spans.len(), 10);
    assert!(budget.used() >= found.spans.len() * SPAN);
    drop(found);
    assert_eq!(budget.used(), 0);
}
