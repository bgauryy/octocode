//! The compile-time caches of a pattern (root, `declares`, required
//! literal) answer exactly what the per-candidate computations did.

use super::{CompiledPattern, declares, effective_pattern_root};
use crate::signatures::languages;
use crate::structural::language::AgLanguage;
use crate::structural::octo::compile_matcher;
use crate::structural::octo::tests::all_matches;
use crate::structural::query::StructuralQuery;

/// What a pattern counts of its own matching.
#[derive(Default)]
pub(super) struct Probe {
    /// Candidates that reached the structural comparison.
    pub(super) compared: std::sync::atomic::AtomicUsize,
}

/// Switches the pre-reject off, for the differential test.
pub(super) static SKIP_PREREJECT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// The two tests that flip the process-wide switch run one at a time.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lang(ext: &str) -> AgLanguage {
    AgLanguage::new(languages::find_entry(ext).expect("test language should exist"))
}

/// Every match of `pattern` in `src` as comparable text.
fn found(ext: &str, pattern: &str, src: &str) -> Vec<String> {
    let matcher = compile_matcher(
        &lang(ext),
        &StructuralQuery::new(Some(pattern), None).expect("query"),
    )
    .expect("compile pattern");
    all_matches(&matcher, src)
        .expect("complete execution")
        .into_iter()
        .map(|m| {
            let m = m.matched;
            let mut metavars: Vec<_> = m.metavars.iter().collect();
            metavars.sort();
            format!(
                "{}:{}-{}:{} {:?} {metavars:?}",
                m.start_line, m.start_col, m.end_line, m.end_col, m.text
            )
        })
        .collect()
}

#[test]
fn cached_root_and_declares_equal_the_walks() {
    let cases = [
        ("rs", "$X.unwrap()"),
        ("rs", "fn $F($$$A) { $$$B }"),
        ("rs", "impl $T for $U {}"),
        ("rs", "struct $S;"),
        ("ts", "foo($X)"),
        ("ts", "class $C { $$$ }"),
        ("ts", "const $A = $B"),
        ("py", "def $F($$$A): $$$B"),
        ("go", "func $F($$$A) $R { $$$B }"),
        ("java", "class $C { void $M() {} }"),
        ("c", "int $A = $B;"),
    ];
    for (ext, text) in cases {
        let Ok(pattern) = CompiledPattern::new(&lang(ext), text) else {
            continue;
        };
        let tree = pattern.tree.as_ref().expect("an ordinary pattern");
        let slow = effective_pattern_root(tree.root_node(), &pattern.source);
        if let Some(cached) = pattern.root() {
            assert_eq!(cached.id(), slow.id(), "{ext} {text}");
        }
        let mut pending = vec![tree.root_node()];
        while let Some(node) = pending.pop() {
            assert_eq!(
                pattern.pattern_declares(node),
                declares(node),
                "{ext} {text}"
            );
            pending.extend(super::children(node));
        }
    }
}

/// A candidate without the pattern's identifier never reaches the structural
/// comparison.
#[test]
fn a_candidate_without_the_required_literal_is_not_compared() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    use crate::structural::octo::matching::{CaptureEnv, Deadline, visit_named};
    use std::sync::atomic::Ordering::Relaxed;
    let src = "fn main() {\n    a.b();\n    c.d(1);\n    e.f(2).g();\n    x.unwrap();\n    y.z.unwrap();\n}\n";
    let pattern = CompiledPattern::new(&lang("rs"), "$X.unwrap()").expect("pattern compiles");
    let tree =
        super::parse_tree_with_deadline(pattern.language(), src, Deadline::UNBOUNDED.parse_by())
            .expect("parse");
    let run = || {
        let mut hits = Vec::new();
        visit_named(tree.root_node(), Deadline::UNBOUNDED, &mut |candidate| {
            if pattern.matches_candidate(candidate)
                && pattern.matches(
                    candidate,
                    src,
                    &mut CaptureEnv::default(),
                    Deadline::UNBOUNDED,
                )?
            {
                hits.push(candidate.byte_range());
            }
            Ok(())
        })
        .expect("walk");
        hits
    };
    let hits = run();
    let compared = pattern.probe.compared.swap(0, Relaxed);
    assert_eq!(hits.len(), 2);
    assert_eq!(
        compared, 2,
        "only the two calls that spell unwrap are compared"
    );
    SKIP_PREREJECT.store(true, Relaxed);
    let all = run();
    SKIP_PREREJECT.store(false, Relaxed);
    let unfiltered = pattern.probe.compared.load(Relaxed);
    assert_eq!(hits, all);
    assert!(unfiltered > compared, "{unfiltered} > {compared}");
}

/// The same matches with and without the pre-reject over the source of every
/// crate, macro bodies included.
#[test]
fn pre_reject_keeps_the_match_set_over_the_crates() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    let mut stack = vec![crates];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    let sources: Vec<String> = files
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .filter(|text| text.contains("macro_rules!") || text.contains("unwrap"))
        .step_by(3)
        .take(120)
        .collect();
    assert!(sources.len() > 40);
    let patterns = [
        "$X.unwrap()",
        "$A.expect($B)",
        "std::mem::take($X)",
        "macro_rules! $N { $$$ }",
        "fn $F($$$A) -> $R { $$$B }",
        "impl $T for $U { $$$ }",
    ];
    let mut total = 0;
    for pattern in patterns {
        for src in &sources {
            let on = found("rs", pattern, src);
            SKIP_PREREJECT.store(true, std::sync::atomic::Ordering::Relaxed);
            let off = found("rs", pattern, src);
            SKIP_PREREJECT.store(false, std::sync::atomic::Ordering::Relaxed);
            assert_eq!(on, off, "{pattern}");
            total += on.len();
        }
    }
    assert!(total > 100, "{total}");
}
