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
    /// Uses of a Rust item declared outside any type or trait (a free
    /// function, a constant, a type): no receiver reaches it, so a member
    /// access (`held.write_atomic(…)`) is none either.
    ItemUses,
    /// Implementations and subtypes: calling the name is none.
    Implementations,
    /// Implementations and subtypes of a Rust type or trait: only a
    /// declaration whose `impl … for` or supertrait clause names it answers
    /// (`&dyn Trait`, a bound or an import does not); a name a macro body
    /// holds may.
    Heritage,
}

impl Question {
    pub(in super::super) fn of(op: Op) -> Self {
        match op {
            Op::References | Op::Callers => Self::Uses,
            Op::Implementation | Op::Subtypes => Self::Implementations,
            _ => Self::Spelling,
        }
    }

    /// This question about the declaration of `symbol` the anchor names at
    /// zero-based `line` and UTF-16 `character` of `text` (the anchor file
    /// `path`): a uses question of a Rust item no receiver reaches asks
    /// [`Self::ItemUses`].
    pub(in super::super) fn at_anchor(
        self,
        path: &str,
        text: Option<&str>,
        (line, character): (u32, u32),
        symbol: &str,
    ) -> Self {
        // The Rust declaration the anchor names: whether it is a type, and
        // whether it is declared outside any type or trait.
        let declared = || {
            let text = text?;
            Path::new(path).extension().filter(|ext| *ext == "rs")?;
            let document = octocode_tools_shared::declarations::file_document(text, path)?;
            let Some(declaration) = document.declarations.iter().find(|declaration| {
                let range = &declaration.selection_range;
                declaration.name == symbol
                    && (range.start.line, range.start.character) <= (line, character)
                    && (line, character) <= (range.end.line, range.end.character)
            }) else {
                // A name a heritage clause holds on the anchor's line (`impl
                // Trait for Type`, a supertrait) is a type declared
                // elsewhere; whether a receiver reaches it is not read here.
                return names_supertype_on(text, path, symbol, line).then_some((true, false));
            };
            let parent = declaration
                .parent
                .as_deref()
                .and_then(|id| document.declarations.iter().find(|parent| parent.id == id));
            let is_type = octocode_engine::signatures::TYPE_DECLARATION_KINDS
                .contains(&declaration.kind.as_str());
            Some((is_type, parent.is_none_or(|parent| parent.kind == "module")))
        };
        match (self, declared()) {
            (Self::Uses, Some((_, true))) => Self::ItemUses,
            (Self::Implementations, Some((true, _))) => Self::Heritage,
            (other, _) => other,
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
        if self == Self::Heritage {
            return occurrences.in_macro_tokens() || names_in_heritage(symbol, file, text);
        }
        // Sites that are not what was asked for, by their zero-based lines.
        let narrow = occurrences
            .sites(file, text, symbol)
            .and_then(|sites| match self {
                Self::Uses | Self::ItemUses => Some(sites.declared),
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
        if self == Self::ItemUses {
            let starts = line_starts(text);
            return occurrences
                .ranges()
                .iter()
                .filter(|range| !through_receiver(text, &starts, &range.start))
                .any(|range| !other.contains(&(range.start.line as usize)));
        }
        occurrences.lines.iter().any(|line| !other.contains(line))
    }

    /// The zero-based lines of the sites `facts` holds that spell `symbol`
    /// without answering this question.
    fn other_sites(self, symbol: &str, facts: &GraphFactsDocument) -> HashSet<usize> {
        match self {
            Self::Uses | Self::ItemUses => facts
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

/// Whether a heritage clause on zero-based `line` of `text` (of `file`)
/// names `symbol` as a supertype (`impl Trait for Type`, `trait Sub: Trait`).
fn names_supertype_on(text: &str, file: &str, symbol: &str, line: u32) -> bool {
    octocode_tools_shared::declarations::named_heritage(text, file, symbol).is_some_and(
        |document| {
            document.edges.iter().any(|edge| {
                matches!(edge.relation.as_str(), "extends" | "implements")
                    && edge.line == line + 1
                    && edge.to.rsplit(['.', ':']).next() == Some(symbol)
            })
        },
    )
}

/// Whether `text` (of `file`) declares a type whose `impl … for` or
/// supertype clause names `symbol`; `true` when only the whole file could
/// tell (a macro body holds the name, or the grammar cannot read it).
fn names_in_heritage(symbol: &str, file: &str, text: &str) -> bool {
    octocode_tools_shared::declarations::named_heritage(text, file, symbol).is_none_or(|document| {
        document.edges.iter().any(|edge| {
            matches!(edge.relation.as_str(), "extends" | "implements")
                && edge.to.rsplit(['.', ':']).next() == Some(symbol)
        })
    })
}

/// The byte offset each line of `text` starts at.
fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect()
}

/// Whether the name at `start` (zero-based line, UTF-16 character) is a
/// member of a receiver: `.` before it, past whitespace and line breaks
/// (`held\n    .write_atomic(…)`), and no range `..`.
fn through_receiver(
    text: &str,
    starts: &[usize],
    start: &octocode_engine::lsp::types::LspExactPosition,
) -> bool {
    let Some(line_start) = starts.get(start.line as usize).copied() else {
        return false;
    };
    let mut units = 0u32;
    let at = text
        .get(line_start..)
        .unwrap_or_default()
        .char_indices()
        .find_map(|(offset, ch)| {
            let here = (units >= start.character).then_some(line_start + offset);
            units += u32::try_from(ch.len_utf16()).unwrap_or(1);
            here
        })
        .unwrap_or(text.len());
    let before = text.get(..at).unwrap_or_default().trim_end();
    before.ends_with('.') && !before.ends_with("..")
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
