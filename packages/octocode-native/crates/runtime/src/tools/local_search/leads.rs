//! The follow-up calls a search page offers (read, references, re-runs,
//! listings), ranked; what the page could not cover is in `coverage`, the
//! empty page's probes in `probes`.

use super::local_search_query;
use super::layout::*;
use super::types::*;
use super::{binaries::*, handoff::*, probes::*};
use crate::execution::cancel::CancellationCheck;
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use crate::policy::prune::DefaultsFlag;
use crate::response::result::{CURSOR_FIELDS, Continuation, ToolError, query_row};
use crate::tools::empty_walk::Cause;
use crate::tools::leads::skipped::under;
use octocode_contract_types::{SkippedPathCause, SkippedPaths};
use octocode_engine::types::{PatternSyntax, SearchOutput, TextSearchOptions};
use serde_json::{Value, json};

mod alternation;
mod reads;
mod skipped;
#[cfg(test)]
mod tests;

use alternation::{regex_engine, regex_lead};

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

    pub(super) fn first_page(&self) -> bool {
        local_search_query::page(self.query).max(1) == 1
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

/// Whether the row lists every skipped path (`extras:["details"]`) instead
/// of one entry per cause and rule.
pub(super) fn lists_every_skipped_path(query: &LocalSearchQuery) -> bool {
    crate::tools::leads::skipped::every_path(&query.extras)
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
