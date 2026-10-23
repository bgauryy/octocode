//! The parsed-facts cache: one file's facts per content version, weighed
//! by their estimated heap.

use std::sync::Arc;

use crate::graph::{
    GraphFactCall, GraphFactCommonJs, GraphFactDeclaration, GraphFactEdge, GraphFactExport,
    GraphFactImport, GraphFactPath, GraphFactRustModule, GraphFactsDocument,
};

/// Graph facts of one parsed file.
pub(super) struct ParsedFacts {
    pub(super) facts: GraphFactsDocument,
    pub(super) reference_counts: Vec<crate::types::GraphReferenceCount>,
    /// Estimated heap bytes of the facts, the cache weight.
    pub(super) weight: u32,
}

/// Estimated heap bytes of one file's facts: every row's own size plus its
/// strings. A file's facts outweigh its source several times (ids, ranges,
/// edge and call rows), so the source size would let the cache hold far more
/// than its capacity names.
pub(super) fn facts_heap_bytes(
    facts: &GraphFactsDocument,
    reference_counts: &[crate::types::GraphReferenceCount],
) -> usize {
    use std::mem::size_of;
    let text = |value: &str| value.len();
    let option = |value: &Option<String>| value.as_deref().map_or(0, str::len);
    let list = |values: &[String]| {
        values
            .iter()
            .map(|value| size_of::<String>() + value.len())
            .sum::<usize>()
    };
    let option_list = |values: &Option<Vec<String>>| values.as_deref().map_or(0, list);
    let mut bytes =
        text(&facts.kind) + text(&facts.source) + text(&facts.language) + text(&facts.file);
    bytes += facts
        .declarations
        .iter()
        .map(|d| {
            size_of::<GraphFactDeclaration>()
                + text(&d.id)
                + text(&d.name)
                + text(&d.kind)
                + list(&d.exported_as)
                + option(&d.parent)
        })
        .sum::<usize>();
    bytes += facts
        .imports
        .iter()
        .map(|i| {
            size_of::<GraphFactImport>()
                + text(&i.id)
                + text(&i.specifier)
                + text(&i.import_kind)
                + option(&i.local_name)
                + option(&i.imported_name)
                + option(&i.resolution_hint)
                + option_list(&i.module_scope)
        })
        .sum::<usize>();
    bytes += facts
        .exports
        .iter()
        .map(|e| {
            size_of::<GraphFactExport>()
                + text(&e.id)
                + text(&e.name)
                + text(&e.export_kind)
                + option(&e.local_name)
                + option(&e.source)
        })
        .sum::<usize>();
    bytes += facts
        .calls
        .iter()
        .map(|c| {
            size_of::<GraphFactCall>()
                + text(&c.id)
                + text(&c.caller)
                + option(&c.caller_id)
                + text(&c.callee)
                + text(&c.kind)
                + option(&c.receiver_type)
        })
        .sum::<usize>();
    bytes += facts
        .common_js
        .iter()
        .map(|c| {
            size_of::<GraphFactCommonJs>()
                + option(&c.specifier)
                + text(&c.kind)
                + text(&c.binding)
                + option(&c.reason)
        })
        .sum::<usize>();
    bytes += facts
        .edges
        .iter()
        .map(|e| {
            size_of::<GraphFactEdge>()
                + text(&e.id)
                + text(&e.from)
                + text(&e.to)
                + text(&e.relation)
                + text(&e.source)
                + text(&e.resolution)
        })
        .sum::<usize>();
    bytes += facts
        .modules
        .iter()
        .map(|m| {
            size_of::<GraphFactRustModule>() + text(&m.name) + list(&m.scope) + option(&m.path)
        })
        .sum::<usize>();
    bytes += facts
        .paths
        .iter()
        .map(|p| size_of::<GraphFactPath>() + text(&p.path) + option(&p.caller_id))
        .sum::<usize>();
    bytes += facts
        .diagnostics
        .iter()
        .map(|d| size_of::<crate::graph::GraphFactDiagnostic>() + text(&d.message))
        .sum::<usize>();
    bytes += facts.error_lines.len() * size_of::<[u32; 2]>();
    bytes += reference_counts
        .iter()
        .map(|r| size_of::<crate::types::GraphReferenceCount>() + text(&r.declaration_id))
        .sum::<usize>();
    bytes
}

/// Parsed facts keyed by (content digest, relative path, parser override):
/// a rescan re-reads and hashes every file but re-parses only changed ones.
/// Content-addressed, so an edited file can never hit a stale entry.
pub(super) static FACTS_MEMO: std::sync::LazyLock<moka::sync::Cache<String, Arc<ParsedFacts>>> =
    std::sync::LazyLock::new(|| {
        moka::sync::Cache::builder()
            .max_capacity(FACTS_MEMO_BYTES)
            .weigher(|_: &String, parsed: &Arc<ParsedFacts>| parsed.weight)
            .time_to_idle(std::time::Duration::from_secs(1800))
            .build()
    });

/// Estimated heap bytes the parsed-facts cache may hold.
const FACTS_MEMO_BYTES: u64 = 128 * 1024 * 1024;
