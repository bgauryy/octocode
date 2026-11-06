//! The owner of each call site among many declarations: the one sweep
//! names what a scan of every declaration names.
use crate::graph::GraphFactDeclaration;
use crate::text::analysis::ObservedSpan;

/// Declarations over `[start, end)` byte ranges, in this order.
fn declarations(ranges: &[(u32, u32)]) -> Vec<GraphFactDeclaration> {
    ranges
        .iter()
        .map(|&(start, end)| GraphFactDeclaration {
            source_range: Some(ObservedSpan::root(start, end)),
            ..GraphFactDeclaration::default()
        })
        .collect()
}

/// The owner as the per-span scan read it: the shortest holding range, the
/// first of equally short ones.
fn scanned(declarations: &[GraphFactDeclaration], (start, end): (u32, u32)) -> Option<usize> {
    declarations
        .iter()
        .enumerate()
        .filter(|(_, d)| {
            d.source_range
                .as_ref()
                .is_some_and(|s| s.start_byte <= start && end <= s.end_byte)
        })
        .min_by_key(|(_, d)| {
            d.source_range
                .as_ref()
                .map_or(u32::MAX, |s| s.end_byte - s.start_byte)
        })
        .map(|(index, _)| index)
}

/// A minified bundle's shape: 100,000 functions side by side, each holding
/// a nested one, plus overlapping, equal and outer ranges; every owner the
/// sweep names is the one the scan names, and a span no range holds has none.
#[test]
fn the_sweep_names_the_owner_a_scan_of_every_declaration_names() {
    let mut ranges = Vec::new();
    for index in 0..100_000_u32 {
        let start = index * 100;
        ranges.push((start, start + 90));
        ranges.push((start + 10, start + 50));
    }
    // An outer range, twin ranges (the first wins) and ranges that overlap
    // without nesting.
    ranges.push((0, 10_000_000));
    ranges.push((500, 590));
    ranges.push((1_000, 1_070));
    ranges.push((1_030, 1_150));
    let declarations = declarations(&ranges);
    // A deterministic spread of spans: inside the inner and outer ranges,
    // across two functions, straddling the overlapping pair, and past all.
    let mut seed = 0x2545_f491_u64;
    let mut spans = (0..300)
        .map(|_| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let start = u32::try_from((seed >> 33) % 10_100_000).expect("fits");
            (
                start,
                start + u32::try_from((seed >> 13) % 160).expect("fits"),
            )
        })
        .collect::<Vec<_>>();
    spans.extend([
        (505, 515),
        (1_040, 1_060),
        (1_040, 1_100),
        (10_000_010, 10_000_020),
    ]);
    let owners = super::super::js_oxc_arguments::innermost_owners(&declarations, &spans);
    for span in spans {
        assert_eq!(
            owners.get(&span).copied(),
            scanned(&declarations, span),
            "{span:?}"
        );
    }
    // Function 5 (index 10) and its later twin hold it equally: the first.
    assert_eq!(owners.get(&(505, 515)), Some(&10));
    assert_eq!(owners.get(&(10_000_010, 10_000_020)), None);
}

/// A caller whose work stopped gets no facts from an input whose extraction
/// outlasts one stop check: the deep-stack job is cancelled, not run out.
#[test]
fn a_stopped_caller_gets_no_facts_from_a_large_input() {
    let source = (0..30_000)
        .map(|index| {
            format!(
                "function f{index}(a,b){{var c=a+b;return g{}(c)}}",
                index % 97
            )
        })
        .collect::<String>();
    let stopped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let facts = crate::deep_stack::while_stoppable(&stopped, || {
        crate::signatures::extract_graph_facts(&source, "stopped.js")
    });
    assert!(facts.is_none());
    assert!(crate::signatures::extract_graph_facts(&source, "stopped.js").is_some());
}
