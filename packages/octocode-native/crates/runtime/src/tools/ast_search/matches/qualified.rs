//! The qualified-path note: a pattern matches a path as written, so
//! `Mutex::new($X)` leaves out `std::sync::Mutex::new(x)`, and a call on a
//! bare name leaves out the calls that qualify it (`redact_lines($$$A)`
//! against `crate::security::scan::redact_lines(x)`). A first page counts
//! those longer spellings in the files its scan evaluated and leads to the
//! rule that lists them all.
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
use octocode_engine::structural::{Rebuilt, pattern_qualifier};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Longer spellings of a pattern's path the scan's files hold.
pub(super) struct QualifiedMiss {
    head: String,
    count: usize,
    files: usize,
    /// The first longer spelling, as `path:line`.
    first: String,
    /// The rule listing the pattern and its longer spelling: one query, or
    /// one per grammar when the scan's grammars spell the longer pattern
    /// differently (a pattern compiles in the grammar it is written in).
    queries: Vec<AstSearchQueryMatch0>,
}

/// [`QualifiedMiss`] of a first page whose `rule.pattern` is a `::` path
/// (or a call on one) or a call on a bare name: its longer spelling matched
/// over every file the scan evaluated. A file the literal prefilter skipped
/// holds no match of the longer spelling, which spells every literal of the
/// pattern. `None`
/// when the pattern has no longer spelling, the scan or a probe did not
/// finish, or no file holds one.
pub(super) fn qualified_miss(
    q: MatchQuery<'_>,
    scope: &Scope,
    scan: &Scan,
    (paths, row_paths): (&PathPolicy, RowPaths<'_>),
    cancel: &dyn CancellationCheck,
) -> Option<QualifiedMiss> {
    if q.page() != 1 || q.match_page() != 1 || q.scan_offset() != 0 || scan.truncated {
        return None;
    }
    let rule = q.to_value().get("rule")?.as_object()?.clone();
    let pattern = rule.get("pattern")?.as_str()?.to_owned();
    if rule.contains_key("any") {
        return None;
    }
    let found = if scope.is_file {
        file_probe(q, scope, &rule, &pattern, paths, cancel)?
    } else {
        directory_probe(scope, scan, &rule, &pattern, cancel)?
    };
    let Probe {
        head,
        spellings,
        hits,
        count,
    } = found;
    let (path, line) = hits
        .first_key_value()
        .map(|(path, line)| (path.clone(), *line))?;
    let pinned = spellings.len() > 1;
    let queries = spellings
        .iter()
        .map(|(grammar, broader)| {
            let mut lead = rule.clone();
            lead.remove("pattern");
            lead.insert(
                "any".into(),
                json!([{"pattern": pattern}, {"pattern": broader}]),
            );
            let mut query = serde_json::to_value(with_rule(q, scope, Value::Object(lead))?).ok()?;
            if pinned {
                query["parser"] = json!(grammar);
            }
            serde_json::from_value(query).ok()
        })
        .collect::<Option<Vec<_>>>()?;
    Some(QualifiedMiss {
        head,
        count,
        files: hits.len(),
        first: format!("{}:{line}", row_paths.delivered(&path)),
        queries,
    })
}

/// The parser ID of the grammar that reads `extension`.
fn extension_grammar(extension: &str) -> String {
    octocode_engine::signatures::languages::find_entry(extension)
        .map(octocode_engine::signatures::languages::label)
        .unwrap_or_default()
}

/// What the longer spellings matched.
struct Probe {
    /// The name the pattern's longer spellings qualify.
    head: String,
    /// Each distinct longer spelling the scan's grammars gave the pattern,
    /// with the grammar that spells it.
    spellings: Vec<(String, String)>,
    /// The first line per file (by path).
    hits: BTreeMap<String, u64>,
    /// The match count.
    count: usize,
}

/// `rule` with its pattern spelled longer.
fn probe_rule(rule: &Map<String, Value>, broader: &str) -> Value {
    let mut probe = rule.clone();
    probe.insert("pattern".into(), json!(broader));
    Value::Object(probe)
}

/// A file scope: the file scanned once more with the longer spelling.
fn file_probe(
    q: MatchQuery<'_>,
    scope: &Scope,
    rule: &Map<String, Value>,
    pattern: &str,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Option<Probe> {
    let extension = scope_extension(scope)?;
    let qualified = pattern_qualifier(pattern, &extension)?;
    let query = with_rule(q, scope, probe_rule(rule, &qualified.broader))?;
    let scan = rule_scan(&query, paths, cancel)?;
    let file = scan.files.first().filter(|file| file.status == "ok")?;
    let matches = file.matches.held();
    let line = matches.first().map(|found| u64::from(found.start_line))?;
    Some(Probe {
        head: qualified.head,
        spellings: vec![(extension_grammar(&extension), qualified.broader)],
        hits: BTreeMap::from([(file.path.clone(), line)]),
        count: matches.len(),
    })
}

/// A directory scope: each file the scan parsed, matched again from the
/// bytes it scanned under the grammar it was read with and the longer
/// spelling that grammar gives the pattern. The grammars must agree on the
/// name they qualify; a file that changed or did not finish matching voids
/// the count.
fn directory_probe(
    scope: &Scope,
    scan: &Scan,
    rule: &Map<String, Value>,
    pattern: &str,
    cancel: &dyn CancellationCheck,
) -> Option<Probe> {
    let mut spellings = BTreeMap::new();
    let mut probed = Vec::new();
    for file in scan.files.iter().filter(|file| file.status == "ok") {
        let Some((source, digest)) = &file.source else {
            return None;
        };
        let extension = parser_extension(scope, source);
        let qualified = spellings
            .entry(extension.clone())
            .or_insert_with(|| pattern_qualifier(pattern, &extension));
        if let Some(qualified) = qualified {
            probed.push((file, source, digest, extension, qualified.broader.clone()));
        }
    }
    let spelled = spellings
        .into_iter()
        .filter_map(|(extension, qualified)| Some((extension, qualified?)))
        .collect::<Vec<_>>();
    let head = spelled.first()?.1.head.clone();
    if spelled.iter().any(|(_, qualified)| qualified.head != head) {
        return None;
    }
    let spellings = spelled
        .into_iter()
        .map(|(extension, qualified)| (extension_grammar(&extension), qualified.broader))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let workers = crate::tools::workers::workers(probed.len(), None);
    let stop = || cancel.check().is_err();
    let found = crate::tools::workers::each_parallel(
        &mut probed,
        workers,
        |(_, source, digest, extension, broader)| match octocode_engine::structural::file_matches(
            source,
            extension,
            None,
            Some(&probe_rule(rule, broader)),
            digest,
            crate::security::SourceLimits::REAL.whole_source as u64,
            &stop,
        ) {
            Ok(Rebuilt::Matches(matches)) => Some(
                matches
                    .first()
                    .map(|m| (u64::from(m.start_line), matches.len())),
            ),
            _ => None,
        },
    );
    let mut hits = BTreeMap::new();
    let mut count = 0;
    for ((file, ..), outcome) in probed.iter().zip(found) {
        if let Some((line, matches)) = outcome? {
            hits.insert(file.path.clone(), line);
            count += matches;
        }
    }
    Some(Probe {
        head,
        spellings,
        hits,
        count,
    })
}

impl QualifiedMiss {
    /// The note that counts the longer spellings, and the rule query that
    /// lists every spelling.
    pub(super) fn attach(self, out: &mut Value) {
        let Self {
            head,
            count,
            files,
            first,
            queries,
        } = self;
        let diagnostic = json!({
            "code": "structural.pattern.qualified",
            "severity": "info",
            "stage": "match",
            "message": format!(
                "The pattern matches the path as written: {count} more match(es) in {files} file(s) spell a path or receiver before `{head}` (first: {first})."
            ),
            "recovery": "broadenSearch lists every spelling.",
        });
        push_diagnostic(out, diagnostic);
        out["calls"]["broadenSearch"] = crate::response::result::Continuation::related_input(
            ToolId::AstSearch,
            json!({"queries": queries}),
        )
        .build();
    }
}
