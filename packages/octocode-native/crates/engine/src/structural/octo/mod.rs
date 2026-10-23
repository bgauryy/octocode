use std::collections::HashMap;
use std::time::Instant;

use crate::signatures::extractor::AST_EXECUTION_TIMEOUT;

use super::language::AgLanguage;
use super::query::StructuralQuery;

mod error;
mod line_index_support;
mod macro_bodies;
mod matching;
mod metavars;
mod pattern;
mod position;
mod recovery;
mod rule;
mod selector;

pub(super) use error::CompileError;
pub(super) use matching::{ExecutionError, named_children, node_text, parse_tree_with_deadline};
pub use rule::{EXAMPLE_RULE, validate_rule_shape};
pub(super) use rule::{RawPattern, RawRule, decode_rule};

#[cfg(test)]
pub(super) use matching::INTERRUPT_NEXT_COMPILE_PARSE;

use super::query::Prefilter;
use crate::text::utf8_offsets::LineIndex;
pub use error::COMPILE_FAILED;
use line_index_support::{to_structural_match, to_structural_match_with_index};
use macro_bodies::MacroBodies;
use matching::{CandidateVisitor, CaptureEnv, collect_kind_matches, visit_named_expanding_macros};
pub(super) use matching::{Found, MatchWithKind};
pub use matching::{MATCH_DEADLINE, PARSE_INTERRUPTED};
use pattern::CompiledPattern;
pub use pattern::PatternDeclaration;
use recovery::Recovery;
pub use rule::RULE_INVALID;
use rule::{CompiledRule, Document};

#[cfg(test)]
use matching::{SECONDARY_CAPTURE, parse_tree, visit_named};
#[cfg(test)]
use rule::RULE_DECODE_COUNT;

#[cfg(test)]
use super::types::StructuralMatch;

/// A compiled query: matches one source into what the caller's [`Found`]
/// keeps.
pub(super) type OctoCompiledMatcher =
    Box<dyn Fn(&str, &mut Found<'_>) -> Result<(), ExecutionError> + Send + Sync>;

/// Every match `run` finds in `content`, materialized.
#[cfg(test)]
pub(super) fn all_matches(
    run: &OctoCompiledMatcher,
    content: &str,
) -> Result<Vec<MatchWithKind>, ExecutionError> {
    matches_with_recovery(run, content).map(|(matches, _)| matches)
}

/// [`all_matches`], and whether the source parsed with recovered syntax that
/// could hide a match ([`Found::recovered`]).
pub(super) fn matches_with_recovery(
    run: &OctoCompiledMatcher,
    content: &str,
) -> Result<(Vec<MatchWithKind>, bool), ExecutionError> {
    let mut found = Found::new(super::types::StructuralDemand::Matches);
    run(content, &mut found)?;
    let recovered = found.recovered();
    Ok((found.into_matches(), recovered))
}

/// The declaration `pattern` spells in `lang` ([`CompiledPattern::declaration`]).
pub(super) fn pattern_declaration(lang: &AgLanguage, pattern: &str) -> Option<PatternDeclaration> {
    CompiledPattern::new(lang, pattern).ok()?.declaration()
}

pub(super) fn compile_matcher(
    lang: &AgLanguage,
    query: &StructuralQuery<'_>,
) -> Result<OctoCompiledMatcher, CompileError> {
    let language = lang.tree_sitter_language();
    let anchors = match query.prefilter() {
        Prefilter::None => None,
        Prefilter::Single(anchor) => Some(vec![anchor]),
        Prefilter::Union(anchors) => Some(anchors),
    };
    let recovery = Recovery::new(query.recovery_anchors());
    let macros = MacroBodies::for_language(lang, anchors);
    match query.parts() {
        (Some(pattern), None) if is_document_probe(pattern) => {
            Ok(Box::new(move |content, found| {
                let deadline = match_deadline(content);
                let tree = parse_tree_with_deadline(&language, content, deadline)?;
                let root = tree.root_node();
                found.push(root, || {
                    to_structural_match(root, content, HashMap::new(), HashMap::new())
                });
                Ok(())
            }))
        }
        (Some(pattern), None) => {
            let compiled = CompiledPattern::new(lang, pattern)?;
            Ok(Box::new(move |content, found| {
                let deadline = match_deadline(content);
                if compiled.is_special() {
                    return compiled.find_special_matches(content, deadline, &recovery, found);
                }

                let tree = parse_tree_with_deadline(compiled.language(), content, deadline)?;
                if recovery.hides_match(&tree, content) {
                    found.mark_recovered();
                }
                // Built on the first match: most parsed files have none.
                let line_index = std::cell::OnceCell::new();
                let visit: &mut CandidateVisitor<'_> = &mut |candidate, _, _| {
                    if !compiled.matches_candidate(candidate) {
                        return Ok(());
                    }
                    let mut captures = CaptureEnv::default();
                    if compiled.matches(candidate, content, &mut captures, deadline)? {
                        found.push(candidate, || {
                            let (values, ranges) = captures.into_maps();
                            to_structural_match_with_index(
                                candidate,
                                content,
                                line_index.get_or_init(|| LineIndex::tree_sitter(content)),
                                values,
                                ranges,
                            )
                        });
                    }
                    Ok(())
                };
                visit_named_expanding_macros(
                    tree.root_node(),
                    content,
                    compiled.language(),
                    macros.as_ref(),
                    deadline,
                    visit,
                )
            }))
        }
        (None, Some(_)) => {
            let compiled = CompiledRule::compile(lang, query.parsed_rule()?)?;
            if let Some(kind) = compiled.simple_kind().map(str::to_owned) {
                return Ok(Box::new(move |content, found| {
                    let deadline = match_deadline(content);
                    let tree = parse_tree_with_deadline(&language, content, deadline)?;
                    if recovery.hides_match(&tree, content) {
                        found.mark_recovered();
                    }
                    collect_kind_matches(
                        tree.root_node(),
                        &kind,
                        content,
                        &language,
                        macros.as_ref(),
                        deadline,
                        found,
                    )
                }));
            }
            Ok(Box::new(move |content, found| {
                let deadline = match_deadline(content);
                let tree = parse_tree_with_deadline(&language, content, deadline)?;
                if recovery.hides_match(&tree, content) {
                    found.mark_recovered();
                }
                let line_index = LineIndex::tree_sitter(content);
                visit_named_expanding_macros(
                    tree.root_node(),
                    content,
                    &language,
                    macros.as_ref(),
                    deadline,
                    &mut |candidate, ancestors, root| {
                        if !compiled.matches_candidate(candidate) {
                            return Ok(());
                        }
                        let document = Document {
                            content,
                            deadline,
                            root,
                            line_index: &line_index,
                        };
                        let mut captures = CaptureEnv::default();
                        if compiled.matches(candidate, Some(ancestors), &document, &mut captures)? {
                            found.push(candidate, || {
                                let (values, ranges) = captures.into_maps();
                                to_structural_match_with_index(
                                    candidate,
                                    content,
                                    &line_index,
                                    values,
                                    ranges,
                                )
                            });
                        }
                        Ok(())
                    },
                )
            }))
        }
        _ => unreachable!("StructuralQuery validates the query shape"),
    }
}

/// How long matching may take on a source of `len` bytes: the shared parse
/// budget, plus one more per whole MiB, so a large source (the parse limit
/// is 8 MiB) is not cut nondeterministically by a small file's budget. A cut
/// match discards the file's matches, and no page resumes inside a file.
pub(super) fn match_budget(len: usize) -> std::time::Duration {
    let extra = u32::try_from(len >> 20).unwrap_or(u32::MAX);
    AST_EXECUTION_TIMEOUT.saturating_mul(extra.saturating_add(1))
}

/// The match deadline of `content`, from now (see [`match_budget`]).
pub(super) fn match_deadline(content: &str) -> Instant {
    Instant::now() + match_budget(content.len())
}

fn is_document_probe(pattern: &str) -> bool {
    pattern.trim() == "$$$"
}

#[cfg(test)]
mod tests;
