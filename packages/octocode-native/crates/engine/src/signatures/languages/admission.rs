use super::LanguageEntry;
use crate::graph::GraphFactsDocument;
use crate::text::analysis::{AnalysisOperation, SourceError};
use crate::types::GrammarFactCapabilities;

pub type FactExtractor =
    fn(&str, &str, AnalysisOperation) -> Result<Option<GraphFactsDocument>, SourceError>;

/// A facts provider needs no tree-sitter grammar or rendered outline.
pub struct FactProvider {
    pub id: &'static str,
    pub version: &'static str,
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub capabilities: GrammarFactCapabilities,
    pub extract: FactExtractor,
}

#[derive(Clone, Copy)]
pub enum RegistryEntry<'a> {
    Grammar(&'a LanguageEntry),
    Facts(&'a FactProvider),
}
impl RegistryEntry<'_> {
    pub fn name(self) -> &'static str {
        match self {
            Self::Grammar(v) => v.name,
            Self::Facts(v) => v.name,
        }
    }
    pub fn language_id(self) -> Option<&'static str> {
        match self {
            Self::Grammar(v) => v.language_id,
            Self::Facts(_) => None,
        }
    }
    pub fn label(self) -> String {
        self.language_id()
            .map_or_else(|| self.name().to_ascii_lowercase(), str::to_owned)
    }
    pub fn capability(self) -> crate::types::GrammarCapability {
        match self {
            Self::Grammar(entry) => super::grammar_capability(entry),
            Self::Facts(entry) => crate::types::GrammarCapability {
                language: entry.name.to_owned(),
                language_id: None,
                selector_aliases: Vec::new(),
                extensions: entry.extensions.iter().map(|s| (*s).to_owned()).collect(),
                structural_search: false,
                signature_outline: false,
                syntax_tree: false,
                graph_facts: entry.capabilities.any(),
                facts: entry.capabilities,
                semantic_binding: false,
                region_adapters: Vec::new(),
                test_frameworks: Vec::new(),
                argument_references: crate::types::GrammarArgumentCapabilities {
                    supported: false,
                    forms: Vec::new(),
                    unsupported_constructs: vec!["argument extraction unavailable".to_owned()],
                },
                value_flow: false,
            },
        }
    }

    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Grammar(v) => v.extensions,
            Self::Facts(v) => v.extensions,
        }
    }
    pub fn admits(self, operation: AnalysisOperation) -> bool {
        match self {
            Self::Grammar(v) => v.admits(operation),
            Self::Facts(v) => match operation {
                AnalysisOperation::Declarations => v.capabilities.declarations,
                AnalysisOperation::GraphFacts => v.capabilities.any(),
                _ => false,
            },
        }
    }
    pub fn cache_identity(self) -> String {
        match self {
            Self::Grammar(v) => format!(
                "grammar:{}:facts:{}",
                v.name,
                crate::signatures::GRAPH_FACTS_SCHEMA_VERSION
            ),
            Self::Facts(v) => format!(
                "provider:{}:{}:facts:{}",
                v.id,
                v.version,
                crate::signatures::GRAPH_FACTS_SCHEMA_VERSION
            ),
        }
    }
}

/// Borrowed immutable admission inventory; injected inventories never mutate production state.
#[derive(Clone, Copy)]
pub struct RegistryView<'a> {
    grammars: &'a [LanguageEntry],
    providers: &'a [FactProvider],
}
impl<'a> RegistryView<'a> {
    pub fn new(
        grammars: &'a [LanguageEntry],
        providers: &'a [FactProvider],
    ) -> Result<Self, SourceError> {
        let view = Self {
            grammars,
            providers,
        };
        let mut seen = std::collections::BTreeSet::new();
        for entry in view.entries() {
            for extension in entry.extensions() {
                if extension.is_empty() || !seen.insert(extension.to_ascii_lowercase()) {
                    return Err(SourceError::InvalidMap("ambiguous registry extension"));
                }
            }
        }
        for provider in providers {
            if provider.id.is_empty() || provider.version.is_empty() || !provider.capabilities.any()
            {
                return Err(SourceError::InvalidMap("invalid facts provider"));
            }
        }
        Ok(view)
    }
    pub fn installed() -> RegistryView<'static> {
        RegistryView {
            grammars: super::all_entries(),
            providers: &[],
        }
    }
    pub fn capabilities(self) -> Vec<crate::types::GrammarCapability> {
        self.entries().map(RegistryEntry::capability).collect()
    }
    pub fn select(self, selector: &str) -> impl Iterator<Item = RegistryEntry<'a>> {
        let selector = selector.trim().trim_start_matches('.').to_owned();
        self.entries().filter(move |entry| match entry {
            RegistryEntry::Grammar(entry) => super::selects(entry, &selector),
            RegistryEntry::Facts(entry) => {
                entry.name.eq_ignore_ascii_case(&selector)
                    || entry
                        .extensions
                        .iter()
                        .any(|ext| ext.eq_ignore_ascii_case(&selector))
            }
        })
    }
    pub fn parser_for_path(self, selector: &str, path: &str) -> Option<&'static str> {
        let extension = crate::text::extension_of(path, true, "");
        self.select(selector)
            .find_map(|entry| {
                entry
                    .extensions()
                    .iter()
                    .copied()
                    .find(|ext| ext.eq_ignore_ascii_case(&extension))
            })
            .or_else(|| {
                self.select(selector).find_map(|entry| match entry {
                    RegistryEntry::Grammar(entry)
                        if entry.also_parses.contains(&extension.as_str()) =>
                    {
                        entry.extensions.first().copied()
                    }
                    _ => None,
                })
            })
            .or_else(|| {
                self.select_one(selector)
                    .and_then(|entry| entry.extensions().first().copied())
            })
    }
    pub fn entries(self) -> impl Iterator<Item = RegistryEntry<'a>> {
        self.grammars
            .iter()
            .map(RegistryEntry::Grammar)
            .chain(self.providers.iter().map(RegistryEntry::Facts))
    }
    pub fn find(self, extension: &str) -> Option<RegistryEntry<'a>> {
        self.entries()
            .find(|entry| entry.extensions().contains(&extension))
    }
    pub fn select_one(self, selector: &str) -> Option<RegistryEntry<'a>> {
        let mut selected = self.entries().filter(|entry| match entry {
            RegistryEntry::Grammar(entry) => super::names(entry, selector),
            RegistryEntry::Facts(entry) => {
                entry.name.eq_ignore_ascii_case(selector)
                    || entry
                        .extensions
                        .iter()
                        .any(|ext| ext.eq_ignore_ascii_case(selector))
            }
        });
        let entry = selected.next()?;
        selected.next().is_none().then_some(entry)
    }
    pub fn extensions(self, operation: AnalysisOperation) -> Vec<String> {
        self.entries()
            .filter(|entry| entry.admits(operation))
            .flat_map(|entry| entry.extensions().iter().map(|s| (*s).to_owned()))
            .collect()
    }
}
