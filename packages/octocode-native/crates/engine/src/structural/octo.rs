mod source_context;
use std::collections::HashMap;

use super::language::AgLanguage;
use super::query::StructuralQuery;

mod error;
mod expanded;
mod line_index_support;
mod macro_bodies;
mod matching;
mod metavars;
mod pattern;
mod position;
mod recovery;
mod rule;
mod stack;
pub(super) use stack::on_deep_stack;
mod selector;

pub(super) use error::CompileError;
pub(super) use matching::{
    Deadline, ExecutionError, named_children, node_text, parse_tree_with_deadline,
};
pub use rule::EXAMPLE_RULE;
pub(super) use rule::{RawPattern, RawRule, decode_rule};

#[cfg(test)]
pub(super) use matching::INTERRUPT_NEXT_COMPILE_PARSE;
#[cfg(test)]
pub(super) use stack::MAX_STRUCTURAL_DEPTH;

use crate::text::LineIndex;
pub use error::COMPILE_FAILED;
use expanded::{ExpandedTree, Seen};
use line_index_support::{to_structural_match, to_structural_match_with_context};
use macro_bodies::MacroBodies;
use matching::{CaptureEnv, collect_kind_matches};
pub(super) use matching::{Found, MatchWithKind};
pub use matching::{MATCH_DEADLINE, PARSE_INTERRUPTED};
use pattern::CompiledPattern;
pub use pattern::{
    LiteralSigil, PatternDeclaration, QualifiedPattern, Separator, SigilForm, SigilRepair,
};
use recovery::Recovery;
pub use rule::RULE_INVALID;
use rule::{CompiledRule, Document, RelationMemo};

#[cfg(test)]
use matching::{SECONDARY_CAPTURE, parse_tree, visit_named};
#[cfg(test)]
use rule::{ANCESTOR_STEPS, RULE_DECODE_COUNT};

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

/// What recovered syntax could hide of one source's matches.
pub(super) struct Recovered {
    /// The source's own parse ([`Found::recovered`]).
    pub(super) file: bool,
    /// Macro bodies whose re-parse could not read a matched node
    /// ([`Found::recovered_bodies`]).
    pub(super) bodies: Vec<super::types::RecoveredBody>,
}

/// [`all_matches`], and what recovered syntax could hide of them.
pub(super) fn matches_with_recovery(
    run: &OctoCompiledMatcher,
    content: &str,
) -> Result<(Vec<MatchWithKind>, Recovered), ExecutionError> {
    let mut found = Found::new(super::types::StructuralDemand::Matches);
    run(content, &mut found)?;
    let recovered = Recovered {
        file: found.recovered(),
        bodies: found.recovered_bodies().to_vec(),
    };
    Ok((found.into_matches(), recovered))
}

/// The declaration `pattern` spells in `lang` ([`CompiledPattern::declaration`]).
pub(super) fn pattern_declaration(lang: &AgLanguage, pattern: &str) -> Option<PatternDeclaration> {
    CompiledPattern::new(lang, pattern).ok()?.declaration()
}

/// The fields in which the first node of the declaration `pattern` spells
/// that starts on 1-based `line` of `content` differs from it
/// ([`CompiledPattern::differing_fields`]); empty when no such node or field.
pub(super) fn pattern_differences(
    lang: &AgLanguage,
    pattern: &str,
    content: &str,
    line: u32,
) -> Vec<String> {
    let Ok(compiled) = CompiledPattern::new(lang, pattern) else {
        return Vec::new();
    };
    let Some(kind) = compiled.declaration().map(|declaration| declaration.kind) else {
        return Vec::new();
    };
    let Ok(tree) = matching::parse_tree(compiled.language(), content) else {
        return Vec::new();
    };
    let row = usize::try_from(line.saturating_sub(1)).unwrap_or(usize::MAX);
    let mut pending = vec![tree.root_node()];
    while let Some(node) = pending.pop() {
        if node.start_position().row > row || node.end_position().row < row {
            continue;
        }
        if node.start_position().row == row && node.kind() == kind {
            return compiled.differing_fields(node, content);
        }
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        pending.extend(children.into_iter().rev());
    }
    Vec::new()
}

/// The kind of node `pattern` parses as in `lang`.
pub(super) fn pattern_kind(lang: &AgLanguage, pattern: &str) -> Option<String> {
    CompiledPattern::new(lang, pattern).ok()?.root_kind()
}

/// A pattern as the caller wrote it: text, or a `{context, selector}` pair.
#[derive(Clone, Copy, Debug)]
pub enum WrittenPattern<'a> {
    Text(&'a str),
    Context { context: &'a str, selector: &'a str },
}

impl<'a> WrittenPattern<'a> {
    /// The text the caller wrote: the pattern, or the context.
    pub fn text(self) -> &'a str {
        match self {
            Self::Text(text) => text,
            Self::Context { context, .. } => context,
        }
    }

    pub(in crate::structural::octo) fn compile(
        self,
        lang: &AgLanguage,
    ) -> Result<CompiledPattern, CompileError> {
        match self {
            Self::Text(text) => CompiledPattern::new(lang, text),
            Self::Context { context, selector } => {
                CompiledPattern::contextual(lang, context, selector)
            }
        }
    }

    /// This pattern with `text` in place of what the caller wrote.
    fn with_text<'b>(self, text: &'b str) -> WrittenPattern<'b>
    where
        'a: 'b,
    {
        match self {
            Self::Text(_) => WrittenPattern::Text(text),
            Self::Context { selector, .. } => WrittenPattern::Context {
                context: text,
                selector,
            },
        }
    }
}

/// The literal tokens of `pattern` in `lang` that spell a metavariable sigil
/// ([`CompiledPattern::literal_sigils`]); none when it does not compile.
pub(super) fn literal_sigils(lang: &AgLanguage, pattern: WrittenPattern<'_>) -> Vec<LiteralSigil> {
    pattern
        .compile(lang)
        .map(|compiled| compiled.literal_sigils(pattern.text()))
        .unwrap_or_default()
}

/// The exact rewrite of `pattern`'s literal sigils in `lang`
/// ([`CompiledPattern::sigil_rewrite`]): `None` when it spells none, a sigil
/// has no exact rewrite, or the rewritten rule does not compile or still
/// spells one.
pub(super) fn sigil_repair(lang: &AgLanguage, pattern: WrittenPattern<'_>) -> Option<SigilRepair> {
    let sigils = literal_sigils(lang, pattern);
    if sigils.is_empty() {
        return None;
    }
    let (repaired, constraints) = CompiledPattern::sigil_rewrite(pattern.text(), &sigils)?;
    let rewritten = pattern.with_text(&repaired);
    if !rewritten
        .compile(lang)
        .ok()?
        .literal_sigils(&repaired)
        .is_empty()
    {
        return None;
    }
    let spelled = match rewritten {
        WrittenPattern::Text(text) => serde_json::json!(text),
        WrittenPattern::Context { context, selector } => {
            serde_json::json!({"context": context, "selector": selector})
        }
    };
    let mut parts = vec![serde_json::json!({"pattern": spelled})];
    parts.extend(constraints.iter().cloned());
    let rule = serde_json::json!({"all": parts});
    let query = StructuralQuery::new(None, Some(&rule)).ok()?;
    let _compiles = compile_matcher(lang, &query).ok()?;
    Some(SigilRepair {
        pattern: repaired,
        constraints,
    })
}

/// How `pattern` parsed in `lang` ([`CompiledPattern::shape`]), as one line,
/// rendered on the large stack: it recurses once per pattern level.
pub(super) fn pattern_shape(lang: &AgLanguage, pattern: WrittenPattern<'_>) -> Option<String> {
    let compiled = pattern.compile(lang).ok()?;
    stack::on_deep_stack(|| compiled.shape()).flatten()
}

/// The longer spelling of `pattern` in `lang` ([`CompiledPattern::qualifier`]).
pub(super) fn pattern_qualifier(
    lang: &AgLanguage,
    pattern: &str,
    separator: Separator,
) -> Option<QualifiedPattern> {
    stack::on_deep_stack(|| {
        CompiledPattern::new(lang, pattern)
            .ok()?
            .qualifier(lang, pattern, separator)
    })
    .flatten()
}

/// Compile `query` into a matcher that runs on the large stack
/// ([`stack`]): matching recurses once per pattern level, whatever the
/// caller's stack holds.
pub(super) fn compile_matcher(
    lang: &AgLanguage,
    query: &StructuralQuery<'_>,
) -> Result<OctoCompiledMatcher, CompileError> {
    // Read here, on the caller's thread: the matcher compiles on another.
    let macro_anchors = query.macro_anchors();
    #[cfg(test)]
    let macro_anchors = macro_anchors.filter(|_| !super::files::unfiltered());
    let run = stack::compile_on_deep_stack(|| compile_matcher_here(lang, query, macro_anchors))?;
    #[cfg(test)]
    let run = prefilter_audit(query, run);
    Ok(Box::new(move |content, found| {
        stack::on_deep_stack(|| run(content, found))
            .unwrap_or_else(|| Err(stack::stack_unavailable()))
    }))
}

fn compile_matcher_here(
    lang: &AgLanguage,
    query: &StructuralQuery<'_>,
    macro_anchors: Option<Vec<String>>,
) -> Result<OctoCompiledMatcher, CompileError> {
    let language = lang.tree_sitter_language();
    let recovery = Recovery::new(query.recovery_anchors());
    let macros = MacroBodies::for_language(lang, macro_anchors);
    match query.parts() {
        (Some(pattern), None) if is_document_probe(pattern) => {
            Ok(Box::new(move |content, found| {
                let deadline = Deadline::UNBOUNDED;
                let tree = parse_tree_with_deadline(&language, content, deadline.parse_by())?;
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
                let deadline = Deadline::UNBOUNDED;
                if compiled.is_special() {
                    return compiled.find_special_matches(content, deadline, &recovery, found);
                }

                let tree =
                    parse_tree_with_deadline(compiled.language(), content, deadline.parse_by())?;
                if recovery.hides_match(&tree, content) {
                    found.mark_recovered();
                }
                let expanded = ExpandedTree::new(
                    tree.root_node(),
                    content,
                    compiled.language(),
                    macros.as_ref(),
                    deadline,
                )?;
                expanded.name_hiding_bodies(found);
                // Built on the first match: most parsed files have none.
                let line_index = std::cell::OnceCell::new();
                expanded.visit_nodes(deadline, &mut |candidate, _, seen| {
                    if !compiled.matches_candidate(candidate) {
                        return Ok(());
                    }
                    let mut captures = CaptureEnv::default();
                    if compiled.matches(candidate, content, &mut captures, deadline)? {
                        if seen == Seen::Unreadable {
                            return disclose(&expanded, candidate, found);
                        }
                        let source = expanded.source_for(candidate, captures.trees())?;
                        found.push(candidate, || {
                            let (values, ranges) = captures.into_maps();
                            to_structural_match_with_context(
                                &source,
                                candidate,
                                content,
                                line_index.get_or_init(|| LineIndex::tree_sitter(content)),
                                values,
                                ranges,
                            )
                        });
                    }
                    Ok(())
                })
            }))
        }
        (None, Some(_)) => {
            let compiled = CompiledRule::compile(lang, query.parsed_rule()?)?;
            if let Some(kind) = compiled.simple_kind().map(str::to_owned) {
                return Ok(Box::new(move |content, found| {
                    let deadline = Deadline::UNBOUNDED;
                    let tree = parse_tree_with_deadline(&language, content, deadline.parse_by())?;
                    if recovery.hides_match(&tree, content) {
                        found.mark_recovered();
                    }
                    let expanded = ExpandedTree::new(
                        tree.root_node(),
                        content,
                        &language,
                        macros.as_ref(),
                        deadline,
                    )?;
                    expanded.name_hiding_bodies(found);
                    collect_kind_matches(&expanded, &kind, content, deadline, found)
                }));
            }
            Ok(Box::new(move |content, found| {
                let deadline = Deadline::UNBOUNDED;
                let tree = parse_tree_with_deadline(&language, content, deadline.parse_by())?;
                if recovery.hides_match(&tree, content) {
                    found.mark_recovered();
                }
                let expanded = ExpandedTree::new(
                    tree.root_node(),
                    content,
                    &language,
                    macros.as_ref(),
                    deadline,
                )?;
                expanded.name_hiding_bodies(found);
                let line_index = LineIndex::tree_sitter(content);
                let memo = RelationMemo::default();
                let document = Document {
                    memo: Some(&memo),
                    content,
                    deadline,
                    tree: &expanded,
                    line_index: &line_index,
                };
                expanded.visit_nodes(deadline, &mut |candidate, ancestors, seen| {
                    if !compiled.matches_candidate(candidate) {
                        return Ok(());
                    }
                    let mut captures = CaptureEnv::default();
                    if compiled.matches(candidate, Some(ancestors), &document, &mut captures)? {
                        if seen == Seen::Unreadable {
                            return disclose(&expanded, candidate, found);
                        }
                        let source = expanded.source_for(candidate, captures.trees())?;
                        found.push(candidate, || {
                            let (values, ranges) = captures.into_maps();
                            to_structural_match_with_context(
                                &source,
                                candidate,
                                content,
                                &line_index,
                                values,
                                ranges,
                            )
                        });
                    }
                    Ok(())
                })
            }))
        }
        _ => unreachable!("StructuralQuery validates the query shape"),
    }
}

/// Every engine test audits the literal prefilter: a source in which `run`
/// finds a match must hold what the query's prefilter requires, or a
/// directory scan would skip that file and drop the match. A violation
/// aborts the test binary (a panic here could be caught as a failed
/// search), naming the query.
#[cfg(test)]
fn prefilter_audit(query: &StructuralQuery<'_>, run: OctoCompiledMatcher) -> OctoCompiledMatcher {
    let prefilter = query.prefilter();
    let Ok(Some(filter)) = super::files::anchors::AnchorFilter::new(&prefilter) else {
        return run;
    };
    let source = query.explanation_with_prefilter(&prefilter).source;
    Box::new(move |content, found| {
        let before = found.pushed();
        run(content, found)?;
        if found.pushed() > before && !filter.admits(content.as_bytes()) {
            use std::io::Write as _;
            let _ = writeln!(
                std::io::stderr(),
                "prefilter audit: {prefilter:?} skips a source where {source} matches:\n{content}"
            );
            std::process::abort();
        }
        Ok(())
    })
}

/// A matched node its macro body's re-parse could not read: no match, but
/// the body is named as one whose matches may be missing.
fn disclose(
    expanded: &ExpandedTree<'_>,
    node: tree_sitter::Node<'_>,
    found: &mut Found<'_>,
) -> Result<(), ExecutionError> {
    if let Some(body) = expanded.disclosed_body(node) {
        found.mark_recovered_body(body);
    }
    Ok(())
}

fn is_document_probe(pattern: &str) -> bool {
    pattern.trim() == "$$$"
}

#[cfg(test)]
mod tests;
