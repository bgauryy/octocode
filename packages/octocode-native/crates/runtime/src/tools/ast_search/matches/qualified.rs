//! The qualified-path note: a pattern matches a path as written, so
//! `Mutex::new($X)` leaves out `std::sync::Mutex::new(x)`, a call on a bare
//! name leaves out the calls that qualify it (`redact_lines($$$A)` against
//! `crate::security::scan::redact_lines(x)`), and a name inside a larger
//! pattern leaves out its qualified spelling (`impl ToolOutput for $T {}`
//! against `impl crate::x::ToolOutput for B {}`). A first page counts those
//! longer spellings in the files its scan evaluated and leads to the rule
//! that lists them all. A call on a bare name also leaves out the calls on a
//! receiver (`sanitize_text($$$A)` against `self.sanitize_text(x)`): in a
//! grammar with `::` paths those are a second longer spelling, counted and
//! listed the same way.
use super::exact_miss::{rule_scan, with_rule};
use super::query::MatchQuery;
use super::rows::push_diagnostic;
use super::scan::parser_extension;
use super::scope::scope_extension;
use super::types::{Scan, Scope};
use crate::execution::cancel::CancellationCheck;
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use crate::tools::ast_search::RowPaths;
use octocode_contract_types::AstSearchQueryMatch0;
use octocode_engine::structural::{
    QualifiedPattern, Rebuilt, StructuralDetailedMatch, pattern_qualifiers,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Longer spellings of a pattern's names the scan's files hold.
pub(super) struct QualifiedMiss {
    /// The names the longer spellings qualify, in pattern order.
    names: Vec<String>,
    count: usize,
    files: usize,
    /// The first longer spelling, as `path:line`; `None` when no probed
    /// file holds one.
    first: Option<String>,
    /// The files whose longer spellings are unknown: their count and the
    /// first one's path.
    unprobed: Option<(usize, String)>,
    /// The rule listing the pattern and every longer spelling found: one
    /// query, or one per grammar when the scan read several (a pattern
    /// compiles in the grammar it is written in).
    queries: Vec<AstSearchQueryMatch0>,
}

/// [`QualifiedMiss`] of a first page whose `rule.pattern` has a literal name
/// a path can qualify ([`pattern_qualifiers`]): its skeleton matched over
/// every file the scan evaluated, and each match that qualifies some of the
/// names counted. A file the literal prefilter skipped holds no such match,
/// which spells every literal of the pattern. `None` when the pattern has no
/// such name, the scan or a probe did not finish, or no file holds one.
pub(super) fn qualified_miss(
    q: MatchQuery<'_>,
    scope: &Scope,
    scan: &Scan,
    (paths, row_paths): (&PathPolicy, RowPaths<'_>),
    cancel: &dyn CancellationCheck,
) -> Option<QualifiedMiss> {
    if q.page() != 1 || q.scan_offset() != 0 || scan.truncated {
        return None;
    }
    let rule = q.to_value().get("rule")?.as_object()?.clone();
    let pattern = rule.get("pattern")?.as_str()?.to_owned();
    if rule.contains_key("any") {
        return None;
    }
    let Probe {
        grammars,
        hits,
        count,
        unprobed,
    } = if scope.is_file {
        file_probe(q, scope, &rule, &pattern, paths, cancel)?
    } else {
        directory_probe(scope, scan, &rule, &pattern, cancel)?
    };
    if hits.is_empty() && unprobed.is_empty() {
        return None;
    }
    let first = hits
        .first_key_value()
        .map(|(path, line)| format!("{}:{line}", row_paths.delivered(path)));
    let unprobed = unprobed
        .first()
        .map(|path| (unprobed.len(), row_paths.delivered(path)));
    let mut names = Vec::new();
    for (qualified, found) in grammars.values().flatten() {
        let spelled = qualified.names();
        for index in found.iter().flatten() {
            if let Some(name) = spelled.get(*index)
                && !names.iter().any(|known| known == name)
            {
                names.push((*name).to_owned());
            }
        }
    }
    let pinned = grammars.len() > 1;
    let queries = grammars
        .iter()
        .map(|(grammar, spelled)| {
            let longer = spelled.iter().flat_map(|(qualified, found)| {
                let mut found = found.iter().collect::<Vec<_>>();
                found.sort_by(|a, b| (a.len(), a).cmp(&(b.len(), b)));
                found.into_iter().map(|names| qualified.spelling(names))
            });
            let spellings = std::iter::once(pattern.clone())
                .chain(longer)
                .map(|spelled| json!({"pattern": spelled}))
                .collect::<Vec<_>>();
            let mut lead = rule.clone();
            lead.remove("pattern");
            lead.insert("any".into(), Value::Array(spellings));
            let mut query = serde_json::to_value(with_rule(q, scope, Value::Object(lead))?).ok()?;
            if pinned {
                query["parser"] = json!(grammar);
            }
            serde_json::from_value(query).ok()
        })
        .collect::<Option<Vec<_>>>()?;
    Some(QualifiedMiss {
        names,
        count,
        files: hits.len(),
        first,
        unprobed,
        queries,
    })
}

/// The parser ID of the grammar that reads `extension`.
fn extension_grammar(extension: &str) -> String {
    octocode_engine::signatures::languages::find_entry(extension)
        .map(octocode_engine::signatures::languages::label)
        .unwrap_or_default()
}

/// One longer spelling of the pattern, and each set of names a match
/// qualified with it.
type Spelled = (QualifiedPattern, BTreeSet<Vec<usize>>);

/// What the longer spellings matched.
struct Probe {
    /// Per grammar (parser ID) that read a longer spelling: each of the
    /// pattern's longer spellings there that matched, and each set of names
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

    /// One file's longer spellings (`found`, in match order), read with
    /// `qualified` in the grammar `extension` names.
    fn add(
        &mut self,
        path: &str,
        extension: &str,
        qualified: &QualifiedPattern,
        found: Vec<(u64, Vec<usize>)>,
    ) {
        let Some(line) = found.iter().map(|(line, _)| *line).min() else {
            return;
        };
        self.hits
            .entry(path.to_owned())
            .and_modify(|first| *first = line.min(*first))
            .or_insert(line);
        self.count += found.len();
        let spelled = self.grammars.entry(extension_grammar(extension)).or_default();
        let at = spelled
            .iter()
            .position(|(known, _)| known == qualified)
            .unwrap_or_else(|| {
                spelled.push((qualified.clone(), BTreeSet::new()));
                spelled.len() - 1
            });
        spelled[at]
            .1
            .extend(found.into_iter().map(|(_, names)| names));
    }
}

/// The skeleton's matches that qualify some of the pattern's names: each
/// one's line and the names it qualifies.
///
/// The captures only name candidates (a capture can hold `&m::Foo`, which
/// ends like a path but is another type), so each candidate counts only
/// when the spellings of the names it qualifies match it too: the note
/// counts exactly what its lead returns. `spelled` runs a rule over the same
/// file and gives its matches' spans; `None` when that run did not finish.
fn longer_spellings(
    rule: &Map<String, Value>,
    qualified: &QualifiedPattern,
    matches: &[StructuralDetailedMatch],
    spelled: impl FnOnce(Value) -> Option<BTreeSet<Span>>,
) -> Option<Vec<(u64, Vec<usize>)>> {
    let candidates = matches
        .iter()
        .filter_map(|found| Some((span(found), qualified.qualified_names(&found.metavars)?)))
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Some(Vec::new());
    }
    let spellings = candidates
        .iter()
        .map(|(_, names)| names)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|names| json!({"pattern": qualified.spelling(names)}))
        .collect();
    let mut any = rule.clone();
    any.remove("pattern");
    any.insert("any".into(), Value::Array(spellings));
    let spans = spelled(Value::Object(any))?;
    Some(
        candidates
            .into_iter()
            .filter(|(at, _)| spans.contains(at))
            .map(|(at, names)| (u64::from(at.0), names))
            .collect(),
    )
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

/// `rule` with `pattern` in place of its own.
fn probe_rule(rule: &Map<String, Value>, pattern: &str) -> Value {
    let mut probe = rule.clone();
    probe.insert("pattern".into(), json!(pattern));
    Value::Object(probe)
}

/// A file scope: the file scanned once more with the skeleton.
fn file_probe(
    q: MatchQuery<'_>,
    scope: &Scope,
    rule: &Map<String, Value>,
    pattern: &str,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Option<Probe> {
    let extension = scope_extension(scope)?;
    let qualifiers = pattern_qualifiers(pattern, &extension);
    if qualifiers.is_empty() {
        return None;
    }
    let mut probe = Probe::new();
    let path = scope.canonical.to_string_lossy();
    for qualified in &qualifiers {
        let query = with_rule(q, scope, probe_rule(rule, qualified.skeleton()))?;
        let found = rule_scan(&query, paths, cancel).and_then(|scan| {
            let file = scan.files.into_iter().find(|file| file.status == "ok")?;
            longer_spellings(rule, qualified, file.matches.held(), |any| {
                let spelled = rule_scan(&with_rule(q, scope, any)?, paths, cancel)?;
                let file = spelled.files.first().filter(|file| file.status == "ok")?;
                Some(file.matches.held().iter().map(span).collect())
            })
        });
        match found {
            Some(found) => probe.add(&path, &extension, qualified, found),
            None if probe.unprobed.is_empty() => probe.unprobed.push(path.to_string()),
            None => {}
        }
    }
    Some(probe)
}

/// A directory scope: each file the scan parsed that spells a name after a
/// separator, matched again from the bytes it scanned under the grammar it
/// was read with and that grammar's skeleton of the pattern. A file that
/// changed or did not finish matching voids the count.
fn directory_probe(
    scope: &Scope,
    scan: &Scan,
    rule: &Map<String, Value>,
    pattern: &str,
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
            .or_insert_with(|| pattern_qualifiers(pattern, &extension));
        if !qualifiers.is_empty() {
            probed.push((file, source, digest, extension));
        }
    }
    let workers = crate::tools::workers::workers(probed.len(), None);
    let stop = || cancel.check().is_err();
    let found = crate::tools::workers::each_parallel(
        &mut probed,
        workers,
        |(_, source, digest, extension)| {
            let qualifiers = grammars.get(extension.as_str())?;
            // Reading the bytes is far cheaper than parsing them: a file
            // that spells no name after a separator holds no longer
            // spelling. One that no longer reads is matched, which says so.
            let limit = crate::security::SourceLimits::REAL.whole_source as u64;
            let text = octocode_engine::text::read_source(source, limit)
                .ok()
                .map(|read| String::from_utf8_lossy(&read.bytes).into_owned());
            let matches = |rule: &Value| match octocode_engine::structural::file_matches(
                source,
                extension,
                None,
                Some(rule),
                digest,
                limit,
                &stop,
            ) {
                Ok(Rebuilt::Matches(matches)) => Some(matches),
                _ => None,
            };
            qualifiers
                .iter()
                .map(|qualified| {
                    if text.as_deref().is_some_and(|text| !qualified.may_qualify(text)) {
                        return Some(Vec::new());
                    }
                    let skeleton = matches(&probe_rule(rule, qualified.skeleton()))?;
                    longer_spellings(rule, qualified, &skeleton, |any| {
                        Some(matches(&any)?.iter().map(span).collect())
                    })
                })
                .collect::<Option<Vec<_>>>()
        },
    );
    for ((file, _, _, extension), outcome) in probed.iter().zip(found) {
        match (grammars.get(extension.as_str()), outcome) {
            (Some(qualifiers), Some(found)) => {
                for (qualified, found) in qualifiers.iter().zip(found) {
                    probe.add(&file.path, extension, qualified, found);
                }
            }
            _ => probe.unprobed.push(file.path.clone()),
        }
    }
    Some(probe)
}

/// Where a [`QualifiedMiss`] note sends its reader.
const RECOVERY: &str = "The related astSearch rule with `any` lists every spelling found.";

impl QualifiedMiss {
    /// The note that counts the longer spellings (a hint on an empty page, a
    /// diagnostic beside matches), and the rule query that lists every
    /// spelling.
    pub(super) fn attach(self, out: &mut Value) {
        let Self {
            names,
            count,
            files,
            first,
            unprobed,
            queries,
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
        let message = match first {
            Some(first) => format!(
                "The pattern matches the path as written: {count} more match(es) in {files} file(s) spell a path or receiver before {names} (first: {first}).{unprobed}"
            ),
            None => format!("The pattern matches the path as written.{unprobed}"),
        };
        if out["status"] == "empty" {
            // An empty page reads as "no such call": the one line that says
            // otherwise is a hint, where every surface shows it.
            crate::response::result::add_hints(
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
                    "recovery": RECOVERY,
                }),
            );
        }
        if queries.is_empty() {
            return;
        }
        // A lead an empty page's own explanation already offers stays: this
        // one takes the next numbered `broadenSearch`.
        crate::response::result::place_numbered(
            out,
            "broadenSearch",
            crate::response::result::Continuation::related_input(
                ToolId::AstSearch,
                json!({"queries": queries}),
            )
            .build(),
        );
    }
}

#[cfg(test)]
#[path = "qualified_tests.rs"]
mod tests;
