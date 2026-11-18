use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};

pub static PARSES: LazyLock<Mutex<HashMap<(&'static str, String), usize>>> =
    LazyLock::new(Mutex::default);

pub(super) fn note(analysis: &'static str, path: &str) {
    *PARSES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry((analysis, path.to_owned()))
        .or_default() += 1;
}

/// Extractions of `path` by the analysis named `analysis`
/// (`declarations`, `graphFacts`, `moduleReferences`, `namedHeritage`).
pub fn of(analysis: &'static str, path: &str) -> usize {
    PARSES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&(analysis, path.to_owned()))
        .copied()
        .unwrap_or(0)
}

/// Whole-file facts parses ([`super::graph_facts`]) of `path`.
pub fn facts_of(path: &str) -> usize {
    of("graphFacts", path)
}
