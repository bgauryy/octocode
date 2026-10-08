//! Coverage disclosure and leads: what a page could not cover (binary,
//! default-excluded, policy-withheld, unreadable) and the follow-up calls
//! (read, references, re-runs, listings) a page offers.

use super::executor::*;
use super::layout::*;
use super::types::*;
use crate::policy::path::PathPolicy;
use crate::policy::prune::DefaultsFlag;
use crate::security::ContentSecurity;
use crate::tools::cancel::CancellationCheck;
use crate::tools::id::ToolId;
use crate::tools::local_fetch::MAX_READ_RANGES;
use crate::tools::result::Continuation;
use octocode_engine::{portable::search_text_cancellable, types::TextSearchOptions};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// What the scan could not cover, and how the row says so.
pub(super) struct Coverage {
    pub(super) error_count: u32,
    /// A file was searched only up to its first NUL (`binaryQuit`).
    pub(super) binary_cut: bool,
    pub(super) binary_files: String,
    pub(super) pcre2_deadline: bool,
    pub(super) skip_hint: Option<String>,
    /// include/exclude filtered out every file: nothing was searched.
    pub(super) scope_miss: bool,
    /// Entries the security path policy withheld from the walk.
    pub(super) withheld: usize,
}

impl Coverage {
    pub(super) fn of(
        query: &LocalSearchQuery,
        stats: &SearchStats,
        scanned: &octocode_engine::types::TextSearchStats,
        root: &std::path::Path,
        output_root: &std::path::Path,
        skipped: &crate::policy::discovery::WalkSkips,
    ) -> Self {
        let cap_has = |name: &str| {
            stats
                .cap_reason
                .as_deref()
                .is_some_and(|reason| reason.split(", ").any(|r| r == name))
        };
        let skip_hint = skipped_target_hint(
            root.is_file(),
            stats.files_searched,
            stats.cap_reason.as_deref(),
        )
        .or_else(|| {
            (root.is_file() && scanned.skipped_binary_count.unwrap_or(0) > 0).then(|| {
                "The target file is binary from its leading bytes (a NUL in its header); it was not searched, and no text tool reads it.".into()
            })
        });
        Self {
            error_count: stats.error_count.unwrap_or(0),
            binary_cut: cap_has("binaryQuit"),
            binary_files: binary_file_list(
                scanned.binary_files.as_deref().unwrap_or_default(),
                scanned.binary_file_count.unwrap_or(0),
                output_root,
            ),
            pcre2_deadline: cap_has("pcre2Deadline"),
            scope_miss: !root.is_file()
                && stats.files_searched == 0
                && skip_hint.is_none()
                && (!query.include.is_empty() || !query.exclude.is_empty()),
            skip_hint,
            withheld: skipped.withheld.total(),
        }
    }

    /// Unreadable paths or a binary cut leave coverage incomplete: absence
    /// is unproven, and prefix matches before a NUL are not the file's full
    /// set.
    pub(super) fn gap(&self) -> bool {
        self.error_count > 0 || self.binary_cut
    }

    pub(super) fn warnings(
        &self,
        files: &[SearchFile],
        layout: &Layout,
        shown_redacted: usize,
        unverified: bool,
        empty: bool,
    ) -> Vec<String> {
        let mut warnings = vec![];
        let any_truncated = files.iter().any(|file| {
            file.matches
                .as_ref()
                .is_some_and(|matches| matches.iter().any(|matched| matched.truncated))
        });
        if layout.budget_binds && any_truncated {
            warnings.push(
                "Match values were shortened to keep the total response within its size budget. Every match row and its line anchor is preserved; next.expandValues reads the shortened values whole.".into(),
            );
        } else if any_truncated {
            warnings.push(
                "Some match values were truncated to matchContentLength; originalChars and returnedChars describe each shortened value. Counts and row pagination are unchanged. next.expandValues reads them whole.".into(),
            );
        }
        if shown_redacted > 0 && !layout.list {
            warnings.push(format!(
                "redactedMatches: {shown_redacted} returned match value(s) had secret-shaped text replaced by [REDACTED…] placeholders; those values are not verbatim source."
            ));
        }
        if unverified {
            warnings.push(
                "Some match values were redacted because their source file could not be re-read to check clipped text for secrets. Use localFetch at the returned anchors.".into(),
            );
        }
        if self.pcre2_deadline {
            warnings.push(
                "The PCRE2 (regex:\"pcre2\") search hit its wall-clock deadline and was stopped; results cover only the files finished before it. Narrow the pattern/scope, or use regex:\"literal\" or the default engine.".into(),
            );
        }
        if self.binary_cut {
            warnings.push(format!(
                "binaryFileSkipped: {} searched only up to the first NUL byte; no text tool reads past it.",
                self.binary_files
            ));
        }
        // Hints are shown only on empty/error rows; a partial row keeps the
        // unreadable-path explanation as a warning.
        if self.error_count > 0 && empty {
            warnings.push(unreadable_hint(self.error_count));
        }
        warnings
    }

    /// Rows carry prose hints only when empty or failed.
    pub(super) fn empty_hints(&self, query: &LocalSearchQuery, empty: bool) -> Vec<String> {
        if !empty {
            return vec![];
        }
        if self.scope_miss {
            vec!["Nothing searched: include/exclude matched no file. hints.viewTree lists what they match.".into()]
        } else if self.error_count > 0 {
            vec![unreadable_hint(self.error_count)]
        } else if let Some(hint) = &self.skip_hint {
            vec![hint.clone()]
        } else if self.withheld > 0 {
            // Not a spelling problem: the policy notice in `warnings` names
            // what was withheld (once, there).
            vec!["No matches where searched; the withheld entries named in warnings may hold it (unproven).".into()]
        } else if self.binary_cut {
            vec![
                "No matches, but binary file(s) were searched only up to their first NUL byte; absence is not proven for them.".into(),
                empty_hint(query),
            ]
        } else {
            vec![empty_hint(query)]
        }
    }
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
    /// What the walk left out: policy-withheld and default-excluded files.
    pub(super) skipped: &'a crate::policy::discovery::WalkSkips,
    /// Nothing capped and no coverage gap.
    pub(super) complete: bool,
    pub(super) empty: bool,
}

impl Found<'_> {
    /// Leads in the tool's own rank: a read of the top hit, the symbol's
    /// lspSearch lead, then reads of clipped values, the skipped binaries
    /// and the literal search.
    pub(super) fn add_leads(
        &self,
        next: &mut Option<Value>,
        hints: &mut Vec<String>,
        warnings: &mut Vec<String>,
        search_options: &TextSearchOptions,
        probe_options: Option<TextSearchOptions>,
        cancel: &impl CancellationCheck,
    ) {
        let query = self.query;
        let mut add = |name: String, lead: Value| {
            if let Some(map) = next.get_or_insert_with(|| json!({})).as_object_mut() {
                map.insert(name, lead);
            }
        };
        if let Some(read) = self.read_lead() {
            add("read".into(), read);
        }
        if let (Some(symbol), Some(definition)) = (self.symbol, self.definition)
            && let Some(row) = crate::tools::lsp_search::verify_query(
                &definition.source,
                symbol,
                u64::from(definition.line),
                crate::tools::lsp_search::Verify::for_kind(&definition.kind),
            )
        {
            add(
                crate::tools::lsp_search::lead_name(&row),
                Continuation::new(ToolId::LspSearch, row)
                    .why("Uses of the searched symbol's declaration.")
                    .build(),
            );
        }
        // Every clipped value on the page stays reachable whole.
        let expansions = expand_values(
            query,
            self.paths,
            self.output_root,
            self.files,
            self.layout.context_lines,
            query.multiline != LocalSearchQueryMultiline::Off,
            // A caller-sized (grid) page keeps its rows whatever the value
            // width; a streamed page is cut by serialized size.
            (!self.layout.streamed).then_some(GridPage {
                value_cap: self.layout.budget_cap.unwrap_or(RESPONSE_VALUE_CHAR_BUDGET),
            }),
        );
        for (index, expansion) in expansions.into_iter().enumerate() {
            let name = if index == 0 {
                "expandValues".to_owned()
            } else {
                format!("expandValues{}", index + 1)
            };
            add(name, expansion);
        }
        if let Some((warning, listing)) = self.skipped_binaries() {
            warnings.extend(Some(warning).filter(|warning| !warning.is_empty()));
            add("binarySkipped".into(), listing);
        }
        // What the walk left out is disclosed once, on the first page: the
        // security policy's withheld entries (nothing lifts them), and the
        // default excludes with the same search over them.
        if self.first_page() {
            warnings.extend(self.skipped.withheld.notice());
            if !self.empty
                && let Some(names) = excluded_names(self.scanned, self.skipped)
            {
                warnings.push(format!(
                    "Default excludes skipped {names}; the same search with defaultExcludes:false covers them."
                ));
                add("includeIgnored".into(), excluded_lead(query));
            }
        }
        // Thousands of hits page for dozens of calls: one count call ranks
        // the files first, so the next read or narrowing is chosen, not paged.
        if self.first_page()
            && !is_count_or_list_view(query)
            && let Some(total) = self
                .scanned
                .match_count
                .filter(|total| *total >= LARGE_RESULT_MATCHES)
        {
            add("countFiles".into(), count_lead(query));
            hints.push(format!(
                "{total} matches: run hints.countFiles to rank files by hits, or narrow path/include."
            ));
        }
        // An unset `regex` is inferred from the text: say which reading ran
        // when the literal and regex readings differ.
        let inferred = match query.regex_mode() {
            _ if query.regex.is_some() => None,
            // Only a `.` or a balanced group reads differently as a regex.
            LocalSearchQueryRegex::Literal => (query.match_string.contains('.')
                || query.match_string.contains('(') && query.match_string.contains(')'))
            .then_some("matchString ran as literal text; regex:\"rust\" runs it as a regex."),
            // An obvious regex (`a.*b`, `\bfoo`, `a|b`) ran as one: no note.
            _ if unambiguous_regex(&query.match_string) => None,
            _ => Some("matchString ran as a regex; regex:\"literal\" matches it exactly."),
        };
        warnings.extend(inferred.map(str::to_owned));
        if self.empty && self.complete && self.first_page() {
            disclose_variants(query, self.paths, search_options, next, hints, cancel);
        }
        if self.empty && self.complete && !self.root.is_file() {
            disclose_unsearched(
                query,
                self.paths,
                probe_options,
                excluded_names(self.scanned, self.skipped),
                next,
                hints,
                cancel,
            );
        }
    }

    pub(super) fn first_page(&self) -> bool {
        self.query.page().max(1) == 1
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
            && self.layout.context_lines == 0
            && query.regex_mode() != LocalSearchQueryRegex::Pcre2
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
                    <= crate::tools::local_fetch::BLOCK_MAX_LINES =>
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

/// The searched symbol's declaration among the shown hits.
pub(super) struct Definition {
    /// Absolute path of the declaring file.
    pub(super) source: String,
    /// The declaration's name line.
    pub(super) line: u32,
    pub(super) kind: String,
    /// Its first and last line.
    pub(super) start: u32,
    pub(super) end: u32,
}

/// localFetch read of a shown declaration whole, from its first line to its
/// last; localFetch pages a long one.
pub(super) fn declaration_read(
    query: &LocalSearchQuery,
    root: &std::path::Path,
    file: &SearchFile,
    start: u32,
    end: u32,
) -> Value {
    Continuation::new(
        ToolId::LocalFetch,
        json!({"path": joined_path(query, root, file), "ranges": [format!("{start}-{end}")]}),
    )
    .why("Read the declaration whole.")
    .build()
}

/// A shown file's path joined to the caller's own `path`, so a read
/// resolves wherever the search did.
pub(super) fn joined_path(
    query: &LocalSearchQuery,
    root: &std::path::Path,
    file: &SearchFile,
) -> String {
    if root.is_file() {
        query.path.to_string()
    } else {
        std::path::Path::new(query.path.as_str())
            .join(&file.path)
            .to_string_lossy()
            .into_owned()
    }
}

/// Re-run an empty search over what the defaults leave out: ignored and
/// hidden entries, and the default-excluded directories (`build/`,
/// `node_modules/`, …). A match there, or default-excluded directories the
/// re-run could not finish, is named with the lead that searches them.
pub(super) fn disclose_unsearched(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    probe_options: Option<TextSearchOptions>,
    excluded: Option<String>,
    next: &mut Option<Value>,
    hints: &mut Vec<String>,
    cancel: &impl CancellationCheck,
) {
    let Some(options) = probe_options else {
        return;
    };
    let probe = ignored_probe(options, paths, cancel);
    let (count, cut, reasons) = match probe {
        Some(found) => (found.count, found.cut, found.reasons),
        None => (0, true, Vec::new()),
    };
    if count == 0 && (!cut || excluded.is_none()) {
        return;
    }
    // The action leads: the response stage clips long hints at the end.
    let hint = if count == 0 {
        let names = excluded
            .map(|names| format!(" ({names})"))
            .unwrap_or_default();
        format!(
            "Run hints.includeIgnored before claiming absence: default-excluded paths were not searched{names}."
        )
    } else {
        let at_least = if cut { "at least " } else { "" };
        let why: Vec<String> = reasons.into_iter().chain(excluded).collect();
        if why.is_empty() {
            format!(
                "Run hints.includeIgnored: {at_least}{count} file(s) match in ignored, hidden or default-excluded paths."
            )
        } else {
            format!(
                "Run hints.includeIgnored: {at_least}{count} file(s) match in skipped paths ({}).",
                why.join(", ")
            )
        }
    };
    hints.insert(0, hint);
    if let Some(map) = next.get_or_insert_with(|| json!({})).as_object_mut() {
        map.insert("includeIgnored".into(), unsearched_lead(query));
    }
}

/// Distinct names of the pruned directories, each with how many were
/// pruned when more than one: `node_modules ×3, build`.
pub(super) fn pruned_names(pruned: &[String]) -> String {
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    for dir in pruned {
        let name = dir.rsplit('/').next().unwrap_or(dir);
        *counts.entry(name).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(name, count)| {
            if count > 1 {
                format!("{name}/ ×{count}")
            } else {
                format!("{name}/")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// What the default excludes left out of the walk, as one clause: the
/// pruned directories and the skipped generated files, each by name with
/// its count (`2 dirs (target/, dist/) and 3 files (*.lock ×2, *.min.js)`).
pub(super) fn excluded_names(
    scanned: &octocode_engine::types::TextSearchStats,
    skipped: &crate::policy::discovery::WalkSkips,
) -> Option<String> {
    let pruned = scanned.pruned_dirs.as_deref().unwrap_or_default();
    let mut parts = Vec::new();
    if !pruned.is_empty() {
        let noun = if pruned.len() == 1 { "dir" } else { "dirs" };
        parts.push(format!(
            "{} {noun} ({})",
            pruned.len(),
            pruned_names(pruned)
        ));
    }
    let files: usize = skipped.generated.values().sum();
    if files > 0 {
        let noun = if files == 1 { "file" } else { "files" };
        let patterns = skipped
            .generated
            .iter()
            .map(|(pattern, count)| {
                if *count > 1 {
                    format!("{pattern} ×{count}")
                } else {
                    pattern.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("{files} {noun} ({patterns})"));
    }
    (!parts.is_empty()).then(|| parts.join(" and "))
}

/// The same search with the default excludes off, from page 1: a result
/// that found matches keeps its ignore and hidden settings.
/// Words of a compound identifier (`parseConfig`, `parse_config`,
/// `ParseConfig`, `parse-config`, `HTTPServer`): split at `_`, `-` and case
/// boundaries; `None` for one word or a non-identifier.
fn identifier_words(term: &str) -> Option<Vec<String>> {
    let valid = term.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && term
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !valid {
        return None;
    }
    let chars: Vec<char> = term.chars().collect();
    let mut words = Vec::new();
    let mut word = String::new();
    for (index, &c) in chars.iter().enumerate() {
        if c == '_' || c == '-' {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            continue;
        }
        let previous = index.checked_sub(1).map(|i| chars[i]);
        let next = chars.get(index + 1).copied();
        // `aB` starts a word; so does the `B` of `HTTPServer` before `e`.
        let boundary = c.is_ascii_uppercase()
            && previous.is_some_and(|p| {
                p.is_ascii_lowercase()
                    || p.is_ascii_digit()
                    || (p.is_ascii_uppercase() && next.is_some_and(|n| n.is_ascii_lowercase()))
            });
        if boundary && !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
        word.push(c.to_ascii_lowercase());
    }
    if !word.is_empty() {
        words.push(word);
    }
    (words.len() >= 2).then_some(words)
}

/// The other case styles of a compound identifier: snake, camel, Pascal,
/// SCREAMING and kebab, without the term itself.
pub(super) fn identifier_variants(term: &str) -> Option<Vec<String>> {
    let words = identifier_words(term)?;
    let capitalized = |word: &String| {
        let mut chars = word.chars();
        chars
            .next()
            .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
            .unwrap_or_default()
    };
    let camel = words[0].clone() + &words[1..].iter().map(capitalized).collect::<String>();
    let mut variants = vec![
        words.join("_"),
        camel,
        words.iter().map(capitalized).collect::<String>(),
        words.join("_").to_ascii_uppercase(),
        words.join("-"),
    ];
    variants.retain(|variant| variant != term);
    variants.dedup();
    (!variants.is_empty()).then_some(variants)
}

/// A bounded files-only walk over the query's own scope for `variants`;
/// how many files hold one (0 when the walk failed or found none).
fn variant_files(
    mut options: TextSearchOptions,
    variants: &[String],
    paths: &PathPolicy,
    cancel: &impl CancellationCheck,
) -> usize {
    options.pattern = variants
        .iter()
        .map(|variant| regex::escape(variant))
        .collect::<Vec<_>>()
        .join("|");
    options.fixed_string = Some(false);
    options.perl_regex = Some(false);
    options.case_sensitive = Some(true);
    options.case_insensitive = Some(false);
    options.files_only = Some(true);
    options.files_without_match = Some(false);
    options.count_lines_per_file = Some(false);
    options.count_matches_per_file = Some(false);
    options.only_matching = Some(false);
    options.invert_match = Some(false);
    options.context_lines = Some(0);
    options.digest_max_bytes = None;
    let started = std::time::Instant::now();
    let deadline = std::time::Duration::from_millis(IGNORED_PROBE_MS);
    search_text_cancellable(options, Arc::new(paths.clone()), &|| {
        cancel.check().is_err() || started.elapsed() > deadline
    })
    .map_or(0, |found| found.files.len())
}

/// The same search over the identifier's case-style variants.
fn variants_lead(query: &LocalSearchQuery, variants: &[String]) -> Value {
    let mut lead = restart_fields(query);
    if let Some(fields) = lead.as_object_mut() {
        let pattern = variants
            .iter()
            .map(|variant| regex::escape(variant))
            .collect::<Vec<_>>()
            .join("|");
        fields.insert("matchString".into(), json!(pattern));
        fields.insert("regex".into(), json!("rust"));
        fields.remove("caseMode");
    }
    Continuation::new(ToolId::LocalSearch, lead)
        .why("The identifier's other case styles.")
        .build()
}

/// An empty search for a compound identifier: say when its other case
/// styles match, so a spelling miss does not read as absence.
pub(super) fn disclose_variants(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    options: &TextSearchOptions,
    next: &mut Option<Value>,
    hints: &mut Vec<String>,
    cancel: &impl CancellationCheck,
) {
    if query
        .regex
        .is_some_and(|regex| regex != LocalSearchQueryRegex::Literal)
    {
        return;
    }
    let Some(variants) = identifier_variants(&query.match_string) else {
        return;
    };
    let count = variant_files(options.clone(), &variants, paths, cancel);
    if count == 0 {
        return;
    }
    hints.insert(
        0,
        format!(
            "No match for {}; its other spellings match in {count} file(s): run hints.didYouMean.",
            query.match_string
        ),
    );
    if let Some(map) = next.get_or_insert_with(|| json!({})).as_object_mut() {
        map.insert("didYouMean".into(), variants_lead(query, &variants));
    }
}

/// Matches at which a line view leads to its per-file counts.
pub(super) const LARGE_RESULT_MATCHES: u32 = 1_000;

/// Views that already answer per file (counts) or list paths.
fn is_count_or_list_view(query: &LocalSearchQuery) -> bool {
    matches!(
        query.result_view,
        LocalSearchQueryResultView::CountMatches
            | LocalSearchQueryResultView::CountLines
            | LocalSearchQueryResultView::Files
            | LocalSearchQueryResultView::FilesWithout
    )
}

/// The same search as per-file match counts, from page 1.
pub(super) fn count_lead(query: &LocalSearchQuery) -> Value {
    let mut lead = restart_fields(query);
    if let Some(fields) = lead.as_object_mut() {
        fields.insert("resultView".into(), json!("countMatches"));
        for field in [
            "contextLines",
            "matchContentLength",
            "pageSize",
            "matchPageSize",
        ] {
            fields.remove(field);
        }
    }
    Continuation::new(ToolId::LocalSearch, lead)
        .why("Per-file match counts, most hits first.")
        .build()
}

pub(super) fn excluded_lead(query: &LocalSearchQuery) -> Value {
    let mut lead = restart_fields(query);
    if let Some(fields) = lead.as_object_mut() {
        fields.insert("defaultExcludes".into(), json!(false));
    }
    Continuation::new(ToolId::LocalSearch, lead).build()
}

/// The query as a fresh page-1 search: no page, match page or snapshot.
pub(super) fn restart_fields(query: &LocalSearchQuery) -> Value {
    let mut lead = serde_json::to_value(query).unwrap_or_else(|_| json!({}));
    if let Some(fields) = lead.as_object_mut() {
        fields.retain(|key, value| {
            !value.is_null() && !matches!(key.as_str(), "page" | "matchPage" | "snapshot")
        });
    }
    lead
}

/// A structureSearch `files` listing of the searched scope with the query's
/// own include/exclude globs: what a scope-miss row's globs match there.
pub(super) fn scope_listing(query: &LocalSearchQuery) -> Value {
    let mut listing = json!({"operation": "files", "path": query.path.as_str()});
    if !query.include.is_empty() {
        listing["include"] = json!(query.include);
    }
    if !query.exclude.is_empty() {
        listing["exclude"] = json!(query.exclude);
    }
    Continuation::new(ToolId::StructureSearch, listing)
        .why("List what the include/exclude globs match.")
        .build()
}

/// Whether `text` reads only as a regex: it holds `.*`, `.+`, `.?`, a
/// `\` + letter class (`\w`, `\d`, `\s`, `\b`), or a `|` outside `||`.
/// Ambiguous texts (`a?.b`, `$scope`, `arr[0]`, `x + y`) keep the inferred-
/// regex note: their literal reading is plausible.
pub(super) fn unambiguous_regex(text: &str) -> bool {
    let bytes = text.as_bytes();
    let wildcard = text.contains(".*") || text.contains(".+") || text.contains(".?");
    // `\w`, `\b`, or escaped punctuation such as `\(`: only a regex escapes.
    let escape = bytes.windows(2).any(|pair| {
        pair[0] == b'\\' && (pair[1].is_ascii_alphabetic() || pair[1].is_ascii_punctuation())
    });
    let alternation = bytes.iter().enumerate().any(|(index, byte)| {
        *byte == b'|'
            && bytes.get(index + 1) != Some(&b'|')
            && (index == 0 || bytes[index - 1] != b'|')
    });
    // A repeated class or group: `[a-z_]+`, `(ab)*`. A trailing `?` stays
    // ambiguous: Rust's `list[i]?` and `read(path)?` are code.
    let quantified = bytes
        .windows(2)
        .any(|pair| matches!(pair[0], b']' | b')') && matches!(pair[1], b'+' | b'*' | b'{'));
    // A class range: `[a-z]`, `[0-9_]`.
    let range = text.split('[').skip(1).any(|rest| {
        rest.split_once(']')
            .is_some_and(|(class, _)| class.len() >= 3 && class[1..class.len() - 1].contains('-'))
    });
    wildcard || escape || alternation || quantified || range || text.starts_with('^')
}

/// The same search over everything the defaults leave out, from page 1.
pub(super) fn unsearched_lead(query: &LocalSearchQuery) -> Value {
    let mut lead = restart_fields(query);
    if let Some(fields) = lead.as_object_mut() {
        fields.insert("noIgnore".into(), json!(true));
        fields.insert("hidden".into(), json!(true));
        fields.insert("defaultExcludes".into(), json!(false));
    }
    Continuation::new(ToolId::LocalSearch, lead).build()
}

/// Shown files a page parses for enclosing names and the searched
/// symbol's declaration (a first page holds at most this many); rows of
/// later files stay unnamed, and the page says so.
pub(super) const ENCLOSING_MAX_FILES: usize = DEFAULT_SNIPPET_PAGE_SIZE as usize;

/// Serialized chars an `enclosing` field adds besides its value: `,"enclosing":`.
pub(super) const ENCLOSING_FIELD_CHARS: usize = 13;

/// The current outline of a shown file, or `None` when it is too large,
/// unreadable, unsupported, or no longer the bytes a stored scan matched.
pub(super) fn shown_outline(
    source: &std::path::Path,
    expected: Option<Option<super::manifest::Digest>>,
    security: &ContentSecurity,
) -> Option<super::enclosing::Outline> {
    let limit = crate::tools::ast_search::MAX_PARSE_SOURCE_BYTES;
    let bytes = crate::tools::source::read_bounded(source, limit).ok()?;
    if let Some(expected) = expected
        && expected != Some(<[u8; 32]>::from(Sha256::digest(&bytes)))
    {
        return None;
    }
    outline_of(&bytes, source, security)
}

/// Largest file outlined for `enclosing` names. Parsing is linear (a 4.5 MB
/// generated Rust file took 1 s of a 1.2 s search, 2026-10-08) and files this
/// large are almost always generated; their hits keep no enclosing name.
pub(super) const ENCLOSING_MAX_BYTES: usize = 1024 * 1024;

/// The outline of a shown file's `bytes`, read within the parse cap; none
/// for a file over [`ENCLOSING_MAX_BYTES`].
pub(super) fn outline_of(
    bytes: &[u8],
    source: &std::path::Path,
    security: &ContentSecurity,
) -> Option<super::enclosing::Outline> {
    if bytes.len() > ENCLOSING_MAX_BYTES {
        return None;
    }
    let limit = crate::tools::ast_search::MAX_PARSE_SOURCE_BYTES;
    let text = security.decode_source_bytes(bytes, limit).ok()?;
    super::enclosing::Outline::of(&text, &source.to_string_lossy())
}

/// Lines a shown row stands for: every matched line of a merged block.
pub(super) fn row_lines(row: &SearchMatch) -> Vec<u32> {
    row.match_lines.clone().unwrap_or_else(|| vec![row.line])
}

/// Set `enclosing` on the hit rows of a page's first [`ENCLOSING_MAX_FILES`]
/// files, in page order while the names fit the page budget, and find the
/// first shown hit that declares `symbol`. Each file is parsed once. Rows a
/// name was found for but not shown (budget or parse bound) are counted in
/// the returned note, never dropped silently.
#[allow(clippy::too_many_arguments)]
pub(super) fn annotate_enclosing(
    files: &mut [SearchFile],
    symbol: Option<&str>,
    declaring: Option<&str>,
    output_root: &std::path::Path,
    page_budget: usize,
    expected: &dyn Fn(&std::path::Path) -> Option<Option<super::manifest::Digest>>,
    mut outlines: super::verify::Outlines,
    security: &ContentSecurity,
) -> (Option<Definition>, Option<String>) {
    let mut names: Vec<Vec<Option<super::types::Enclosing>>> = Vec::with_capacity(files.len());
    let mut definition = None;
    let mut unparsed_rows = 0usize;
    let mut oversized_rows = 0usize;
    for (position, file) in files.iter().enumerate() {
        let Some(matches) = file.matches.as_ref().filter(|rows| !rows.is_empty()) else {
            names.push(Vec::new());
            continue;
        };
        if position >= ENCLOSING_MAX_FILES {
            unparsed_rows += matches.len();
            names.push(Vec::new());
            continue;
        }
        let source = output_root.join(&file.path);
        // The secret check already read and outlined most shown files.
        let outline = match outlines.remove(&source) {
            Some(outline) => outline,
            None => shown_outline(&source, expected(&source), security),
        };
        let Some(outline) = outline else {
            if std::fs::metadata(&source).is_ok_and(|meta| meta.len() > ENCLOSING_MAX_BYTES as u64)
            {
                oversized_rows += matches.len();
            }
            names.push(Vec::new());
            continue;
        };
        if definition.is_none()
            && let Some(symbol) = symbol
            && let Some((line, (kind, start, end))) = matches
                .iter()
                .flat_map(row_lines)
                .find_map(|line| Some((line, outline.declaration(symbol, line)?)))
        {
            definition = Some(Definition {
                source: source.to_string_lossy().into_owned(),
                line,
                kind: kind.to_owned(),
                start,
                end,
            });
        }
        names.push(row_owners(&outline, matches, declaring));
    }
    // Names go on in page order while the page stays within its budget
    // (the caller's budget already leaves out the rendered path prefixes).
    let mut used = json_bytes(&*files);
    let mut over_budget = 0usize;
    for (file, names) in files.iter_mut().zip(names) {
        for (row, name) in file.matches.iter_mut().flatten().zip(names) {
            let Some(name) = name else { continue };
            let added = json_bytes(&name).saturating_add(ENCLOSING_FIELD_CHARS);
            if over_budget == 0 && used.saturating_add(added) <= page_budget {
                used += added;
                row.enclosing = Some(name);
            } else {
                over_budget += 1;
            }
        }
    }
    let mut note = match (over_budget, unparsed_rows) {
        (0, 0) => None,
        (0, rows) => Some(format!(
            "Enclosing declarations (enclosing) are named for the first {ENCLOSING_MAX_FILES} files only; {rows} hit rows after them have none."
        )),
        (rows, _) => Some(format!(
            "Enclosing declarations (enclosing) omitted on {rows} later rows to keep the page within its size budget."
        )),
    };
    if oversized_rows > 0 {
        let oversized = format!(
            "Enclosing declarations (enclosing) are not named in files over 1 MiB; {oversized_rows} hit rows there have none."
        );
        note = Some(match note {
            Some(note) => format!("{note} {oversized}"),
            None => oversized,
        });
    }
    (definition, note)
}

/// Bytes `value` serializes to as JSON, counted without building the text;
/// `usize::MAX` when it does not serialize.
pub(super) fn json_bytes<T: serde::Serialize + ?Sized>(value: &T) -> usize {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0 += buf.len();
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    serde_json::to_writer(&mut count, value).map_or(usize::MAX, |()| count.0)
}

/// Each row's `enclosing` declaration. A run of consecutive rows in one declaration names
/// it once, on its first row, with the declaration's last line; the rows
/// after it inside that range share it. With `declaring` (a declaration
/// search), only rows that declare that symbol are named.
pub(super) fn row_owners(
    outline: &super::enclosing::Outline,
    rows: &[SearchMatch],
    declaring: Option<&str>,
) -> Vec<Option<super::types::Enclosing>> {
    let owners = rows
        .iter()
        .map(|row| {
            let named = declaring.is_none_or(|symbol| {
                row_lines(row)
                    .iter()
                    .any(|line| outline.declares(symbol, *line))
            });
            named.then(|| outline.owner(row_lines(row)[0])).flatten()
        })
        .collect::<Vec<_>>();
    owners
        .iter()
        .enumerate()
        .map(|(index, owner)| {
            let owner = (*owner)?;
            if index > 0 && owners[index - 1] == Some(owner) {
                return None;
            }
            outline.label(owner)
        })
        .collect()
}

/// Hits a handed-off read may cover (each opens a ±6-line window).
pub(super) const READ_HANDOFF_MAX_HITS: usize = 20;

/// Lines a handed-off read shows on each side of a hit.
/// A range end past any file: localFetch clamps it to the last line, so the
/// read runs from its start to the end of the file, paged.
pub(super) const READ_TO_END_LINE: usize = 1_000_000;

pub(super) const READ_HANDOFF_CONTEXT: u32 = 6;

/// `(start, end)` windows of `context` lines around each line, merged where
/// they overlap or touch, in line order.
pub(super) fn hit_windows(lines: impl IntoIterator<Item = u32>, context: u32) -> Vec<(u32, u32)> {
    crate::tools::line_spans::merge_spans(lines.into_iter().map(|line| {
        (
            line.saturating_sub(context).max(1),
            line.saturating_add(context),
        )
    }))
}

/// localFetch read of one file's hits in context (the file declaring the
/// searched symbol, else the top file): ±6-line `ranges`
/// around exactly the shown hits, or (when they need more windows than one
/// read holds) the search text as `matchString`. Paths join the caller's
/// own `path`, so the read resolves wherever the search did.
pub(super) fn read_handoff(
    query: &LocalSearchQuery,
    root: &std::path::Path,
    top: &SearchFile,
) -> Option<Value> {
    let hits = top.matches.as_ref()?;
    if hits.is_empty() || hits.len() > READ_HANDOFF_MAX_HITS {
        return None;
    }
    let path = joined_path(query, root, top);
    let windows = hit_windows(hits.iter().flat_map(row_lines), READ_HANDOFF_CONTEXT);
    if windows.len() <= MAX_READ_RANGES {
        let ranges = windows
            .iter()
            .map(|(start, end)| format!("{start}-{end}"))
            .collect::<Vec<_>>();
        return Some(
            Continuation::new(ToolId::LocalFetch, json!({"path": path, "ranges": ranges}))
                .why("Read the top file's hits in context.")
                .build(),
        );
    }
    let mut read = json!({
        "path": path,
        "matchString": query.match_string.as_str(),
        "contextLines": READ_HANDOFF_CONTEXT,
    });
    if query.regex_mode() != LocalSearchQueryRegex::Literal
        && regex::escape(&query.match_string) != query.match_string.as_str()
    {
        read["regex"] = json!("rust");
    }
    if query.case_mode != LocalSearchQueryCaseMode::Insensitive {
        read["caseMode"] = json!(query.case_mode);
    }
    Some(
        Continuation::new(ToolId::LocalFetch, read)
            .why("Read the top file's hits in context.")
            .build(),
    )
}

/// A query that explicitly targets a single file which the engine then skips
/// (e.g. over the per-file byte ceiling, surfaced as `capReason:"maxFileSize"`
/// with `filesScanned:0`) is a silent false negative without an explanation:
/// nothing was searched, so "no matches" would be misleading.
pub(super) fn skipped_target_hint(
    single_file: bool,
    files_searched: u32,
    cap_reason: Option<&str>,
) -> Option<String> {
    let reason = cap_reason?;
    if single_file && reason.contains("binaryQuit") {
        return Some(
            "The target file is binary (NUL byte found); it was not searched past that point, and no text tool reads past it."
                .into(),
        );
    }
    (single_file && files_searched == 0).then(|| {
        format!(
            "Nothing searched: the target file was skipped ({reason}). Read it with localFetch."
        )
    })
}

pub(super) fn unreadable_hint(count: u32) -> String {
    format!(
        "{count} path(s) unreadable (check permissions, stats.firstError); absence is unproven, so narrow path to readable dirs."
    )
}

pub(super) fn empty_hint(query: &LocalSearchQuery) -> String {
    let mut tips = Vec::new();
    // Published fields only: smart case (the default) matches any case for
    // an all-lowercase term; `caseMode` is named only to a caller who set it.
    match query.case_mode {
        LocalSearchQueryCaseMode::Sensitive => tips.push("caseMode:\"insensitive\""),
        LocalSearchQueryCaseMode::Smart if query.match_string.chars().any(char::is_uppercase) => {
            tips.push("an all-lowercase matchString")
        }
        _ => {}
    }
    tips.push("a shorter term");
    if query.regex_mode() == LocalSearchQueryRegex::Literal {
        tips.push("regex:\"rust\"");
    } else {
        tips.push("regex:\"literal\" if matchString has metacharacters");
    }
    format!("No matches. Try {}.", tips.join(", "))
}

/// Wall-clock bound of the ignored/hidden re-walk behind an empty result.
pub(super) const IGNORED_PROBE_MS: u64 = 400;

/// Files an empty search's re-run found where the defaults do not look.
pub(super) struct IgnoredMatches {
    pub(super) count: usize,
    /// The re-run stopped early (deadline or cap): there may be more.
    pub(super) cut: bool,
    /// Why the first matches were skipped, distinct: `gen/.gitignore:2 '*'`
    /// or `hidden .notes/`. A default-excluded directory is named by the
    /// walk's pruned names instead.
    pub(super) reasons: Vec<String>,
}

/// Probe matches whose skip reason the hint names.
const EXPLAINED_MATCHES: usize = 8;
/// Distinct skip reasons one hint names.
const NAMED_REASONS: usize = 3;

/// Why the default walk skipped `file` under `root`: the deciding ignore
/// rule (deepest ignore file first; a `!` re-include there ends the search),
/// else its first hidden component. `None` for a default-excluded directory.
pub(super) fn skip_reason(root: &std::path::Path, file: &std::path::Path) -> Option<String> {
    use ignore::Match;
    use ignore::gitignore::GitignoreBuilder;
    let shown = |path: &std::path::Path| {
        path.strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    };
    let in_git = file.ancestors().any(|dir| dir.join(".git").exists());
    for dir in file.ancestors().skip(1) {
        let mut candidates = vec![dir.join(".ignore")];
        if in_git {
            candidates.push(dir.join(".gitignore"));
            candidates.push(dir.join(".git/info/exclude"));
        }
        for source in candidates.into_iter().filter(|path| path.is_file()) {
            let mut builder = GitignoreBuilder::new(dir);
            if builder.add(&source).is_some() {
                continue;
            }
            let Ok(rules) = builder.build() else {
                continue;
            };
            match rules.matched_path_or_any_parents(file, false) {
                Match::Ignore(glob) => {
                    let pattern = glob.original();
                    let line = std::fs::read_to_string(&source)
                        .ok()
                        .and_then(|text| {
                            text.lines()
                                .enumerate()
                                .filter(|(_, line)| line.trim_end() == pattern)
                                .last()
                                .map(|(index, _)| index + 1)
                        })
                        .map(|line| format!(":{line}"))
                        .unwrap_or_default();
                    return Some(format!("{}{line} '{pattern}'", shown(&source)));
                }
                Match::Whitelist(_) => return hidden_component(root, file),
                Match::None => {}
            }
        }
        if dir.join(".git").exists() {
            break;
        }
    }
    hidden_component(root, file)
}

fn hidden_component(root: &std::path::Path, file: &std::path::Path) -> Option<String> {
    let relative = file.strip_prefix(root).ok()?;
    let mut prefix = std::path::PathBuf::new();
    let count = relative.components().count();
    for (index, part) in relative.components().enumerate() {
        prefix.push(part);
        if part.as_os_str().to_string_lossy().starts_with('.') {
            let slash = if index + 1 < count { "/" } else { "" };
            return Some(format!("hidden {}{slash}", prefix.to_string_lossy()));
        }
    }
    None
}

/// Re-run an empty search with `noIgnore`, `hidden` and no default prune
/// (files only, within [`IGNORED_PROBE_MS`]); `None` when it could not run.
pub(super) fn ignored_probe(
    mut options: TextSearchOptions,
    paths: &PathPolicy,
    cancel: &impl CancellationCheck,
) -> Option<IgnoredMatches> {
    options.no_ignore = Some(true);
    options.hidden = Some(true);
    options.exclude_dir = None;
    options.files_only = Some(true);
    options.files_without_match = Some(false);
    options.count_lines_per_file = Some(false);
    options.count_matches_per_file = Some(false);
    options.only_matching = Some(false);
    options.unique = Some(false);
    options.count_unique = Some(false);
    options.context_lines = Some(0);
    options.sort = Some("path".into());
    options.digest_max_bytes = None;
    let root = std::path::PathBuf::from(&options.path);
    let started = std::time::Instant::now();
    let deadline = std::time::Duration::from_millis(IGNORED_PROBE_MS);
    let found = search_text_cancellable(options, Arc::new(paths.clone()), &|| {
        cancel.check().is_err() || started.elapsed() > deadline
    })
    .ok()?;
    let mut reasons = Vec::new();
    for file in found.files.iter().take(EXPLAINED_MATCHES) {
        let reason = skip_reason(&root, &root.join(&file.path));
        if let Some(reason) = reason.filter(|reason| !reasons.contains(reason)) {
            reasons.push(reason);
        }
    }
    reasons.truncate(NAMED_REASONS);
    Some(IgnoredMatches {
        reasons,
        count: found.files.len(),
        cut: found
            .stats
            .cap_reason
            .as_deref()
            .is_some_and(|reason| !reason.is_empty()),
    })
}

/// `binarySkipped` warning: how many files were skipped as binary from
/// their leading bytes, grouped by extension (`.woff2 54, .png 2`).
pub(super) fn leading_binary_warning(
    groups: &[octocode_engine::types::BinaryExtensionCount],
) -> String {
    let total: u64 = groups.iter().map(|group| u64::from(group.count)).sum();
    let by_extension = binary_counts(groups);
    // Self-contained: the listing lead may be cut by the lead cap.
    let (noun, them) = if total == 1 {
        ("binary file", "it")
    } else {
        ("binary files", "them")
    };
    format!(
        "binarySkipped: {total} {noun} not searched ({by_extension}); structureSearch operation:\"files\" with these extensions lists {them}."
    )
}

/// Most chars of exact names a binary listing's `nameRegex` spells out.
pub(super) const MAX_LISTED_NAME_CHARS: usize = 2_000;

/// Most extensions a structureSearch `extensions` filter takes.
pub(super) const MAX_LISTED_EXTENSIONS: usize = 100;

/// A structureSearch `files` query over the searched scope that lists every
/// skipped binary: by `extensions`, or by a basename regex when one has no
/// extension (or there are more extensions than the filter takes). It may
/// also list same-extension text files; it never misses a skipped one.
pub(super) fn leading_binary_listing(
    query: &LocalSearchQuery,
    groups: &[octocode_engine::types::BinaryExtensionCount],
) -> Value {
    let mut listing = json!({
        "operation": "files",
        "path": query.path.as_str(),
        "entryType": "f",
    });
    let by_extension = groups.len() <= MAX_LISTED_EXTENSIONS
        && groups.iter().all(|group| !group.extension.is_empty());
    if by_extension {
        listing["extensions"] = json!(
            groups
                .iter()
                .map(|group| group.extension.as_str())
                .collect::<Vec<_>>()
        );
    } else {
        // Extensionless binaries by exact name: a no-extension pattern
        // would also list every extensionless text file.
        let names = groups
            .iter()
            .filter(|group| group.extension.is_empty())
            .flat_map(|group| group.names.iter().map(|name| regex::escape(name)))
            .collect::<Vec<_>>();
        let extensions = groups
            .iter()
            .filter(|group| !group.extension.is_empty())
            .map(|group| regex::escape(&group.extension))
            .collect::<Vec<_>>();
        let mut alternatives = Vec::new();
        if !names.is_empty() {
            let exact = format!("^(?:{})$", names.join("|"));
            // Too many names for one call: every extensionless name instead,
            // which may also list text files but never misses a binary.
            alternatives.push(if exact.len() > MAX_LISTED_NAME_CHARS {
                r"^\.?[^.]*$".to_owned()
            } else {
                exact
            });
        }
        if !extensions.is_empty() {
            alternatives.push(format!(r"\.(?i:{})$", extensions.join("|")));
        }
        listing["nameRegex"] = json!(alternatives.join("|"));
    }
    if let Some(depth) = query.max_depth {
        listing["maxDepth"] = json!(depth);
    }
    if !query.exclude.is_empty() {
        listing["exclude"] = json!(query.exclude);
    }
    if !query.default_excludes.defaults() {
        listing["defaultExcludes"] = json!(false);
    }
    if query.no_ignore == Some(true) {
        listing["noIgnore"] = json!(true);
    }
    Continuation::new(ToolId::StructureSearch, listing)
        .why("List the files skipped as binary.")
        .confidence("high")
        .build()
}

/// Binary-cut files a warning names one by one; more are a count by
/// extension, listed whole by the `binarySkipped` lead.
pub(super) const MAX_NAMED_BINARY_FILES: usize = 10;

/// Paths grouped by lowercased extension (`""` for none, naming those
/// files), most files first, as the engine groups skipped binaries.
pub(super) fn binary_groups(paths: &[String]) -> Vec<octocode_engine::types::BinaryExtensionCount> {
    let mut groups = std::collections::BTreeMap::<String, (u32, Vec<String>)>::new();
    for path in paths {
        let path = std::path::Path::new(path);
        let extension = octocode_engine::text::extension_of(&path.to_string_lossy(), true, "");
        let group = groups.entry(extension.clone()).or_default();
        group.0 = group.0.saturating_add(1);
        if extension.is_empty()
            && let Some(name) = path.file_name()
        {
            group.1.push(name.to_string_lossy().into_owned());
        }
    }
    let mut groups = groups
        .into_iter()
        .map(|(extension, (count, mut names))| {
            names.sort();
            names.dedup();
            octocode_engine::types::BinaryExtensionCount {
                extension,
                count,
                names,
            }
        })
        .collect::<Vec<_>>();
    groups.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.extension.cmp(&b.extension))
    });
    groups
}

/// Two extension groupings as one: counts summed, names joined.
pub(super) fn merge_binary_groups(
    first: &[octocode_engine::types::BinaryExtensionCount],
    second: &[octocode_engine::types::BinaryExtensionCount],
) -> Vec<octocode_engine::types::BinaryExtensionCount> {
    let mut merged: Vec<octocode_engine::types::BinaryExtensionCount> = first.to_vec();
    for group in second {
        match merged
            .iter_mut()
            .find(|known| known.extension == group.extension)
        {
            Some(known) => {
                known.count = known.count.saturating_add(group.count);
                known.names.extend(group.names.iter().cloned());
                known.names.sort();
                known.names.dedup();
            }
            None => merged.push(group.clone()),
        }
    }
    merged
}

/// `.woff2 54, .png 2, no extension 1`.
pub(super) fn binary_counts(groups: &[octocode_engine::types::BinaryExtensionCount]) -> String {
    groups
        .iter()
        .map(|group| {
            if group.extension.is_empty() {
                format!("no extension {}", group.count)
            } else {
                format!(".{} {}", group.extension, group.count)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The binary-cut files a warning names: every root-relative path, files
/// sharing a directory written once under it (`dir/{a.txt,b.txt}`), then
/// the count of any the engine could not name (`and 3 more`). Past
/// [`MAX_NAMED_BINARY_FILES`] they are a count by extension, and the
/// `binarySkipped` listing names each one.
pub(super) fn binary_file_list(paths: &[String], total: u32, root: &std::path::Path) -> String {
    if paths.is_empty() {
        return "a file with a NUL byte was".into();
    }
    if paths.len() > MAX_NAMED_BINARY_FILES {
        let total = (total as usize).max(paths.len());
        return format!(
            "{total} files ({}; structureSearch operation:\"files\" with these extensions lists them) were",
            binary_counts(&binary_groups(paths))
        );
    }
    let mut by_dir = std::collections::BTreeMap::<String, Vec<String>>::new();
    for path in paths {
        let path = std::path::Path::new(path);
        let relative = path
            .strip_prefix(root)
            .ok()
            .filter(|relative| !relative.as_os_str().is_empty())
            .unwrap_or(path);
        let dir = relative
            .parent()
            .map(|dir| dir.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = relative.file_name().map_or_else(
            || relative.to_string_lossy().into_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        by_dir.entry(dir).or_default().push(name);
    }
    let mut names = by_dir
        .into_iter()
        .flat_map(|(dir, names)| {
            let prefix = if dir.is_empty() {
                String::new()
            } else {
                format!("{dir}/")
            };
            if names.len() == 1 || dir.is_empty() {
                names
                    .into_iter()
                    .map(|name| format!("{prefix}{name}"))
                    .collect::<Vec<_>>()
            } else {
                vec![format!("{prefix}{{{}}}", names.join(","))]
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let unnamed = (total as usize).saturating_sub(paths.len());
    if unnamed > 0 {
        names.push_str(&format!(" and {unnamed} more"));
    }
    let verb = if paths.len() + unnamed == 1 {
        "was"
    } else {
        "were"
    };
    format!("{names} {verb}")
}

#[cfg(test)]
mod enclosing_cap_tests {
    /// A file over the enclosing cap is not outlined (its hits keep no
    /// `enclosing`); one at the cap still is.
    #[test]
    fn files_over_the_enclosing_cap_are_not_outlined() {
        let security = crate::security::ContentSecurity;
        let path = std::path::Path::new("big.rs");
        let line = "fn f() {}\n";
        let at_cap = line.repeat(super::ENCLOSING_MAX_BYTES / line.len());
        assert!(super::outline_of(at_cap.as_bytes(), path, &security).is_some());
        let over = format!("{at_cap}{line}{line}");
        assert!(over.len() > super::ENCLOSING_MAX_BYTES);
        assert!(super::outline_of(over.as_bytes(), path, &security).is_none());
    }
}

#[cfg(test)]
mod empty_hint_tests {
    use super::{LocalSearchQuery, empty_hint};

    fn query(row: serde_json::Value) -> LocalSearchQuery {
        serde_json::from_value(row).expect("localSearch row")
    }

    /// An empty-result tip names only published fields: smart case already
    /// matches any case for an all-lowercase term, so a mixed-case term gets
    /// that tip; only a caller who set `caseMode` is told about it.
    #[test]
    fn empty_tips_name_published_fields() {
        let mixed = empty_hint(&query(
            serde_json::json!({"path":".","matchString":"FooBar"}),
        ));
        assert!(!mixed.contains("caseMode"), "{mixed}");
        assert!(mixed.contains("all-lowercase matchString"), "{mixed}");
        let lower = empty_hint(&query(
            serde_json::json!({"path":".","matchString":"foobar"}),
        ));
        assert!(
            !lower.contains("caseMode") && !lower.contains("lowercase"),
            "{lower}"
        );
        let strict = empty_hint(&query(
            serde_json::json!({"path":".","matchString":"FooBar","caseMode":"sensitive"}),
        ));
        assert!(strict.contains("caseMode:\"insensitive\""), "{strict}");
    }
}
