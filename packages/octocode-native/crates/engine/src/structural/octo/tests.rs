use super::matching::{parse_tree, visit_named};
use super::*;
use crate::signatures::languages;
use crate::structural::types::StructuralMatch;
use serde_json::{Value, json};
use std::time::Instant;

thread_local! {
    /// Interrupts this thread's next compile-time parse (and a deep-stack
    /// run of it): `matching::parse_tree` reads it.
    pub(in crate::structural) static INTERRUPT_NEXT_COMPILE_PARSE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Rules this thread decoded (`rule::decode_rule`).
    pub(super) static RULE_DECODE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Ancestors an `inside` relation read in this thread (a deep-stack run
    /// adds its own): the work its answers cost, so a test asserts how it
    /// grows with depth instead of timing it.
    pub(super) static ANCESTOR_STEPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Rule evaluations (`CompiledRule::matches`) this thread ran, for
    /// tests that measure the work of a walk.
    pub(super) static EVALUATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Every match `run` finds in `content`, materialized.
pub(super) fn all_matches(
    run: &OctoCompiledMatcher,
    content: &str,
) -> Result<Vec<MatchWithKind>, ExecutionError> {
    matches_with_recovery(run, content).map(|(matches, _)| matches)
}

/// Every engine test audits the literal prefilter: a source in which `run`
/// finds a match must hold what the query's prefilter requires, or a
/// directory scan would skip that file and drop the match. A violation
/// aborts the test binary (a panic here could be caught as a failed
/// search), naming the query.
pub(super) fn prefilter_audit(
    query: &StructuralQuery<'_>,
    run: OctoCompiledMatcher,
) -> OctoCompiledMatcher {
    let prefilter = query.prefilter();
    let Ok(Some(filter)) = crate::structural::files::anchors::AnchorFilter::new(&prefilter) else {
        return run;
    };
    let source = query.explanation_with_prefilter(&prefilter).source;
    Box::new(move |content, found| {
        let before = found.pushed();
        run(content, found)?;
        if found.pushed() > before && !filter.admits(content.as_bytes()) {
            use std::io::Write as _;
            let _ = writeln!(
                std::io::stderr(),
                "prefilter audit: {prefilter:?} skips a source where {source} matches:\n{content}"
            );
            std::process::abort();
        }
        Ok(())
    })
}

fn run_pattern(src: &str, ext: &str, pattern: &str) -> Vec<StructuralMatch> {
    let matcher = compile_matcher(
        &lang(ext),
        &StructuralQuery::new(Some(pattern), None).expect("query"),
    )
    .expect("compile pattern");
    all_matches(&matcher, src)
        .expect("complete execution")
        .into_iter()
        .map(|m| m.matched)
        .collect()
}

fn run_rule(src: &str, ext: &str, rule: impl std::borrow::Borrow<Value>) -> Vec<StructuralMatch> {
    let matcher = compile_matcher(
        &lang(ext),
        &StructuralQuery::new(None, Some(rule.borrow())).expect("query"),
    )
    .expect("compile rule");
    all_matches(&matcher, src)
        .expect("complete execution")
        .into_iter()
        .map(|m| m.matched)
        .collect()
}

fn lang(ext: &str) -> AgLanguage {
    AgLanguage::new(languages::find_entry(ext).expect("test language should exist"))
}

mod grammars;
mod limits;
mod macros;
mod owned_core;
mod patterns;
#[cfg(feature = "tree-sitter-cpp")]
mod recovery;
mod rule_features;
mod rules;
