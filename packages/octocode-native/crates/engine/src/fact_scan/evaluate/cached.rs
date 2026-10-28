use super::super::memo::{FACTS_MEMO, ParsedFacts, facts_heap_bytes};
use super::*;

impl<T> EntryEvaluator<'_, T> {
    pub(super) fn cached_facts(
        &self,
        content: &str,
        relative_path: &str,
        parser: Option<&str>,
        selected_entry: RegistryEntry<'_>,
        content_digest: &str,
    ) -> Option<(Arc<ParsedFacts>, bool)> {
        let memo_key = format!(
            "{content_digest}\u{0}{relative_path}\u{0}{}\u{0}{}",
            parser.unwrap_or(""),
            selected_entry.cache_identity()
        );
        let (parsed, reused) = if let Some(hit) = FACTS_MEMO.get(&memo_key) {
            (hit, true)
        } else {
            let reused = match self.source {
                FactsSource::CachedGraph(reuse) => reuse(relative_path, content_digest, parser)
                    .filter(|entry| {
                        entry.relative_path == *relative_path
                            && entry.content_digest == *content_digest
                            && entry.facts.file == *relative_path
                            && entry.facts.source_digest
                                == crate::digest::sha256(content.as_bytes())
                            && entry.facts.schema_version
                                == crate::signatures::GRAPH_FACTS_SCHEMA_VERSION
                    }),
                _ => None,
            };
            let reused_hit = reused.is_some();
            let extraction = if let Some(reused) = reused {
                Some(crate::signatures::GraphFactsExtraction {
                    facts: reused.facts,
                    reference_counts: reused.reference_counts,
                })
            } else if let Some(extension) = parser {
                crate::signatures::graph_facts::extract_graph_facts_with_metadata_with_extension(
                    content,
                    relative_path,
                    extension,
                )
            } else {
                crate::signatures::extract_graph_facts_with_metadata_inner(content, relative_path)
            };
            let extraction = extraction?;
            let weight = facts_heap_bytes(&extraction.facts, &extraction.reference_counts);
            let parsed = Arc::new(ParsedFacts {
                facts: extraction.facts,
                reference_counts: extraction.reference_counts,
                weight: u32::try_from(weight).unwrap_or(u32::MAX),
            });
            FACTS_MEMO.insert(memo_key, Arc::clone(&parsed));
            (parsed, reused_hit)
        };
        Some((parsed, reused))
    }
}
