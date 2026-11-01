//! The parsed-facts cache: one file's facts per content version, weighed
//! by their estimated heap.

use std::sync::Arc;

use crate::graph::{
    GraphFactCall, GraphFactCommonJs, GraphFactCommonJsBinding, GraphFactDeclaration,
    GraphFactEdge, GraphFactExport, GraphFactImport, GraphFactPath, GraphFactRustModule,
    GraphFactsDocument,
};

/// Graph facts of one parsed file.
pub(super) struct ParsedFacts {
    pub(super) facts: GraphFactsDocument,
    pub(super) reference_counts: Vec<crate::graph::GraphReferenceCount>,
    /// Estimated heap bytes of the facts, the cache weight.
    pub(super) weight: u32,
}

/// Estimated heap bytes of one file's facts: every row's own size plus its
/// strings. A file's facts outweigh its source several times (ids, ranges,
/// edge and call rows), so the source size would let the cache hold far more
/// than its capacity names.
pub(super) fn facts_heap_bytes(
    facts: &GraphFactsDocument,
    reference_counts: &[crate::graph::GraphReferenceCount],
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
    let mut bytes = text(&facts.kind)
        + text(&facts.source)
        + text(&facts.language)
        + text(&facts.file)
        + text(&facts.source_digest)
        + facts.source_templates.heap_bytes();
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
                + d.test_identity.as_ref().map_or(0, |test| match test {
                    crate::graph::GraphTestIdentity::Googletest { suite, name, .. } => {
                        suite.len() + name.len()
                    }
                    crate::graph::GraphTestIdentity::Rust { name, .. } => name.len(),
                })
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
                + option(&i.scope_id)
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
                + option(&c.self_template)
        })
        .sum::<usize>();
    bytes += (facts.calls.capacity() - facts.calls.len()) * size_of::<GraphFactCall>();
    bytes += facts
        .common_js
        .iter()
        .map(|c| {
            size_of::<GraphFactCommonJs>()
                + option(&c.specifier)
                + text(&c.kind)
                + text(&c.binding)
                + option(&c.reason)
                + c.bindings.capacity() * size_of::<GraphFactCommonJsBinding>()
                + c.bindings
                    .iter()
                    .map(|b| text(&b.local) + option(&b.imported) + text(&b.declaration_id))
                    .sum::<usize>()
        })
        .sum::<usize>();
    bytes += facts
        .argument_sites
        .iter()
        .map(crate::graph::GraphFactArgumentSite::heap_bytes)
        .sum::<usize>();
    bytes += (facts.argument_sites.capacity() - facts.argument_sites.len())
        * size_of::<crate::graph::GraphFactArgumentSite>();
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
    bytes += (facts.error_lines.len() + facts.test_lines.len()) * size_of::<[u32; 2]>();
    bytes += reference_counts
        .iter()
        .map(|r| size_of::<crate::graph::GraphReferenceCount>() + text(&r.declaration_id))
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

#[cfg(test)]
mod argument_memory_tests {
    use super::*;
    use crate::graph::{
        GraphArgumentGroup, GraphArgumentGroupObservation, GraphArgumentSlot, GraphFactArgumentSite,
    };

    #[test]
    fn argument_vectors_include_spare_allocation_capacity() {
        let mut facts = crate::signatures::extract_graph_facts_with_metadata_inner(
            "consume(handler)",
            "fixture.ts",
        )
        .expect("facts")
        .facts;
        let before = facts_heap_bytes(&facts, &[]);
        let capacity = facts.argument_sites.capacity();
        facts.argument_sites.reserve(20);
        let expected = (facts.argument_sites.capacity() - capacity)
            * std::mem::size_of::<GraphFactArgumentSite>();
        assert_eq!(facts_heap_bytes(&facts, &[]) - before, expected);
        let site = &mut facts.argument_sites[0];
        let before = site.heap_bytes();
        let capacity = site.groups.capacity();
        site.groups.reserve(20);
        let expected =
            (site.groups.capacity() - capacity) * std::mem::size_of::<GraphArgumentGroup>();
        assert_eq!(site.heap_bytes() - before, expected);
        let before = site.heap_bytes();
        let GraphArgumentGroupObservation::Slots { slots } = &mut site.groups[0].observation else {
            panic!("slots")
        };
        let capacity = slots.capacity();
        slots.reserve(20);
        let expected = (slots.capacity() - capacity) * std::mem::size_of::<GraphArgumentSlot>();
        assert_eq!(site.heap_bytes() - before, expected);
    }
}

#[cfg(test)]
mod call_memory_tests {
    use super::*;

    #[test]
    fn constructor_admission_and_call_spare_capacity_are_accounted() {
        let mut facts = crate::signatures::extract_graph_facts_with_metadata_inner(
            "class C { C(){ super(); } }",
            "fixture.java",
        )
        .expect("facts")
        .facts;
        assert_eq!(facts.calls.len(), 1);
        let before = facts_heap_bytes(&facts, &[]);
        let capacity = facts.calls.capacity();
        facts.calls.reserve(20);
        assert_eq!(
            facts_heap_bytes(&facts, &[]) - before,
            (facts.calls.capacity() - capacity) * std::mem::size_of::<GraphFactCall>()
        );
        let before = facts_heap_bytes(&facts, &[]);
        facts.calls[0].binding = crate::graph::GraphCallBinding::ConstructedType;
        assert_eq!(
            facts_heap_bytes(&facts, &[]),
            before,
            "admission owns no heap buffers"
        );
    }
}
