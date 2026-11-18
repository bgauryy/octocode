//! What a request's question makes of a file that spells the name but is not
//! in the answer: a word in code is no missed site if it cannot be what was
//! asked for.
use super::*;
use crate::lsp_search::Op;
use octocode_engine::graph::GraphFactsDocument;
use octocode_engine::lsp::resolver::CodeOccurrences;
use std::collections::HashSet;

/// What an incoming request asks for, by the sites that would answer it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in super::super) enum Question {
    /// Nothing narrower than the spelling is known.
    #[default]
    Spelling,
    /// Uses (`references`, `callers`): declaring the same name is none, as
    /// an override is no call of what it overrides.
    Uses,
    /// Implementations and subtypes: calling the name is none.
    Implementations,
}

impl Question {
    pub(in super::super) fn of(op: Op) -> Self {
        match op {
            Op::References | Op::Callers => Self::Uses,
            Op::Implementation | Op::Subtypes => Self::Implementations,
            _ => Self::Spelling,
        }
    }

    /// Whether code spelling `symbol` at `occurrences` of `file` may answer
    /// this question. A file the grammar cannot read may.
    pub(super) fn may_answer(
        self,
        symbol: &str,
        file: &str,
        text: &str,
        occurrences: &CodeOccurrences,
    ) -> bool {
        if self == Self::Spelling {
            return true;
        }
        // Sites that are not what was asked for, by their zero-based lines.
        let narrow = occurrences
            .sites(file, text, symbol)
            .and_then(|sites| match self {
                Self::Uses => Some(sites.declared),
                _ => sites.called,
            });
        let other: HashSet<usize> = match narrow {
            Some(lines) => lines.into_iter().collect(),
            None => {
                let Some(facts) = octocode_tools_shared::declarations::graph_facts(text, file)
                else {
                    return true;
                };
                self.other_sites(symbol, &facts)
            }
        };
        occurrences.lines.iter().any(|line| !other.contains(line))
    }

    /// The zero-based lines of the sites `facts` holds that spell `symbol`
    /// without answering this question.
    fn other_sites(self, symbol: &str, facts: &GraphFactsDocument) -> HashSet<usize> {
        match self {
            Self::Uses => facts
                .declarations
                .iter()
                .filter(|declaration| declaration.name == symbol)
                .map(|declaration| declaration.selection_range.start.line as usize)
                .collect(),
            _ => facts
                .calls
                .iter()
                .filter(|call| {
                    let (_, name) = call
                        .callee
                        .rsplit_once(['.', ':'])
                        .unwrap_or(("", &call.callee));
                    name == symbol
                })
                .flat_map(|call| call.range.start.line as usize..=call.range.end.line as usize)
                .collect(),
        }
    }
}

/// The zero-based lines of `file` where code declares `symbol` and where it
/// calls it, from the parse `occurrences` came from where it can tell, else
/// the file's facts; `None` when neither reads the file.
pub(super) fn declared_and_called(
    symbol: &str,
    file: &str,
    text: &str,
    occurrences: &CodeOccurrences,
) -> Option<(HashSet<usize>, HashSet<usize>)> {
    let sites = occurrences.sites(file, text, symbol);
    let facts = || octocode_tools_shared::declarations::graph_facts(text, file);
    let declared = match sites.as_ref() {
        Some(sites) => sites.declared.iter().copied().collect(),
        None => Question::Uses.other_sites(symbol, &*facts()?),
    };
    let called = match sites.and_then(|sites| sites.called) {
        Some(called) => called.into_iter().collect(),
        None => Question::Implementations.other_sites(symbol, &*facts()?),
    };
    Some((declared, called))
}

impl Scope {
    /// Record what this request asks for; [`Self::text_only_files`] counts
    /// only the files that may answer it.
    pub(in super::super) fn asks(&self, question: Question) {
        let _ = self.question.set(question);
    }
}
