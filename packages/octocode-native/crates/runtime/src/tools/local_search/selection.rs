//! clasifyContext for localSearch: each snippet group (one hit with its
//! context window; overlapping or touching windows in a file are one group)
//! is a candidate, judged before paging. A file stays when any of its groups
//! is selected and shows only those hits; the counts describe the kept hits,
//! and the receipt counts groups. Unknown groups stay reachable through a
//! lead to exactly the files holding them.
use super::cursor::{normalized_query, stale_snapshot};
use super::layout::streamed_layout;
use super::types::SearchContext;
use super::types::*;
use super::verify::{PageScope, Redacted, verify_candidate};
use crate::execution::cancel::CancellationCheck;
use crate::id::ToolId;
use crate::num::u32_of_usize;
use crate::response::result::ToolError;
use crate::tools::selection::{Candidate, Decision, Failure, Outcome, ProviderError, Runtime};
use octocode_contract_types::SelectionUnit;
use octocode_engine::types::{TextSearchFile, TextSearchResult};
use serde_json::{Value, json};
use std::collections::HashSet;

/// A selected page's plan and what it removed.
pub(super) struct Selected {
    pub(super) outcome: Outcome,
    /// Files (root-relative, scan order) holding a group judged unknown,
    /// one without a state to judge, or one the budget left unjudged.
    unknown_files: Vec<String>,
    /// Files the scan matched.
    scan_files: usize,
    /// Files that held hits of this candidate page before and after
    /// selection.
    files_before: usize,
    files_kept: usize,
    /// Hit rows considered by this candidate page, before and after
    /// selection.
    rows_before: usize,
    rows_kept: usize,
    /// The provider's error when it failed this page: the hits are shown
    /// unfiltered under an `unavailable` receipt.
    provider: Option<ProviderError>,
}

/// The snippet groups of one file: indices into its `matches`, each group
/// in line order. Windows `[line - before, line + after]` that overlap or
/// touch form one group.
fn groups(file: &TextSearchFile, context: SearchContext) -> Vec<Vec<usize>> {
    let mut order = (0..file.matches.len()).collect::<Vec<_>>();
    order.sort_by_key(|&i| (file.matches[i].line, file.matches[i].column));
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut end = 0u32;
    for index in order {
        let line = file.matches[index].line;
        match groups.last_mut() {
            Some(group) if line.saturating_sub(context.before) <= end.saturating_add(1) => {
                group.push(index);
            }
            _ => groups.push(vec![index]),
        }
        end = end.max(line.saturating_add(context.after));
    }
    groups
}

/// Select snippet groups before paging; a replay requires its bound plan.
pub(super) fn select_hits(
    scope: &PageScope<'_, impl CancellationCheck>,
    parsed: &mut TextSearchResult,
    runtime: Option<&Runtime>,
    identity: &str,
    redacted: &mut Redacted,
) -> Result<Option<Selected>, ToolError> {
    let query = scope.query;
    if query.clasify_context.is_none() {
        return match query.selection {
            Some(_) => Err(stale_snapshot(query)),
            None => Ok(None),
        };
    }
    if query.snapshot.is_some() && query.selection.is_none() {
        return Err(stale_snapshot(query));
    }
    let prior = query.selection.as_ref().map(|token| token.as_str());
    let context = if super::executor::uses_context(query.result_view) {
        local_search_query::context(query)
    } else {
        SearchContext::default()
    };
    let file_groups = parsed
        .files
        .iter()
        .map(|file| groups(file, context))
        .collect::<Vec<_>>();
    let total = file_groups.iter().map(Vec::len).sum();
    let needs_states = crate::tools::selection::needs_candidate_states(runtime, prior, None, total);
    let mut candidates = Vec::with_capacity(total);
    for (file, groups) in parsed.files.iter_mut().zip(&file_groups) {
        let verified = needs_states && verify_candidate(scope, file, redacted)?;
        // The declaration around each hit (the same `enclosing` a shown row
        // names), so a predicate about scope (test or production code, which
        // function) can be judged from the candidate itself.
        let outline = verified
            .then(|| {
                let source = scope.output_root.join(&file.path);
                super::enclosing::shown_outline(
                    &source,
                    (scope.expected_digest)(&source),
                    scope.security,
                )
            })
            .flatten();
        for group in groups {
            let state = verified.then(|| {
                json!({"path": file.path, "matches": group.iter().map(|&i| {
                    let hit = &file.matches[i];
                    let mut row = json!({"line":hit.line,"value":hit.value});
                    if let Some(enclosing) = outline
                        .as_ref()
                        .and_then(|outline| outline.label(outline.owner(hit.line)?))
                    {
                        row["enclosing"] =
                            json!({"kind":enclosing.kind,"symbolName":enclosing.symbol_name});
                    }
                    row
                }).collect::<Vec<_>>()})
            });
            candidates.push(Candidate::new(state));
        }
    }
    let query_json = normalized_query(query, streamed_layout(query));
    // The search succeeded: a provider that cannot answer now never
    // withholds its hits ([`crate::tools::selection::unless_transient`]).
    // A refused credential or request stays the row's error, with its
    // unfiltered lead.
    let (outcome, provider) =
        crate::tools::selection::unless_transient(crate::tools::selection::select(
            runtime,
            &query_json,
            identity,
            prior,
            &candidates,
            None,
            SelectionUnit::Chunks,
        ))
        .map_err(|failure| match failure {
            Failure::Stale => stale_snapshot(query),
            Failure::Cancelled => ToolError::cancelled("Selection was cancelled"),
            Failure::Provider(failure) => failure.tool_error(crate::tools::selection::unfiltered(
                ToolId::LocalSearch,
                &query_json,
                &[],
            )),
        })?;
    let unavailable = outcome.provider_unavailable();
    let mut decisions = vec![Decision::Unknown; total];
    for &ordinal in &outcome.selected {
        decisions[ordinal] = Decision::Selected;
    }
    for &ordinal in &outcome.excluded {
        decisions[ordinal] = Decision::Excluded;
    }
    let unjudged = outcome.unjudged.iter().copied().collect::<HashSet<_>>();
    // Unjudged groups are unknown: the unknown lead reads them as they are,
    // the next candidate page (when its token fits the `selection` field)
    // judges them.
    let (mut rows_total, mut rows_before, mut lines_removed) = (0usize, 0usize, 0usize);
    let scan_files = parsed.files.len();
    let (mut files_before, mut unknown_files) = (0usize, Vec::new());
    let mut kept = Vec::with_capacity(parsed.files.len());
    let mut ordinal = 0usize;
    for (mut file, groups) in std::mem::take(&mut parsed.files)
        .into_iter()
        .zip(file_groups)
    {
        rows_total += file.matches.len();
        let lines_before = distinct_lines(&file);
        let mut keep = vec![false; file.matches.len()];
        let (mut holds_unknown, mut judged) = (false, false);
        for group in groups {
            // A group outside this candidate page's pool was judged (and
            // shown, or left to its unfiltered route) by an earlier one.
            if outcome.considers(ordinal) {
                if unjudged.contains(&ordinal) {
                    holds_unknown = true;
                } else {
                    judged = true;
                    rows_before += group.len();
                    match decisions[ordinal] {
                        Decision::Selected => group.iter().for_each(|&i| keep[i] = true),
                        // Unjudgeable here: shown as found.
                        Decision::Unknown if unavailable => {
                            group.iter().for_each(|&i| keep[i] = true);
                        }
                        Decision::Unknown => holds_unknown = true,
                        Decision::Excluded => {}
                    }
                }
            }
            ordinal += 1;
        }
        files_before += usize::from(judged);
        if holds_unknown {
            unknown_files.push(file.path.clone());
        }
        let mut flags = keep.into_iter();
        file.matches.retain(|_| flags.next().unwrap_or(false));
        lines_removed += lines_before - distinct_lines(&file);
        if !file.matches.is_empty() {
            file.match_count = file.matches.len() as u32;
            kept.push(file);
        }
    }
    parsed.files = kept;
    let rows_kept = parsed.files.iter().map(|file| file.matches.len()).sum();
    recount(
        query,
        &mut parsed.stats,
        rows_total - rows_kept,
        lines_removed,
    );
    Ok(Some(Selected {
        outcome,
        unknown_files,
        scan_files,
        files_before,
        files_kept: parsed.files.len(),
        rows_before,
        rows_kept,
        provider,
    }))
}

fn distinct_lines(file: &TextSearchFile) -> usize {
    file.matches
        .iter()
        .map(|hit| hit.line)
        .collect::<HashSet<_>>()
        .len()
}

/// Totals that describe the kept hits: matched lines lose the lines whose
/// every row was removed. Occurrences follow only when each row is one
/// occurrence (matchOnly without `unique`) or every hit line of the scan
/// holds exactly one; otherwise they are unknown (absent), never estimated.
fn recount(
    query: &LocalSearchQuery,
    stats: &mut octocode_engine::types::TextSearchStats,
    rows_removed: usize,
    lines_removed: usize,
) {
    if rows_removed == 0 {
        return;
    }
    let saturate = |n: usize| u32_of_usize(n);
    let rows_are_occurrences = query.result_view == LocalSearchQueryResultView::MatchOnly
        && query.unique == LocalSearchQueryUnique::Off;
    let one_per_line = stats.match_count.is_some() && stats.match_count == stats.matched_lines;
    stats.match_count = if rows_are_occurrences {
        stats
            .match_count
            .map(|n| n.saturating_sub(saturate(rows_removed)))
    } else if one_per_line {
        stats
            .match_count
            .map(|n| n.saturating_sub(saturate(lines_removed)))
    } else {
        None
    };
    stats.matched_lines = stats
        .matched_lines
        .map(|n| n.saturating_sub(saturate(lines_removed)));
}

impl Selected {
    /// The receipt the row reports.
    pub(super) fn receipt(&self) -> &octocode_contract_types::Selection {
        &self.outcome.receipt
    }

    /// The plan token later pages replay.
    pub(super) fn token(&self) -> &str {
        &self.outcome.token
    }

    /// `next.unfiltered`, the whole unfiltered result of this scan, and
    /// `next.unfilteredUnknown` when it is narrower: the unknown hits (the
    /// same search over exactly the files holding them). Without unknown
    /// hits, or when they span every file, the whole result is the one
    /// lead. Groups the candidate budget left
    /// unjudged get `nextCandidatePage`: the same classified search of the
    /// same scan judging exactly them, so a chain of candidate pages judges
    /// every group once.
    pub(super) fn leads(
        &self,
        query: &LocalSearchQuery,
        snapshot: &str,
        root_is_dir: bool,
    ) -> Vec<(&'static str, Value)> {
        // A page that kept every hit, or that the provider failed, already
        // shows every hit unfiltered.
        if !self.outcome.leaves_out() {
            return Vec::new();
        }
        let mut query_json = normalized_query(query, streamed_layout(query));
        // The clasify context default (`local_search_query::context`) stated:
        // an unfiltered lead drops `clasifyContext` and must keep the windows
        // the snapshot captured.
        if query.context_lines.is_none()
            && query.context_before.is_none()
            && query.context_after.is_none()
        {
            query_json["contextLines"] =
                json!(super::types::local_search_query::CLASIFY_CONTEXT_LINES);
        }
        let mut leads = self.unfiltered_leads(&query_json, snapshot, root_is_dir);
        if let Some(token) = self.outcome.next_candidates(ToolId::LocalSearch) {
            let mut row = query_json;
            if let Some(fields) = row.as_object_mut() {
                for field in crate::response::result::CURSOR_FIELDS {
                    fields.remove(field);
                }
            }
            row["snapshot"] = json!(snapshot);
            row["selection"] = json!(token);
            leads.push((
                "nextCandidatePage",
                crate::response::result::Continuation::new(ToolId::LocalSearch, row).build(),
            ));
        }
        leads
    }

    /// The whole unfiltered result of this scan.
    fn whole(&self, query_json: &Value, snapshot: &str) -> Value {
        let mut whole = crate::tools::selection::unfiltered_row(query_json, &[]);
        whole["snapshot"] = json!(snapshot);
        crate::response::result::Continuation::new(ToolId::LocalSearch, whole).build()
    }

    fn unfiltered_leads(
        &self,
        query_json: &Value,
        snapshot: &str,
        root_is_dir: bool,
    ) -> Vec<(&'static str, Value)> {
        let whole = self.whole(query_json, snapshot);
        let globs = self
            .unknown_files
            .iter()
            .map(|path| exact_glob(path))
            .collect::<Option<Vec<_>>>()
            .filter(|globs| {
                root_is_dir
                    && !globs.is_empty()
                    && globs.len() < self.scan_files
                    && globs.len() <= include_cap()
            });
        let Some(globs) = globs else {
            return vec![("unfiltered", whole)];
        };
        let mut unknown = crate::tools::selection::unfiltered_row(query_json, &[]);
        unknown["include"] = json!(globs);
        let unknown =
            crate::response::result::Continuation::new(ToolId::LocalSearch, unknown).build();
        vec![("unfiltered", whole), ("unfilteredUnknown", unknown)]
    }

    /// The provider's error when it failed this page, the pre-selection
    /// totals when selection removed hits, and the groups the candidate
    /// budget left unjudged. That counts describe kept hits is the tool
    /// description's, not repeated here.
    pub(super) fn disclosure(&self) -> Vec<String> {
        let mut notes = Vec::new();
        if let Some(error) = &self.provider {
            notes.push(crate::tools::selection::unclassified_warning(
                error,
                "this search",
                "its hits are",
            ));
        }
        if self.rows_kept < self.rows_before {
            notes.push(format!(
                "clasifyContext kept {} of {} hit rows ({} of {} files).",
                self.rows_kept, self.rows_before, self.files_kept, self.files_before,
            ));
        }
        notes.extend(
            self.outcome
                .budget_warning(ToolId::LocalSearch, "hit groups"),
        );
        notes
    }
}

/// `include` entries one call accepts (the contract's `maxItems`).
fn include_cap() -> usize {
    crate::contracts::query_schema_number(ToolId::LocalSearch, None, "include", "maxItems")
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0)
}

/// A glob matching exactly `path` (root-relative): a leading `/` anchors it
/// at the root and `*`/`?` are bracketed. `None` for a path a class or
/// alternation character would make ambiguous.
fn exact_glob(path: &str) -> Option<String> {
    if path.contains(['[', ']', '{', '}', '\\']) {
        return None;
    }
    let mut glob = String::with_capacity(path.len() + 1);
    glob.push('/');
    for c in path.chars() {
        if matches!(c, '*' | '?') {
            glob.push('[');
            glob.push(c);
            glob.push(']');
        } else {
            glob.push(c);
        }
    }
    Some(glob)
}

#[cfg(test)]
mod tests {
    use super::super::execute_local_search;
    use super::*;
    use crate::execution::cancel::NeverCancel;
    use crate::security::ContentSecurity;
    use crate::tools::selection::{JudgeCall, Judgments, SelectionJudge};
    use crate::tools::test_support::selection_fake::{self, DOWN, REFUSED, WordJudge};
    use octocode_contract_types::ErrorCode;
    use std::time::Duration;

    fn runtime(fail: Option<ErrorCode>, executor: &tokio::runtime::Runtime) -> Runtime {
        runtime_with(fail, executor, 64)
    }

    fn runtime_with(
        fail: Option<ErrorCode>,
        executor: &tokio::runtime::Runtime,
        max: usize,
    ) -> Runtime {
        let mut runtime = selection_fake::runtime(
            WordJudge { fail },
            executor.handle().clone(),
            Duration::from_secs(5),
        );
        runtime.limits.max_candidates = max;
        runtime
    }

    fn search(
        root: &std::path::Path,
        fields: Value,
        fail: Option<ErrorCode>,
    ) -> Result<LocalSearchResult, ToolError> {
        search_with(root, fields, fail, 64)
    }

    fn search_with(
        root: &std::path::Path,
        fields: Value,
        fail: Option<ErrorCode>,
        max: usize,
    ) -> Result<LocalSearchResult, ToolError> {
        // Bare hit lines (`contextLines: 0`) unless a test asks for context:
        // these tests count one judgment per hit, which the clasify context
        // default would merge into shared windows.
        let mut query = json!({"path": root, "matchString": "needle",
            "contextLines": 0, "clasifyContext": {"classifiers": ["Is this kept?"]}});
        for (key, value) in fields.as_object().into_iter().flatten() {
            query[key] = value.clone();
        }
        let query: LocalSearchQuery = serde_json::from_value(query).expect("query");
        let policy = crate::tools::test_support::workspace_policy(root);
        let executor = tokio::runtime::Runtime::new().expect("runtime");
        let selection = runtime_with(fail, &executor, max);
        execute_local_search(
            &query,
            &policy,
            &ContentSecurity::new(),
            &NeverCancel,
            None,
            None,
            Some(&selection),
        )
    }

    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("fixture");
        std::fs::write(
            root.path().join("a.rs"),
            "needle keep one\nx\nx\nneedle drop\nx\nx\nneedle keep two\n",
        )
        .expect("a");
        std::fs::write(root.path().join("b.rs"), "needle maybe\n").expect("b");
        std::fs::write(root.path().join("c.rs"), "needle drop\n").expect("c");
        root
    }

    #[test]
    fn span_selection_ignores_context_controls() {
        let root = fixture();
        let base =
            json!({"matchString":"needle (keep|drop)","regex":"rust","resultView":"matchOnly"});
        let plain = search(root.path(), base.clone(), None).expect("search");
        assert!(
            plain
                .files
                .iter()
                .flat_map(|f| f.matches.iter().flatten())
                .all(|m| !m.value.contains("drop"))
        );
        for extra in [
            json!({"contextBefore":100}),
            json!({"contextAfter":100}),
            json!({"contextLines":100}),
        ] {
            let mut fields = base.clone();
            fields
                .as_object_mut()
                .expect("fields")
                .extend(extra.as_object().expect("extra").clone());
            let contextual = search(root.path(), fields, None).expect("search");
            assert_eq!(
                serde_json::to_value(&contextual.files).expect("files"),
                serde_json::to_value(&plain.files).expect("files")
            );
        }
    }

    #[test]
    fn oversized_unknown_group_does_not_claim_its_hits_were_judged() {
        let root = tempfile::tempdir().expect("fixture");
        std::fs::write(
            root.path().join("large.txt"),
            "needle keep\n".repeat(30_000),
        )
        .expect("source");
        let result = search(root.path(), json!({"contextLines":1}), None).expect("search");
        let receipt = result.selection.as_ref().expect("receipt");
        assert_eq!(receipt.unknown_items, 1);
        assert_eq!(receipt.selected_items, 0);
        assert!(
            result
                .warnings
                .iter()
                .any(|note| note.contains("kept 0 of 30000 hit rows"))
        );
        assert!(
            result
                .warnings
                .iter()
                .all(|note| !note.contains("judged hit rows"))
        );
        assert!(result.next.as_ref().unwrap().get("unfiltered").is_some());
    }

    #[test]
    fn each_hit_is_judged_and_counts_describe_the_kept_hits() {
        let root = fixture();
        let result = search(root.path(), json!({}), None).expect("selected search");
        let receipt = result.selection.as_ref().expect("receipt");
        assert_eq!(receipt.unit, SelectionUnit::Chunks);
        assert_eq!(
            (
                receipt.total_items,
                receipt.selected_items,
                receipt.excluded_items,
                receipt.unknown_items
            ),
            (5, 2, 2, 1)
        );
        assert_eq!(result.files.len(), 1, "{:?}", result.files);
        let kept = result.files[0].matches.as_ref().expect("rows");
        assert_eq!(
            kept.iter().map(|hit| hit.line).collect::<Vec<_>>(),
            [1, 7],
            "the excluded hit in a kept file is dropped"
        );
        assert_eq!(result.stats.total_occurrences, Some(2));
        assert_eq!(result.stats.matched_lines, Some(2));
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("kept 2 of 5 hit rows (1 of 3 files)")),
            "{:?}",
            result.warnings
        );
        let next = result.next.as_ref().expect("leads");
        // The filtered-evidence notice is the response stage's, said once
        // (`selection_errors::decorate`); the reader states only its counts.
        assert!(
            !result
                .warnings
                .iter()
                .any(|warning| warning.contains("not proof of absence")),
            "{:?}",
            result.warnings
        );
        let unknown = &next["unfilteredUnknown"]["query"]["queries"][0];
        assert_eq!(unknown["include"], json!(["/b.rs"]), "{next}");
        assert!(unknown.get("clasifyContext").is_none());
        assert!(unknown.get("snapshot").is_none());
        let whole = &next["unfiltered"]["query"]["queries"][0];
        assert!(whole.get("include").is_none());
        assert!(whole.get("snapshot").is_some());
    }

    #[test]
    fn the_unknown_lead_reaches_exactly_the_files_holding_unknown_hits() {
        let root = fixture();
        let result = search(root.path(), json!({}), None).expect("selected search");
        let lead = result.next.expect("leads")["unfilteredUnknown"]["query"]["queries"][0].clone();
        let query: LocalSearchQuery = serde_json::from_value(lead).expect("executable lead");
        let policy = crate::tools::test_support::workspace_policy(root.path());
        let unfiltered = super::super::execute_local_search(
            &query,
            &policy,
            &ContentSecurity::new(),
            &NeverCancel,
            None,
            None,
            None,
        )
        .expect("unknown evidence");
        let paths = unfiltered
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(paths, ["b.rs"]);
    }

    /// Groups past the candidate budget, including the rest of a file the
    /// budget reached in part, are judged by `nextCandidatePage`: the same
    /// classified search of the same scan. Following the chain judges every
    /// group exactly once and shows exactly the hits one unbudgeted call
    /// keeps.
    #[test]
    fn a_chain_of_candidate_pages_judges_every_group_once() {
        let root = fixture();
        let shown = |result: &LocalSearchResult| {
            result
                .files
                .iter()
                .flat_map(|file| {
                    let path = file.path.clone();
                    file.matches
                        .iter()
                        .flatten()
                        .map(move |hit| (path.clone(), hit.line))
                })
                .collect::<Vec<_>>()
        };
        let unbudgeted = search(root.path(), json!({}), None).expect("one call");
        let mut expected = shown(&unbudgeted);
        expected.sort();
        // a.rs holds groups 0-2, b.rs 3, c.rs 4; two are judged per call.
        let mut result = search_with(root.path(), json!({}), None, 2).expect("first page");
        let receipt = result.selection.as_ref().expect("receipt");
        assert_eq!(receipt.reason.as_deref(), Some("candidateBudget"));
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("judged 2 of 5 hit groups") && w.contains("3 were not judged")),
            "{:?}",
            result.warnings
        );
        let (mut totals, mut seen) = (Vec::new(), Vec::new());
        loop {
            let receipt = result.selection.as_ref().expect("receipt");
            totals.push(receipt.total_items);
            seen.extend(shown(&result));
            let Some(page) = result
                .next
                .as_ref()
                .and_then(|next| next.get("nextCandidatePage"))
            else {
                break;
            };
            let row = page["query"]["queries"][0].clone();
            assert!(row.get("clasifyContext").is_some(), "classified: {row}");
            assert!(row.get("snapshot").is_some() && row.get("selection").is_some());
            assert!(row.get("include").is_none(), "the same scan: {row}");
            assert!(totals.len() < 5, "the chain ends");
            result = search_with(root.path(), row, None, 2).expect("next candidates");
        }
        assert_eq!(totals, [5, 3, 1], "each page counts the groups it judges");
        seen.sort();
        assert_eq!(seen, expected, "every kept hit once, none twice");
    }

    /// A candidate names the declaration around each hit, so a predicate
    /// about scope can be judged.
    #[test]
    fn candidates_carry_the_enclosing_declaration() {
        struct Capture(std::sync::Mutex<Vec<Value>>);
        impl SelectionJudge for Capture {
            fn identity(&self) -> Value {
                json!({"test":"capture"})
            }
            fn select(&self, call: JudgeCall<'_>) -> Judgments {
                let mut seen = self.0.lock().expect("lock");
                seen.extend(
                    call.admitted
                        .iter()
                        .filter_map(|&i| call.candidates[i].state.clone()),
                );
                Ok(Vec::new())
            }
        }
        let root = tempfile::tempdir().expect("fixture");
        std::fs::write(
            root.path().join("a.rs"),
            "fn production() {\n    needle();\n}\n\n#[test]\nfn checks_it() {\n    needle();\n}\n",
        )
        .expect("a");
        let query: LocalSearchQuery = serde_json::from_value(json!({"path": root.path(),
            "matchString": "needle", "contextLines": 0,
            "clasifyContext": {"classifiers": ["Is this test code?"]}}))
        .expect("query");
        let executor = tokio::runtime::Runtime::new().expect("runtime");
        let judge = std::sync::Arc::new(Capture(std::sync::Mutex::new(Vec::new())));
        let selection = Runtime {
            judge: judge.clone(),
            ..runtime(None, &executor)
        };
        execute_local_search(
            &query,
            &crate::tools::test_support::workspace_policy(root.path()),
            &ContentSecurity::new(),
            &NeverCancel,
            None,
            None,
            Some(&selection),
        )
        .expect("search");
        let seen = judge.0.lock().expect("lock").clone();
        let names = seen
            .iter()
            .map(|state| state["matches"][0]["enclosing"]["symbolName"].clone())
            .collect::<Vec<_>>();
        assert_eq!(names, [json!("production"), json!("checks_it")], "{seen:?}");
    }

    /// A provider that cannot answer (down, overloaded, unreachable) never
    /// withholds a search that succeeded: every hit is shown under an `unavailable` receipt naming
    /// the provider's code, the warning states its error and hint, and the
    /// next page replays that plan without another provider call.
    #[test]
    fn a_provider_that_is_down_shows_the_unfiltered_page_as_unavailable() {
        let root = fixture();
        let result = search(root.path(), json!({}), DOWN).expect("the search succeeded");
        let receipt = result.selection.as_ref().expect("receipt");
        assert_eq!(
            receipt.status,
            octocode_contract_types::SelectionStatus::Unavailable
        );
        assert_eq!(
            receipt.reason.as_deref(),
            DOWN.map(crate::id::error_codes::as_str)
        );
        assert_eq!((receipt.total_items, receipt.unknown_items), (5, 5));
        let shown = result
            .files
            .iter()
            .map(|file| {
                (
                    file.path.as_str(),
                    file.matches.as_ref().map_or(0, Vec::len),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(shown, [("a.rs", 3), ("b.rs", 1), ("c.rs", 1)], "every hit");
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("HTTP 503")
                    && warning.contains("shown unfiltered")
                    && warning.contains("OCTOCODE_CLASSIFICATION_API")),
            "{:?}",
            result.warnings
        );
        let next = result.next.as_ref();
        assert!(
            next.is_none_or(|next| next.get("unfiltered").is_none()),
            "the page is the unfiltered view: {next:?}"
        );
        // The recorded plan replays on the next page: the provider (now
        // answering) is not asked again, so the page stays unavailable.
        let first = search(root.path(), json!({"pageSize": 1}), DOWN).expect("first page");
        let next =
            first.next.as_ref().expect("a next page")["nextPage"]["query"]["queries"][0].clone();
        assert!(next.get("selection").is_some(), "{next}");
        let replay = search(root.path(), next, None).expect("replayed plan");
        assert_eq!(
            replay.selection.as_ref().map(|receipt| receipt.status),
            Some(octocode_contract_types::SelectionStatus::Unavailable)
        );
        assert!(!replay.files.is_empty(), "the next page's hits are shown");
    }

    /// A refused credential is the caller's to fix: the row fails with the
    /// provider's typed error and hint, never masked as a page, and its
    /// unfiltered lead keeps the evidence one call away.
    #[test]
    fn a_refused_credential_fails_the_row_with_its_typed_error() {
        let root = fixture();
        let error = search(root.path(), json!({}), REFUSED).expect_err("refused credential");
        assert_eq!(error.code, ErrorCode::ClassificationProviderError);
        assert!(
            error
                .hints
                .iter()
                .any(|hint| hint.contains("OCTOCODE_CLASSIFICATION_API"))
        );
        let receipt = error.selection.as_ref().expect("receipt");
        assert_eq!(
            receipt.status,
            octocode_contract_types::SelectionStatus::Unavailable
        );
        assert_eq!(
            receipt.reason.as_deref(),
            REFUSED.map(crate::id::error_codes::as_str)
        );
        assert_eq!(receipt.unknown_items, 5);
        let lead = &error.next.as_ref().expect("evidence stays reachable")["unfiltered"];
        assert!(lead["query"]["queries"][0].get("clasifyContext").is_none());
    }

    #[test]
    fn snippet_groups_merge_overlapping_or_touching_windows() {
        let hit = |line| octocode_engine::types::TextSearchMatch {
            deferred: None,
            line,
            last_line: None,
            column: 0,
            byte_offset: 0,
            value: String::new(),
            value_digest: <sha2::Sha256 as sha2::Digest>::digest(b"").into(),
            count: None,
            rank: None,
            original_chars: None,

            original_bytes: None,
        };
        let file = TextSearchFile {
            path: "a.rs".into(),
            match_count: 4,
            matches: vec![hit(20), hit(1), hit(2), hit(9)],
            source: None,
        };
        assert_eq!(
            groups(&file, SearchContext::symmetric(0)),
            [vec![1, 2], vec![3], vec![0]]
        );
        assert_eq!(
            groups(&file, SearchContext::symmetric(3)),
            [vec![1, 2, 3], vec![0]]
        );
    }

    #[test]
    fn an_exact_glob_anchors_and_brackets_metacharacters() {
        assert_eq!(exact_glob("a.rs").as_deref(), Some("/a.rs"));
        assert_eq!(exact_glob("src/*x?.rs").as_deref(), Some("/src/[*]x[?].rs"));
        assert_eq!(exact_glob("app/[id].tsx"), None);
    }
}
