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

    /// The file's facts, freshly extracted unless the caller supplies a
    /// checkpoint validated against the admitted source.
    pub(super) fn extract_facts(
        &self,
        admitted: &crate::text::analysis::AdmittedSource<'_>,
        relative_path: &str,
        parser: Option<&str>,
        content_digest: &str,
    ) -> Option<(crate::signatures::GraphFactsExtraction, bool)> {
        let content = admitted.text();
        let checkpoint = self.reused_facts(admitted, relative_path, parser, content_digest);
        let reused = checkpoint.is_some();
        let extraction = if let Some(checkpoint) = checkpoint {
            Some(crate::signatures::GraphFactsExtraction {
                facts: checkpoint.facts,
                reference_counts: checkpoint.reference_counts,
            })
        } else if let Some(extension) = parser {
            crate::signatures::extract_graph_facts_parsed_as(content, relative_path, extension)
        } else {
            crate::signatures::extract_graph_facts_with_metadata_inner(content, relative_path)
        };
        Some((extraction?, reused))
    }
}
