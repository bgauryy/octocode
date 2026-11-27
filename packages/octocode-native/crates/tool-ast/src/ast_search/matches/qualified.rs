//! The qualified-path note: a pattern matches a path as written, so
//! `Mutex::new($X)` leaves out `std::sync::Mutex::new(x)`, a call on a bare
//! name leaves out the calls that qualify it (`redact_lines($$$A)` against
//! `octocode_runtime_base::security::scan::redact_lines(x)`), and a name inside a larger
//! pattern leaves out its qualified spelling (`impl ToolOutput for $T {}`
//! against `impl crate::x::ToolOutput for B {}`). A page counts those
//! longer spellings in the files its scan evaluated (a paged walk reads its
//! scope window by window: each window discloses what its own files hold) and leads to the rule
//! that lists them all. A call on a bare name also leaves out the calls on a
//! receiver (`sanitize_text($$$A)` against `self.sanitize_text(x)`): in a
//! grammar with `::` paths those are a second longer spelling, counted and
//! listed the same way. The same holds for a pattern nested in the rule (in
//! a relation, `all` or `any`): its lead rewrites that pattern in place.
use super::exact_miss::{asked_view, lead_row, rule_scan, with_rule};
use super::query::MatchQuery;
use super::rows::push_diagnostic;
use super::scan::parser_extension;
use super::scope::scope_extension;
use super::types::{Scan, Scope};
use crate::ast_search::RowPaths;
use octocode_contract_types::AstSearchQueryMatch0;
use octocode_engine::structural::{
    FileMatches, QualifiedPattern, Rebuilt, StructuralDetailedMatch, pattern_qualifiers,
};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::policy::path::PathPolicy;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Longer spellings of a rule's patterns the scan's files hold.
pub(super) struct QualifiedMiss {
    /// The names the longer spellings qualify, in pattern order.
    names: Vec<String>,
    count: usize,
    /// Whether a lead rewrites more than one pattern: each was probed with
    /// the others as written, so the lead also returns the matches that
    /// qualify two of them at once and `count` is a floor.
    floor: bool,
    files: usize,
    /// The first longer spelling, as `path:line` (`line N` in a file
    /// scope, where the path is the request's own); `None` when no probed
    /// file holds one.
    first: Option<String>,
    /// The files whose longer spellings are unknown: their count and the
    /// first one's path.
    unprobed: Option<(usize, String)>,
    /// The rule listing each pattern and every longer spelling found: one
    /// query, or one per grammar when the scan read several (a pattern
    /// compiles in the grammar it is written in).
    queries: Vec<AstSearchQueryMatch0>,
    /// The summary `view` the call asked for, which the lead keeps
    /// ([`asked_view`]).
    view: Option<Value>,
    /// The page is a later window of a paged walk: the count is of the
    /// files that window read, as each window discloses what its own files
    /// hold.
    later: bool,
}

/// A pattern of the rule: the JSON pointer of the rule object that holds it,
/// and its text.
type Site = (String, String);

/// The patterns of `rule` a longer spelling can stand in for. A pattern
/// under `not` that misses a spelling excludes less, so it cannot empty or
/// shorten a result, and one in a `stopBy` says where a relation stops, not
/// what matches; an object that also holds an `any` has no place for the
/// `any` of spellings.
fn sites(q: MatchQuery<'_>, rule: &Value) -> Vec<Site> {
    q.patterns()
        .into_iter()
        .filter(|found| !found.negated() && !found.at.split('/').any(|key| key == "stopBy"))
        .filter_map(|found| {
            let text = found.pattern.as_str()?.to_owned();
            let holder = rule.pointer(&found.at)?;
            holder.get("any").is_none().then_some((found.at, text))
        })
        .collect()
}

/// [`QualifiedMiss`] of a first page whose rule has a positive pattern
/// ([`sites`]) with a literal name a path can qualify
/// ([`pattern_qualifiers`]): its skeleton matched over every file the scan
/// evaluated, and each spelling that qualifies some of the names counted by
/// the matches it adds to the rule's. A file the literal prefilter skipped
/// holds no such match, which spells every literal of the pattern. A window
/// its page filled probes the files it read, and its count is a floor.
/// `None` when no pattern has such a name, the scan or a probe did not
/// finish, or no file holds one.
pub(super) fn qualified_miss(
    q: MatchQuery<'_>,
    scope: &Scope,
    scan: &Scan,
    (paths, row_paths): (&PathPolicy, RowPaths<'_>),
    cancel: &dyn CancellationCheck,
) -> Option<QualifiedMiss> {
    if q.page() > 1 || (scan.truncated && !scan.filled) {
        return None;
    }
    let rule = q.rule()?;
    let sites = sites(q, &rule);
    if sites.is_empty() {
        return None;
    }
    let Probe {
        grammars,
        hits,
        count,
        unprobed,
    } = if scope.is_file {
        file_probe(q, scope, &rule, &sites, paths, cancel)?
    } else {
        directory_probe(scope, scan, &rule, &sites, cancel)?
    };
    if hits.is_empty() && unprobed.is_empty() {
        return None;
    }
    let first = hits.first_key_value().map(|(path, line)| {
        if scope.is_file {
            format!("line {line}")
        } else {
            format!("{}:{line}", row_paths.delivered(path))
        }
    });
    let unprobed = unprobed
        .first()
        .map(|path| (unprobed.len(), row_paths.delivered(path)));
    let mut names = Vec::new();
    for (_, qualified, found) in grammars.values().flatten() {
        let spelled = qualified.names();
        for index in found.iter().flatten() {
            if let Some(name) = spelled.get(*index)
                && !names.iter().any(|known| known == name)
            {
                names.push((*name).to_owned());
            }
        }
    }
    let floor = scan.filled
        || grammars.values().any(|spelled| {
            spelled
                .iter()
                .any(|(a, ..)| spelled.iter().any(|(b, ..)| a != b && conjoined(a, b)))
        });
    let pinned = grammars.len() > 1;
    let queries = grammars
        .iter()
        .map(|(grammar, spelled)| {
            let lead = lead_rule(&rule, spelled)?;
            let mut query = serde_json::to_value(with_rule(q, scope, lead)?).ok()?;
            if pinned {
                query["parser"] = json!(grammar);
            }
            serde_json::from_value(query).ok()
        })
        .collect::<Option<Vec<_>>>()?;
    Some(QualifiedMiss {
        names,
        count,
        floor,
        files: hits.len(),
        first,
        unprobed,
        queries,
        view: asked_view(q),
        later: q.scan_offset() > 0,
    })
}

/// Whether one match needs both the patterns at the pointers `a` and `b`:
/// not when the rule picks between them in an `any`, where each alternative
/// adds its own matches and the count stays exact.
fn conjoined(a: &str, b: &str) -> bool {
    let (a, b) = (
        a.split('/').collect::<Vec<_>>(),
        b.split('/').collect::<Vec<_>>(),
    );
    let common = a.iter().zip(&b).take_while(|(a, b)| a == b).count();
    let alternatives = common > 0 && a[common - 1] == "any" && common < a.len().min(b.len());
    !alternatives
}

/// `rule` with each pattern `spelled` found longer spellings of replaced, in
/// place, by the `any` of the pattern and those spellings, shortest first.
/// Each pattern's spellings take captures the rest of the lead leaves free,
/// so two rewritten patterns never tie their qualifiers together.
fn lead_rule(rule: &Value, spelled: &[Spelled]) -> Option<Value> {
    let mut lead = rule.clone();
    let mut rewritten = BTreeSet::new();
    for (at, ..) in spelled {
        if !rewritten.insert(at) {
            continue;
        }
        let beside = lead.to_string();
        let longer = spelled
            .iter()
            .filter(|(site, ..)| site == at)
            .flat_map(|(_, qualified, found)| {
                let mut found = found.iter().collect::<Vec<_>>();
                found.sort_by(|a, b| (a.len(), a).cmp(&(b.len(), b)));
                found
                    .into_iter()
                    .map(|names| json!(qualified.spelling(names, &beside)))
            })
            .collect::<Vec<_>>();
        let holder = lead.pointer_mut(at)?.as_object_mut()?;
        let written = holder.remove("pattern")?;
        let spellings = std::iter::once(written)
            .chain(longer)
            .map(|spelled| json!({"pattern": spelled}))
            .collect();
        holder.insert("any".into(), Value::Array(spellings));
    }
    Some(lead)
}

/// The parser ID of the grammar that reads `extension`.
fn extension_grammar(extension: &str) -> String {
    octocode_engine::signatures::languages::find_entry(extension)
        .map(octocode_engine::signatures::languages::label)
        .unwrap_or_default()
}

/// One longer spelling of a pattern: the pattern's place in the rule
/// ([`Site`]) and its spellings in one grammar.
type Qualifier = (String, QualifiedPattern);

/// Each site's longer spellings in the grammar `extension` names.
fn site_qualifiers(sites: &[Site], extension: &str) -> Vec<Qualifier> {
    sites
        .iter()
        .flat_map(|(at, pattern)| {
            pattern_qualifiers(pattern, extension)
                .into_iter()
                .map(move |qualified| (at.clone(), qualified))
        })
        .collect()
}

/// One [`Qualifier`], and each set of names a match qualified with it.
type Spelled = (String, QualifiedPattern, BTreeSet<Vec<usize>>);

/// Per qualifier, each set of names whose spelling adds matches to the
/// rule's, with the matches (by span) it adds.
type FileSpellings = Vec<Vec<(Vec<usize>, BTreeSet<Span>)>>;

/// What the longer spellings matched.
struct Probe {
    /// Per grammar (parser ID) that read a longer spelling: each of the
    /// patterns' longer spellings there that matched, and each set of names
    /// a match qualified with it.
    grammars: BTreeMap<String, Vec<Spelled>>,
    /// The first line per file (by path).
    hits: BTreeMap<String, u64>,
    /// The match count.
    count: usize,
    /// The files whose probe did not finish (changed since the scan, or cut
    /// by a limit), by path: their longer spellings are unknown.
    unprobed: Vec<String>,
}

impl Probe {
    fn new() -> Self {
        Self {
            grammars: BTreeMap::new(),
            unprobed: Vec::new(),
            hits: BTreeMap::new(),
            count: 0,
        }
    }

    /// One file's longer spellings (`found`, per one of `qualifiers`), read
    /// in the grammar `extension` names. A match two spellings add counts
    /// once.
    fn add(&mut self, path: &str, extension: &str, qualifiers: &[Qualifier], found: FileSpellings) {
        let added = found
            .iter()
            .flatten()
            .flat_map(|(_, spans)| spans)
            .collect::<BTreeSet<_>>();
        let Some(first) = added.first() else {
            return;
        };
        self.hits.insert(path.to_owned(), u64::from(first.0));
        self.count += added.len();
        let spelled = self
            .grammars
            .entry(extension_grammar(extension))
            .or_default();
        for ((at, qualified), found) in qualifiers.iter().zip(found) {
            if found.is_empty() {
                continue;
            }
            let index = spelled
                .iter()
                .position(|(site, known, _)| site == at && known == qualified)
                .unwrap_or_else(|| {
                    spelled.push((at.clone(), qualified.clone(), BTreeSet::new()));
                    spelled.len() - 1
                });
            spelled[index]
                .2
                .extend(found.into_iter().map(|(names, _)| names));
        }
    }
}

/// Each set of names some match of `qualified`'s skeleton (`matches`)
/// qualifies, with the matches of `rule` (by span) when the pattern at `at`
/// spells exactly those names qualified; a set whose spelling matches
/// nothing is left out.
///
/// The captures only name candidates (a capture can hold `&m::Foo`, which
/// ends like a path but is another type), so a set counts only by what its
/// spelling matches: the note counts exactly what its lead returns. `spans`
/// runs a rule over the same file and gives its matches' spans; `None` when
/// a run did not finish.
fn longer_spellings(
    (rule, at): (&Value, &str),
    qualified: &QualifiedPattern,
    matches: &[StructuralDetailedMatch],
    spans: &mut impl FnMut(Value) -> Option<BTreeSet<Span>>,
) -> Option<Vec<(Vec<usize>, BTreeSet<Span>)>> {
    let candidates = matches
        .iter()
        .filter_map(|found| qualified.qualified_names(&found.metavars))
        .collect::<BTreeSet<_>>();
    let beside = rule.to_string();
    let mut found = Vec::new();
    for names in candidates {
        let spelled = spans(respelled(rule, at, &qualified.spelling(&names, &beside))?)?;
        if !spelled.is_empty() {
            found.push((names, spelled));
        }
    }
    Some(found)
}

/// One file's [`FileSpellings`] for `qualifiers`. `text`, the file's bytes
/// when read, rules out a qualifier none of whose names follows a
/// separator; `matches` runs a rule over the file, `None` when that run did
/// not finish.
fn file_spellings(
    rule: &Value,
    qualifiers: &[Qualifier],
    text: Option<&str>,
    matches: impl Fn(&Value) -> Option<Vec<StructuralDetailedMatch>>,
) -> Option<FileSpellings> {
    let mut spans = |rule: Value| Some(matches(&rule)?.iter().map(span).collect());
    let mut found = Vec::new();
    for (at, qualified) in qualifiers {
        if text.is_some_and(|text| !qualified.may_qualify(text)) {
            found.push(Vec::new());
            continue;
        }
        let skeleton = matches(&site_rule(rule, at, qualified.skeleton())?)?;
        found.push(longer_spellings(
            (rule, at),
            qualified,
            &skeleton,
            &mut spans,
        )?);
    }
    if found.iter().all(Vec::is_empty) {
        return Some(found);
    }
    // What the rule as written matches is no longer spelling: a pattern in
    // a relation or an `any` can be met by another node or alternative.
    let written = spans(rule.clone())?;
    for spellings in &mut found {
        for (_, spelled) in spellings.iter_mut() {
            spelled.retain(|at| !written.contains(at));
        }
        spellings.retain(|(_, spelled)| !spelled.is_empty());
    }
    Some(found)
}

/// Where a match sits: start line and column, end line and column.
type Span = (u32, u32, u32, u32);

fn span(found: &StructuralDetailedMatch) -> Span {
    (
        found.start_line,
        found.start_col,
        found.end_line,
        found.end_col,
    )
}

/// The rule object at `at` as a rule of its own, with `pattern` in place of
/// its own: the candidates of a pattern nested anywhere. A relation's
/// `stopBy` and `field` belong to the relation, not to the object.
fn site_rule(rule: &Value, at: &str, pattern: &str) -> Option<Value> {
    let mut site = rule.pointer(at)?.as_object()?.clone();
    site.remove("stopBy");
    site.remove("field");
    site.insert("pattern".into(), json!(pattern));
    Some(Value::Object(site))
}

/// `rule` with `pattern` in place of the one at `at`.
fn respelled(rule: &Value, at: &str, pattern: &str) -> Option<Value> {
    let mut respelled = rule.clone();
    *respelled.pointer_mut(at)?.get_mut("pattern")? = json!(pattern);
    Some(respelled)
}

/// The text of the file at `source` as its bytes read, for the separator
/// prefilter ([`file_spellings`]). Reading the bytes is far cheaper than
/// scanning them again: a file that spells no name after a separator holds
/// no longer spelling. `None` when it no longer reads; it is then matched,
/// which says so.
fn source_text(source: &std::path::Path) -> Option<String> {
    let limit = octocode_runtime_base::security::SourceLimits::REAL.whole_source as u64;
    octocode_engine::text::read_source(source, limit)
        .ok()
        .map(|read| String::from_utf8_lossy(&read.bytes).into_owned())
}

/// A file scope: the file scanned once more with each skeleton and
/// spelling its text may hold.
fn file_probe(
    q: MatchQuery<'_>,
    scope: &Scope,
    rule: &Value,
    sites: &[Site],
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Option<Probe> {
    let extension = scope_extension(scope)?;
    let qualifiers = site_qualifiers(sites, &extension);
    if qualifiers.is_empty() {
        return None;
    }
    let mut probe = Probe::new();
    let path = scope.canonical.to_string_lossy();
    let text = match &scope.inline {
        Some(code) => Some(code.clone()),
        None => source_text(&scope.canonical),
    };
    let matches = |rule: &Value| {
        let scan = rule_scan(
            &with_rule(q, scope, rule.clone())?,
            paths,
            cancel,
            super::scan::Keeps::Page(None),
        )?;
        let file = scan.files.into_iter().find(|file| file.status == "ok")?;
        Some(match file.matches {
            FileMatches::Held(matches) => matches,
            _ => Vec::new(),
        })
    };
    match file_spellings(rule, &qualifiers, text.as_deref(), matches) {
        Some(found) => probe.add(&path, &extension, &qualifiers, found),
        None => probe.unprobed.push(path.to_string()),
    }
    Some(probe)
}

/// A directory scope: each file the scan parsed that spells a name after a
/// separator, matched again from the bytes it scanned under the grammar it
/// was read with and that grammar's skeletons and spellings of the
/// patterns. A file that changed or did not finish matching voids the
/// count.
fn directory_probe(
    scope: &Scope,
    scan: &Scan,
    rule: &Value,
    sites: &[Site],
    cancel: &dyn CancellationCheck,
) -> Option<Probe> {
    let mut probe = Probe::new();
    let mut grammars = BTreeMap::new();
    let mut probed = Vec::new();
    for file in scan.files.iter().filter(|file| file.status == "ok") {
        let Some((source, digest)) = &file.source else {
            probe.unprobed.push(file.path.clone());
            continue;
        };
        let extension = parser_extension(scope, source);
        let qualifiers = grammars
            .entry(extension.clone())
            .or_insert_with(|| site_qualifiers(sites, &extension));
        if !qualifiers.is_empty() {
            probed.push((file, source, digest, extension));
        }
    }
    let workers = octocode_tools_shared::workers::workers(probed.len(), None);
    let stop = || cancel.check().is_err();
    let found = octocode_tools_shared::workers::each_parallel(
        &mut probed,
        workers,
        |(_, source, digest, extension)| {
            let qualifiers = grammars.get(extension.as_str())?;
            let probe = || {
                let limit = octocode_runtime_base::security::SourceLimits::REAL.whole_source as u64;
                let text = source_text(source);
                let judge = super::super::source::judge();
                let matches = |rule: &Value| match octocode_engine::structural::file_matches(
                    source,
                    extension,
                    None,
                    Some(rule),
                    digest,
                    (limit, Some(&judge)),
                    &stop,
                ) {
                    Ok(Rebuilt::Matches(matches)) => Some(matches),
                    _ => None,
                };
                file_spellings(rule, qualifiers, text.as_deref(), matches)
            };
            kept_spellings((source, digest, extension), rule, &stop, probe)
        },
    );
    for ((file, _, _, extension), outcome) in probed.iter().zip(found) {
        match (grammars.get(extension.as_str()), outcome) {
            (Some(qualifiers), Some(found)) => probe.add(&file.path, extension, qualifiers, found),
            _ => probe.unprobed.push(file.path.clone()),
        }
    }
    Some(probe)
}

/// Each probed file's longer spellings, kept by the digest of the bytes the
/// scan matched, its path and parser, and the rule (its qualifiers follow
/// from the rule and the parser): a repeat of the query, or its first page
/// again, probes only files whose bytes moved. A probe that did not finish
/// is not kept.
static SPELLINGS: octocode_tools_shared::page_memo::PageMemo<Option<FileSpellings>> =
    octocode_tools_shared::page_memo::PageMemo::new();

/// [`file_spellings`] of the file at `source` (`digest`, read as
/// `extension`) under `rule`: the kept answer, else `probe`'s.
fn kept_spellings(
    (source, digest, extension): (&std::path::Path, &str, &str),
    rule: &Value,
    stop: &dyn Fn() -> bool,
    probe: impl FnOnce() -> Option<FileSpellings>,
) -> Option<FileSpellings> {
    let path = source.to_string_lossy();
    let rule = rule.to_string();
    let key = format!(
        "{digest}:{}:{extension}:{}:{path}:{rule}",
        extension.len(),
        path.len()
    );
    SPELLINGS.get_or_compute(key, "", stop, || {
        let value = probe();
        let keep = value.as_ref().map(|found| {
            128 + found
                .iter()
                .flatten()
                .map(|(names, spans)| 32 + names.len() * 8 + spans.len() * 24)
                .sum::<usize>()
        });
        octocode_tools_shared::page_memo::Computed { value, keep }
    })
}

/// Where a [`QualifiedMiss`] note sends its reader. The empty page's hint
/// says it; the diagnostic beside matches leaves it to its code's reference.
const RECOVERY: &str = "The related astSearch rule with `any` lists every spelling found.";

impl QualifiedMiss {
    /// The note that counts the longer spellings (a hint on an empty page, a
    /// diagnostic beside matches), and the rule query that lists every
    /// spelling.
    pub(super) fn attach(self, out: &mut Value) {
        let Self {
            names,
            count,
            floor,
            files,
            first,
            unprobed,
            queries,
            view,
            later,
        } = self;
        let unprobed = unprobed
            .map(|(files, first)| {
                format!(" Longer spellings in {files} file(s) are unknown: the probe did not finish (first: {first}).")
            })
            .unwrap_or_default();
        let names = names
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(" or ");
        let more = if floor { "at least " } else { "" };
        let window = if later { " this page's scan read" } else { "" };
        let message = match first {
            Some(first) => format!(
                "The pattern matches the path as written: {more}{count} more match(es) in {files} file(s){window} spell a path or receiver before {names} (first: {first}).{unprobed}"
            ),
            None => format!("The pattern matches the path as written.{unprobed}"),
        };
        if out["status"] == "empty" {
            // An empty page reads as "no such call": the one line that says
            // otherwise is a hint, where every surface shows it.
            octocode_runtime_base::response::result::add_hints(
                out,
                [format!("{message} {RECOVERY}")],
            );
        } else {
            push_diagnostic(
                out,
                json!({
                    "code": "structural.pattern.qualified",
                    "severity": "info",
                    "stage": "match",
                    "message": message,
                }),
            );
        }
        let rows = queries
            .iter()
            .filter_map(|query| lead_row(query, view.as_ref()))
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return;
        }
        // A lead an empty page's own explanation already offers stays: this
        // one takes the next numbered `broadenSearch`.
        octocode_runtime_base::response::result::place_numbered(
            out,
            "broadenSearch",
            octocode_runtime_base::response::result::Continuation::related_input(
                ToolId::AstSearch,
                json!({"queries": rows}),
            )
            // The diagnostic says spellings were left out: the row needs the
            // rule that lists them.
            .required()
            .build(),
        );
    }
}

#[cfg(test)]
#[path = "qualified_tests.rs"]
mod tests;
