use super::super::memo::{FACTS_MEMO, ParsedFacts, facts_heap_bytes};
use super::*;

impl<T> EntryEvaluator<'_, T> {
    pub(super) fn provider_facts(
        &self,
        provider: &crate::signatures::languages::FactProvider,
        admitted: &crate::text::analysis::AdmittedSource<'_>,
        relative_path: &str,
        parser: Option<&str>,
        content_digest: &str,
        operation: AnalysisOperation,
    ) -> Result<Option<(crate::graph::GraphFactsTypedEntry, bool)>, GraphScanError> {
        if let Some(entry) = self.reused_facts(admitted, relative_path, parser, content_digest) {
            return Ok(Some((entry, true)));
        }
        Ok(
            (provider.extract)(admitted.text(), relative_path, operation)
                .map_err(GraphScanError::Source)?
                .map(|facts| {
                    (
                        crate::graph::GraphFactsTypedEntry {
                            relative_path: relative_path.into(),
                            content_digest: content_digest.into(),
                            source_stamp: None,
                            facts,
                            reference_counts: Vec::new(),
                        },
                        false,
                    )
                }),
        )
    }

    pub(super) fn reused_facts(
        &self,
        admitted: &crate::text::analysis::AdmittedSource<'_>,
        relative_path: &str,
        parser: Option<&str>,
        content_digest: &str,
    ) -> Option<crate::graph::GraphFactsTypedEntry> {
        match self.source {
            FactsSource::CachedGraph(reuse) => {
                let context = super::super::GraphFactsReuseContext {
                    relative_path,
                    content_digest,
                    source_digest: admitted.text_digest(),
                    parser,
                };
                reuse(&context).filter(|entry| context.accepts(entry))
            }
            _ => None,
        }
    }

    /// The file's facts: from the caller's store (`CachedGraph`), else the
    /// process memo (`Graph` scans only), else a parse. A caller that
    /// persists facts owns their reuse, so its scan neither fills nor
    /// reads the memo.
    pub(super) fn cached_facts(
        &self,
        admitted: &crate::text::analysis::AdmittedSource<'_>,
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
        // The caller may own durable progress, so its validated checkpoint is
        // consulted even when a process-local memo also has these facts.
        let memo = matches!(self.source, FactsSource::Graph);
        let content = admitted.text();
        let checkpoint = self.reused_facts(admitted, relative_path, parser, content_digest);
        let (parsed, reused) = if checkpoint.is_none()
            && memo
            && let Some(hit) = FACTS_MEMO.get(&memo_key)
        {
            (hit, true)
        } else {
            let reused_hit = checkpoint.is_some();
            let extraction = if let Some(reused) = checkpoint {
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
            if memo {
                FACTS_MEMO.insert(memo_key, Arc::clone(&parsed));
            }
            (parsed, reused_hit)
        };
        Some((parsed, reused))
    }
}
