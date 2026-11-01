//! Test-only provider for proving admission independently of tree-sitter.
use super::{FactProvider, RegistryView};
use crate::num::saturating_u32;
use crate::{graph::*, text::analysis::*};

#[expect(
    clippy::expect_used,
    reason = "test-only static provider inventory has a distinct extension and nonempty identity"
)]
pub fn toml_registry() -> RegistryView<'static> {
    static PROVIDERS: [FactProvider; 1] = [FactProvider {
        id: "fixture-toml-keys",
        version: "1",
        name: "TOML conformance",
        extensions: &["toml"],
        capabilities: crate::types::GrammarFactCapabilities {
            declarations: true,
            calls: false,
            imports: false,
            exports: false,
            heritage: false,
        },
        extract: extract_keys,
    }];
    RegistryView::new(super::all_entries(), &PROVIDERS).expect("fixture has distinct extension")
}
fn extract_keys(
    content: &str,
    file: &str,
    operation: AnalysisOperation,
) -> Result<Option<GraphFactsDocument>, SourceError> {
    let (parsed, errors) = toml::de::DeTable::parse_recoverable(content);
    if !errors.is_empty() {
        return Ok(None);
    }
    let mut facts = GraphFactsDocument {
        kind: "graphFacts".into(),
        schema_version: crate::signatures::GRAPH_FACTS_SCHEMA_VERSION,
        source: "native-ast".into(),
        language: "TOML conformance".into(),
        file: file.into(),
        source_digest: crate::digest::sha256(content.as_bytes()),
        source_templates: SourceTemplates::new(
            content,
            Producer::new("fixture-toml-keys", "1"),
            operation,
        )?,
        ..Default::default()
    };
    let position = |byte: usize| {
        let prefix = &content[..byte];
        GraphPosition {
            line: saturating_u32(prefix.bytes().filter(|b| *b == b'\n').count()),
            character: saturating_u32(
                prefix
                    .rsplit('\n')
                    .next()
                    .unwrap_or_default()
                    .encode_utf16()
                    .count(),
            ),
        }
    };
    for (key, value) in parsed.get_ref() {
        let selected = key.span();
        let end = value.span().end;
        let range = GraphRange {
            start: position(selected.start),
            end: position(end),
        };
        let selection_range = GraphRange {
            start: position(selected.start),
            end: position(selected.end),
        };
        facts.declarations.push(GraphFactDeclaration {
            id: format!("{file}:{}:{}", selected.start, selected.end),
            name: key.get_ref().to_string(),
            kind: "variable".into(),
            line: range.start.line + 1,
            range: range.clone(),
            selection_range,
            source_range: Some(ObservedSpan::root(
                saturating_u32(selected.start),
                saturating_u32(end),
            )),
            selection_byte_range: Some(ObservedSpan::root(
                saturating_u32(selected.start),
                saturating_u32(selected.end),
            )),
            parse_assessment: GraphParseAssessment::parsed(range),
            ..Default::default()
        });
    }
    Ok(Some(facts))
}
