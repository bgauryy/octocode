//! The follow-up calls a search page offers (read, references, re-runs,
//! listings), ranked; what the page could not cover is in `coverage`, the
//! empty page's probes in `probes`.

use super::executor::*;
use super::layout::*;
use super::types::*;
use super::{binaries::*, handoff::*, probes::*};
use crate::execution::cancel::CancellationCheck;
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use crate::policy::prune::DefaultsFlag;
use crate::response::result::Confidence;
use crate::response::result::{CURSOR_FIELDS, Continuation, ToolError, query_row};
use crate::tools::empty_walk::Cause;
use crate::tools::leads::skipped::under;
use octocode_contract_types::{SkippedPathCause, SkippedPaths};
use octocode_engine::types::{PatternSyntax, SearchOutput, TextSearchOptions};
use serde_json::{Value, json};

/// What a page's leads disclosed of the paths its walk skipped.
pub(super) struct Disclosed {
    /// Skipped paths hold a match, or may: a retry over them reaches it.
    pub(super) skipped_match: bool,
    /// The empty page's check of them did not finish: the row is partial.
    pub(super) unchecked: bool,
    pub(super) skipped_paths: Option<SkippedPaths>,
}

/// A page's result, for its leads.
pub(super) struct Found<'a> {
    pub(super) query: &'a LocalSearchQuery,
    pub(super) paths: &'a PathPolicy,
    pub(super) root: &'a std::path::Path,
    pub(super) output_root: &'a std::path::Path,
    pub(super) files: &'a [SearchFile],
    pub(super) layout: &'a Layout,
    pub(super) definition: Option<&'a Definition>,
    pub(super) symbol: Option<&'a str>,
    pub(super) scanned: &'a octocode_engine::types::TextSearchStats,
    /// What the walk left out: policy-withheld, default-excluded and hidden
    /// entries.
    pub(super) skipped: &'a crate::policy::discovery::WalkSkips,
    /// The walk reached every path. A binary cut or an unreadable file
    /// leaves file content unread, not paths unwalked, so it does not
    /// clear this.
    pub(super) walk_complete: bool,
    pub(super) empty: bool,
}

impl Found<'_> {
    /// Leads in the tool's own rank: a read of the top hit, the symbol's
    /// lspSearch lead, then reads of clipped values, the skipped binaries
    /// and the literal search. Returns what it disclosed of the skipped
    /// paths.
    pub(super) fn add_leads(
        &self,
        stats: &mut SearchStats,
        next: &mut Option<Value>,
        hints: &mut Vec<String>,
        warnings: &mut Vec<String>,
        search_options: &TextSearchOptions,
        cancel: &impl CancellationCheck,
    ) -> Result<Disclosed, ToolError> {
        let query = self.query;
        let first_disclosure =
            self.first_page() && local_search_query::match_page(query).max(1) == 1;
        let mut add = |name: String, lead: Value| {
            if let Some(map) = next.get_or_insert_with(|| json!({})).as_object_mut() {
                map.insert(name, lead);
            }
        };
        if let Some(read) = self.read_lead() {
            add("read".into(), read);
        }
        if let (Some(symbol), Some(definition)) = (self.symbol, self.definition)
            && let Some(row) = crate::tools::leads::lsp::verify_query(
                &definition.source,
                symbol,
                u64::from(definition.line),
                crate::tools::leads::lsp::Verify::for_kind(&definition.kind),
            )
        {
            add(
                crate::tools::leads::lsp::lead_name(&row),
                Continuation::new(ToolId::LspSearch, row).build(),
            );
        }
        for (name, lead) in self.expand_values_leads() {
            add(name, lead);
        }
        // Binaries were never text to search: a row with hits states their
        // count in `stats`, an empty row says it in `warnings`, where the
        // count may explain the miss.
        if let Some((warning, listing)) = self.skipped_binaries() {
            if self.empty {
                warnings.extend(Some(warning).filter(|warning| !warning.is_empty()));
            } else {
                stats.binary_skipped = self.leading_binaries();
            }
            add("binarySkipped".into(), listing);
        }
        // Directories the depth limit left unentered are coverage the walk
        // never had: disclosed once, with the same search at every depth.
        if first_disclosure
            && let Some(dirs) = self.scanned.depth_cut_dirs.filter(|count| *count > 0)
        {
            warnings.push(format!(
                "{dirs} dir(s) at maxDepth were not entered; files below them were not searched."
            ));
            add("expandDepth".into(), unlimited_depth_lead(query));
        }
        // What the walk left out is disclosed once, on the first page, in
        // `skippedPaths` (the security policy's withheld entries, which
        // nothing lifts, among them), with the same search over the paths a
        // flag lifts. An empty page proves which of them hold a match below,
        // in `disclose_unsearched`.
        if first_disclosure {
            let causes = self.skipped_causes();
            if !self.empty && !causes.is_empty() {
                add(
                    "includeSkipped".into(),
                    crate::tools::leads::skipped::lead(ToolId::LocalSearch, query, &causes),
                );
            }
        }
        // Thousands of hits page for dozens of calls: one count call ranks
        // the files first, so the next read or narrowing is chosen, not paged.
        if self.first_page()
            && !list_view(query.result_view)
            && let Some(total) = self
                .scanned
                .match_count
                .filter(|total| *total >= LARGE_RESULT_MATCHES)
        {
            add("viewFileCount".into(), count_lead(query));
            hints.push(format!("{total} matches."));
        }
        // An unset `regex` is literal; an empty literal search whose text has
        // regex characters says the other reading exists.
        let literal_missed = self.empty
            && query.regex.is_none()
            && local_search_query::pattern(query).contains(super::types::REGEX_METACHARACTERS);
        // The other reading is offered as the call that runs it, under the
        // engine the text compiles in (lookaround compiles only in pcre2).
        let engine = literal_missed
            .then(|| regex_engine(local_search_query::pattern(query)))
            .flatten();
        if let Some(engine) = engine {
            warnings
                .push("matchString ran as literal text; a lead reruns it as a regex.".to_owned());
            add("findSimilar".into(), regex_lead(query, engine));
        } else if literal_missed {
            warnings.push("matchString ran as literal text.".to_owned());
        }
        // A literal page whose text reads as an alternation (`a|b`) may
        // answer part of the question: one bounded count of the regex
        // reading says when it matches more lines.
        if let Some((engine, warning)) = self.alternation_reading(search_options, cancel) {
            warnings.push(warning);
            add("findSimilar".into(), regex_lead(query, engine));
        }
        // Files whose match the `fileHas`/`fileLacks` terms removed: the
        // empty page is the filter's, not the pattern's.
        if self.empty
            && let Some(dropped) = self.scanned.file_terms_dropped.filter(|count| *count > 0)
        {
            warnings.push(format!(
                "{dropped} file(s) matched matchString but fileHas/fileLacks removed them."
            ));
            add("broadenSearch".into(), unfiltered_terms_lead(query));
        }
        // An empty page's two probes (other spellings, skipped paths) are
        // independent bounded walks: they run side by side and disclose in
        // a fixed order.
        let spellings = self.empty && self.walk_complete && self.first_page();
        let unsearched =
            self.empty && self.walk_complete && first_disclosure && !self.root.is_file();
        let paths = self.paths;
        let (variants, tagged) = std::thread::scope(|scope| {
            let variants = spellings
                .then(|| scope.spawn(|| probe_variants(query, paths, search_options, cancel)));
            let tagged = unsearched.then(|| probe_unsearched(query, paths, search_options, cancel));
            (
                variants.and_then(|probe| probe.join().ok()).flatten(),
                tagged,
            )
        });
        if let Some(variants) = variants {
            disclose_variants(query, variants, next, hints);
        }
        let unsearched = match tagged {
            Some(tagged) => disclose_unsearched(
                query,
                search_options,
                tagged,
                Skipped {
                    walk: self.skipped,
                    causes: self.skipped_causes(),
                },
                next,
                hints,
                warnings,
            )?,
            None => Unsearched::default(),
        };
        if unsearched.unchecked {
            stats
                .partial_reasons
                .push(crate::tools::leads::skipped::SKIPPED_UNCHECKED.to_owned());
        }
        let skipped_paths = if first_disclosure {
            self.skipped_paths(unsearched.observed)?
        } else {
            None
        };
        Ok(Disclosed {
            skipped_match: unsearched.may_hold_matches,
            unchecked: unsearched.unchecked,
            skipped_paths,
        })
    }

    /// The causes the walk skipped paths by, so the lead over them lifts
    /// each in one call: a dot directory the default prune also names
    /// (`.github`, `.cache`), or a dot file the default excludes match, sits
    /// behind both.
    fn skipped_causes(&self) -> Vec<Cause> {
        let skipped = self.skipped;
        let excluded = skipped.hidden_excluded
            || skipped.ignored_excluded
            || !skipped.generated.is_empty()
            || !skipped.pruned.is_empty();
        [
            (!skipped.ignored.is_empty(), Cause::Ignored),
            (
                !skipped.hidden.is_empty() || skipped.ignored_hidden,
                Cause::Hidden,
            ),
            (excluded, Cause::DefaultExcluded),
        ]
        .into_iter()
        .filter_map(|(hit, cause)| hit.then_some(cause))
        .collect()
    }

    /// What the walk left out, on the first page: every ignored,
    /// default-excluded and hidden path with `extras:["details"]`; otherwise
    /// one entry per cause, its first path with `count`, and `sampled` then;
    /// and what the path policy withheld, by reason, at the walked scope.
    /// An empty page's `observed` entries (the skipped paths a re-run found
    /// matches in, each with its rule) replace the walk's for their causes.
    fn skipped_paths(
        &self,
        observed: Option<SkippedPaths>,
    ) -> Result<Option<SkippedPaths>, ToolError> {
        let named = |cause: SkippedPathCause| {
            observed
                .as_ref()
                .is_some_and(|observed| observed.entries.iter().any(|entry| entry.cause == cause))
        };
        use octocode_contract_types::SkippedPathCause::{DefaultExcluded, Hidden, Ignored};
        let (root, skipped) = (self.root, self.skipped);
        let walked = under(root, Ignored, &skipped.ignored)
            .chain(under(root, DefaultExcluded, &skipped.pruned))
            .chain(under(root, DefaultExcluded, &skipped.generated))
            .chain(under(root, Hidden, &skipped.hidden))
            .filter(|skip| !named(skip.cause))
            .chain(crate::tools::leads::skipped::unlifted(
                &self.root.to_string_lossy(),
                &skipped.withheld,
                Default::default(),
            ));
        let disclosure = crate::tools::leads::skipped::skipped_paths(
            walked,
            lists_every_skipped_path(self.query),
            !self.walk_complete,
        )?;
        Ok(match (disclosure, observed) {
            (Some(mut disclosure), Some(observed)) => {
                disclosure.entries.extend(observed.entries);
                disclosure.sampled |= observed.sampled;
                Some(disclosure)
            }
            (disclosure, observed) => disclosure.or(observed),
        })
    }

    /// The calls that read every clipped value on the page whole
    /// (see [`numbered_calls`]).
    fn expand_values_leads(&self) -> Vec<(String, Value)> {
        let query = self.query;
        let expansions = expand_values(
            query,
            self.paths,
            self.output_root,
            self.files,
            self.layout.context,
            query.multiline != LocalSearchQueryMultiline::Off,
            // A caller-sized (grid) page keeps its rows whatever the value
            // width; a streamed page is cut by serialized size.
            (!self.layout.streamed).then_some(GridPage {
                value_cap: self.layout.budget_cap.unwrap_or(RESPONSE_VALUE_CHAR_BUDGET),
            }),
        );
        numbered_calls("expandValues", &expansions)
    }

    /// The regex reading of a literal first page whose `matchString` reads
    /// as an alternation ([`reads_as_alternation`]) and compiles: its engine
    /// and the warning, when one bounded count of it over the query's own
    /// walk finds more matched lines than the literal search.
    fn alternation_reading(
        &self,
        search_options: &TextSearchOptions,
        cancel: &impl CancellationCheck,
    ) -> Option<(&'static str, String)> {
        let query = self.query;
        let literal = self.scanned.matched_lines?;
        if self.empty
            || !self.walk_complete
            || !self.first_page()
            || query.regex.is_some()
            || query.invert_match == Some(true)
            || !reads_as_alternation(local_search_query::pattern(query))
        {
            return None;
        }
        let engine = regex_engine(local_search_query::pattern(query))?;
        let mut options = search_options.probe(SearchOutput::CountLines);
        options.syntax = if engine == "pcre2" {
            PatternSyntax::Pcre2
        } else {
            PatternSyntax::Regex
        };
        if octocode_engine::search::text_pattern::matches_line_break(
            local_search_query::pattern(query),
            false,
        ) {
            options.multiline = Some(true);
        }
        let (found, cut) = probe_search(
            options,
            self.paths,
            query.default_excludes.defaults(),
            cancel,
        )?;
        let lines = found.stats.matched_lines.filter(|lines| *lines > literal)?;
        let floor = if cut { "at least " } else { "" };
        Some((
            engine,
            format!(
                "matchString ran as literal text ({}); with regex:\"{engine}\" it matches {floor}{lines}.",
                crate::response::pages::counted(
                    u64::from(literal),
                    "matched line",
                    "matched lines"
                )
            ),
        ))
    }

    pub(super) fn first_page(&self) -> bool {
        local_search_query::page(self.query).max(1) == 1
    }

    /// The natural next step on a first page with hits: read the top hit in
    /// context. A shown declaration of the searched symbol reads that
    /// declaration whole when one block read holds it; a larger one (a big
    /// class) reads its hit in context, like any other hit. A partial page
    /// (a binary cut, an unreadable path) still reads its shown hits; a file
    /// cut at a NUL byte is never the read (localFetch rejects binaries).
    pub(super) fn read_lead(&self) -> Option<Value> {
        let query = self.query;
        let readable = self.first_page()
            && !self.layout.list
            && query.result_view != LocalSearchQueryResultView::MatchOnly
            && self.layout.context.is_empty()
            && local_search_query::regex_mode(query) != LocalSearchQueryRegex::Pcre2
            && query.invert_match != Some(true);
        if !readable {
            return None;
        }
        let binary_cut = self.scanned.binary_files.as_deref().unwrap_or_default();
        let text_file = |file: &&SearchFile| {
            !binary_cut
                .iter()
                .any(|cut| std::path::Path::new(cut).ends_with(&file.path))
        };
        let declared = self.definition.and_then(|definition| {
            self.files
                .iter()
                .filter(text_file)
                .find(|file| {
                    self.output_root.join(&file.path).to_string_lossy()
                        == definition.source.as_str()
                })
                .zip(Some(definition))
        });
        match declared {
            Some((file, definition))
                if (definition.end + 1).saturating_sub(definition.start) as usize
                    <= crate::tools::source_view::BLOCK_MAX_LINES =>
            {
                Some(declaration_read(
                    query,
                    self.root,
                    file,
                    definition.start,
                    definition.end,
                ))
            }
            Some((file, _)) => read_handoff(query, self.root, file),
            None => self
                .files
                .iter()
                .find(text_file)
                .and_then(|top| read_handoff(query, self.root, top)),
        }
    }

    /// Files skipped as binary from their leading bytes, when any.
    pub(super) fn leading_binaries(&self) -> Option<u32> {
        let total: u32 = self
            .scanned
            .skipped_binary_extensions
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|group| group.count)
            .sum();
        (total > 0).then_some(total)
    }

    /// Files binary from their leading bytes are outside a text search, not
    /// a coverage gap: disclosed once, on the first page, as a count by
    /// extension with a listing.
    /// The listing also covers binary-cut files too many to name in their
    /// own warning ([`binary_file_list`]).
    pub(super) fn skipped_binaries(&self) -> Option<(String, Value)> {
        if !self.first_page() || self.root.is_file() {
            return None;
        }
        let skipped = self
            .scanned
            .skipped_binary_extensions
            .as_deref()
            .unwrap_or_default();
        let cut = self.scanned.binary_files.as_deref().unwrap_or_default();
        let cut_groups = if cut.len() > MAX_NAMED_BINARY_FILES {
            binary_groups(cut)
        } else {
            Vec::new()
        };
        if skipped.is_empty() && cut_groups.is_empty() {
            return None;
        }
        let listing =
            leading_binary_listing(self.query, &merge_binary_groups(skipped, &cut_groups));
        let warning = if skipped.is_empty() {
            // The cut files' own warning carries the count.
            String::new()
        } else {
            leading_binary_warning(skipped)
        };
        Some((warning, listing))
    }
}

/// The same search without its `maxDepth`, from page 1.
fn unlimited_depth_lead(query: &LocalSearchQuery) -> Value {
    restart_lead(query, |fields| {
        fields.remove("maxDepth");
    })
}

/// The same search without its `fileHas`/`fileLacks` terms, from page 1.
fn unfiltered_terms_lead(query: &LocalSearchQuery) -> Value {
    restart_lead(query, |fields| {
        fields.remove("fileHas");
        fields.remove("fileLacks");
    })
}

/// Whether `text` read as a regex is an alternation: a `|` outside escapes
/// and character classes splits it into alternatives none of which is
/// empty.
fn reads_as_alternation(text: &str) -> bool {
    // Chars in each alternative so far.
    let mut alternatives = vec![0usize];
    let mut chars = text.chars();
    let mut class = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '[' => class = true,
            ']' => class = false,
            '|' if !class => {
                alternatives.push(0);
                continue;
            }
            _ => {}
        }
        if let Some(last) = alternatives.last_mut() {
            *last += 1;
        }
    }
    alternatives.len() > 1 && alternatives.iter().all(|chars| *chars > 0)
}

/// The engine `text` compiles in as a regex: `rust`, else `pcre2`.
fn regex_engine(text: &str) -> Option<&'static str> {
    use octocode_engine::search::text_pattern::validate_text_pattern;
    if validate_text_pattern(text, false, false).valid {
        Some("rust")
    } else {
        validate_text_pattern(text, false, true)
            .valid
            .then_some("pcre2")
    }
}

/// The same search with `matchString` read as a regex under `engine`, and
/// across lines when the regex matches a line break, so the lead runs.
fn regex_lead(query: &LocalSearchQuery, engine: &str) -> Value {
    restart_lead(query, |fields| {
        fields.insert("regex".into(), json!(engine));
        if query.multiline == LocalSearchQueryMultiline::Off
            && octocode_engine::search::text_pattern::matches_line_break(
                local_search_query::pattern(query),
                false,
            )
        {
            fields.insert("multiline".into(), json!("on"));
        }
    })
}

/// Whether the row lists every skipped path (`extras:["details"]`) instead
/// of one entry per cause and rule.
pub(super) fn lists_every_skipped_path(query: &LocalSearchQuery) -> bool {
    query
        .extras
        .contains(&octocode_contract_types::ExtraKind::Details)
}

/// Matches at which a line view leads to its per-file counts.
pub(super) const LARGE_RESULT_MATCHES: u32 = 1_000;

/// The same search as per-file match counts, from page 1.
pub(super) fn count_lead(query: &LocalSearchQuery) -> Value {
    restart_lead(query, |fields| {
        fields.insert("resultView".into(), json!("countMatches"));
        // Fields that shape match rows (or are valid only under matchOnly)
        // have no meaning in a count view.
        for field in [
            "contextLines",
            "contextBefore",
            "contextAfter",
            "matchContentLength",
            "pageSize",
            "matchPageSize",
            "unique",
        ] {
            fields.remove(field);
        }
    })
}

/// The query as a fresh page-1 search (no [`CURSOR_FIELDS`]) with `edit`
/// applied, as the localSearch call that runs it.
pub(super) fn restart_lead(
    query: &LocalSearchQuery,
    edit: impl FnOnce(&mut serde_json::Map<String, Value>),
) -> Value {
    let mut lead = query_row(query, &CURSOR_FIELDS);
    if let Some(fields) = lead.as_object_mut() {
        edit(fields);
    }
    Continuation::new(ToolId::LocalSearch, lead).build()
}

/// A structureSearch `files` listing of the searched scope in the query's
/// walk scope (`includeIgnored`, `defaultExcludes`; a `files` listing always
/// shows dot entries). With `keep_globs`, the query's own include/exclude
/// globs: what they match there (a glob that reaches above `path` is left
/// out: it never matches, and the row's warning names it). Without, when
/// those globs matched no file, the listing shows the names that exist.
pub(super) fn scope_listing(query: &LocalSearchQuery, keep_globs: bool) -> Value {
    let mut listing = json!({"operation": "files", "path": query.path.as_str()});
    let matchable = |globs: &[String]| {
        globs
            .iter()
            .filter(|glob| keep_globs && !crate::policy::include::reaches_above(glob))
            .cloned()
            .collect::<Vec<_>>()
    };
    let (include, exclude) = (matchable(&query.include), matchable(&query.exclude));
    if !include.is_empty() {
        listing["include"] = json!(include);
    }
    if !exclude.is_empty() {
        listing["exclude"] = json!(exclude);
    }
    walk_scope(query, &mut listing);
    Continuation::new(ToolId::StructureSearch, listing).build()
}

/// The search's walk scope on a structureSearch listing.
fn walk_scope(query: &LocalSearchQuery, listing: &mut Value) {
    if query.include_ignored == Some(true) {
        listing["includeIgnored"] = json!(true);
    }
    if !query.default_excludes.defaults() {
        listing["defaultExcludes"] = json!(false);
    }
}

/// The searched scope's top directories (a structureSearch tree one level
/// deep): a search past the collection cap splits by them, each part
/// collecting its own files.
pub(super) fn scope_directories(query: &LocalSearchQuery) -> Value {
    let mut tree = json!({"operation": "tree", "path": query.path.as_str(), "maxDepth": 1});
    walk_scope(query, &mut tree);
    Continuation::new(ToolId::StructureSearch, tree).build()
}

#[cfg(test)]
mod alternation_tests {
    use super::reads_as_alternation;

    /// A `|` splits alternatives only outside escapes and classes, and every
    /// alternative must hold text.
    #[test]
    fn an_alternation_has_text_on_every_side() {
        for text in ["a|b", "(a|b)c", "foo|bar|baz", r"a\.b|c"] {
            assert!(reads_as_alternation(text), "{text}");
        }
        for text in ["a", "a||b", "|a", "a|", r"a\|b", "[a|b]", ""] {
            assert!(!reads_as_alternation(text), "{text}");
        }
    }
}

/// Executable calls from `reads` (one-row calls, see [`expand_values`]):
/// each run of one tool's rows in calls of as many rows as that tool
/// accepts, the first named `name` and the rest `name2`, `name3`, …, so
/// every read is one call away. A default page is cut so its reads fit one
/// call (see [`stream_pages`]); a caller-sized page may need more.
fn numbered_calls(name: &str, reads: &[Value]) -> Vec<(String, Value)> {
    let mut calls = Vec::new();
    for run in reads.chunk_by(|a, b| a["tool"] == b["tool"]) {
        let Some(tool) = run[0]["tool"].as_str().and_then(ToolId::from_name) else {
            continue;
        };
        let rows = run
            .iter()
            .filter_map(crate::response::result::continuation_row)
            .cloned()
            .collect::<Vec<_>>();
        for chunk in rows.chunks(crate::response::result::rows_per_call(tool)) {
            let key = crate::id::numbered(name, calls.len());
            let call = Continuation::input(tool, json!({ "queries": chunk }))
                .confidence(Confidence::Exact)
                .build();
            calls.push((key, call));
        }
    }
    calls
}

#[cfg(test)]
mod json_size_policy_tests {
    #[test]
    fn invalid_lead_json_exceeds_every_budget() {
        let invalid = std::collections::BTreeMap::from([((1, 2), "value")]);
        assert_eq!(
            octocode_engine::json_size::bytes(&invalid).unwrap_or(usize::MAX),
            usize::MAX
        );
    }
}
