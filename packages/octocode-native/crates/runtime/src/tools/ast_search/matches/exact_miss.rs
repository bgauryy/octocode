//! The exact-miss rule: an empty declaration pattern's nearest matches
//! (declarations of its kind and name that differ in a part it spells), else
//! the rule listing its kind.
use super::clipped::row_file;
use super::continuations::tree_lead;
use super::query::MatchQuery;
use super::scan::{scan_directory, scan_file};
use super::scope::{resolve_scope, scope_extension};
use super::types::{Scan, Scope};
use crate::execution::cancel::CancellationCheck;
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use octocode_contract_types::AstSearchQueryMatch0;
use octocode_engine::structural::StructuralDetailedMatch;
use serde_json::{Value, json};

/// A declaration pattern that matched nothing in a scope that holds
/// declarations of its kind (named as the pattern names it): a pattern
/// matches every part it spells, so an item that adds or leaves out a part
/// (a modifier, a return type) is not a match. The rule that lists them.
pub(super) struct ExactMiss {
    pub(super) kind: String,
    pub(super) count: usize,
    pub(super) files: usize,
    pub(super) query: Value,
    /// The syntax tree of the first missed declaration, where the pattern's
    /// shape is compared with the source.
    pub(super) tree: Option<Value>,
}

/// [`ExactMiss`] of an empty `rule.pattern` scan, counted by one scan of its
/// rule over the same scope; `None` when the pattern spells no declaration
/// or the scope holds none of its kind.
pub(super) fn exact_miss(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Option<ExactMiss> {
    let declaration = pattern_declaration(q, scope)?;
    let mut rule = json!({"kind": declaration.kind});
    if let Some((kind, name)) = &declaration.name {
        rule["has"] = json!({"kind": kind, "regex": format!("^{}$", regex::escape(name))});
    }
    let query = with_rule(q, scope, rule)?;
    let scan = rule_scan(&query, paths, cancel)?;
    let count = scan
        .files
        .iter()
        .map(|file| file.matches.len())
        .sum::<usize>();
    let files = scan
        .files
        .iter()
        .filter(|file| !file.matches.is_empty())
        .count();
    let tree = scan
        .files
        .iter()
        .find(|file| !file.matches.is_empty())
        .and_then(|file| {
            let path = row_file(scope, &json!({"path": file.path}))?;
            // A directory scan keeps only row prices: the file is matched
            // once more for the declaration's line.
            let first_line = |matches: &[StructuralDetailedMatch]| {
                matches.first().map(|found| u64::from(found.start_line))
            };
            let line = first_line(file.matches.held()).or_else(|| {
                let bytes = super::super::source::read_parse_source(&path).ok()??;
                let source = super::super::source::parse_text(&path, &bytes).ok()?;
                let parser = Some(file.parser.as_str()).filter(|parser| !parser.is_empty());
                let found = octocode_engine::structural::search_detailed(
                    &source,
                    &path.to_string_lossy(),
                    parser,
                    None,
                    query.get("rule"),
                );
                first_line(&found.matches)
            });
            Some(tree_lead(q, scope, &path, line))
        });
    (count > 0).then_some(ExactMiss {
        kind: declaration.kind,
        count,
        files,
        query,
        tree,
    })
}

/// The declaration an empty `rule.pattern` spells in the scope's grammar.
pub(super) fn pattern_declaration(
    q: MatchQuery<'_>,
    scope: &Scope,
) -> Option<octocode_engine::structural::PatternDeclaration> {
    octocode_engine::structural::pattern_declaration(&q.pattern_text()?, &scope_extension(scope)?)
}

/// `q` as a fresh `rule` query over the same scope (a directory scan pins
/// the grammar it inferred).
fn with_rule(q: MatchQuery<'_>, scope: &Scope, rule: Value) -> Option<Value> {
    let mut query = crate::response::result::fresh_query_row(ToolId::AstSearch, &q.to_value());
    let fields = query.as_object_mut()?;
    fields.insert("rule".into(), rule);
    if !scope.is_file
        && !fields.contains_key("parser")
        && let Some(inferred) = &scope.inferred
    {
        fields.insert("parser".into(), json!(inferred));
    }
    Some(query)
}

/// One scan of a rule `query` over its scope.
fn rule_scan(query: &Value, paths: &PathPolicy, cancel: &dyn CancellationCheck) -> Option<Scan> {
    let rule_query: AstSearchQueryMatch0 = serde_json::from_value(query.clone()).ok()?;
    let rule_query = MatchQuery::Content(&rule_query);
    let rule_scope = resolve_scope(rule_query, paths, cancel).ok()?;
    if rule_scope.is_file {
        scan_file(rule_query, &rule_scope, cancel)
    } else {
        scan_directory(rule_query, &rule_scope, paths, cancel)
    }
    .ok()
}

/// A named declaration pattern with no near miss (no declaration of its
/// kind and name in the scope): the rule that lists every declaration of
/// its kind, where a differently named one is found; `None` when the scope
/// holds none of the kind (an unnamed pattern's [`exact_miss`] already
/// counted them).
pub(super) fn kind_broaden(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Option<Value> {
    let declaration = pattern_declaration(q, scope)?;
    declaration.name.as_ref()?;
    let query = with_rule(q, scope, json!({"kind": declaration.kind}))?;
    rule_scan(&query, paths, cancel)?
        .files
        .iter()
        .any(|file| !file.matches.is_empty())
        .then(|| crate::response::result::Continuation::related(ToolId::AstSearch, query).build())
}

impl ExactMiss {
    /// The diagnostic that counts the declarations the pattern missed, and
    /// the rule query that lists them.
    pub(super) fn attach(self, out: &mut Value) {
        let Self {
            kind,
            count,
            files,
            query,
            tree,
        } = self;
        let diagnostic = json!({
            "code": "structural.pattern.exact",
            "severity": "warning",
            "stage": "match",
            "message": format!(
                "The pattern matches as written: {count} {kind} node(s) in {files} file(s) differ from it in a part it spells or leaves out (a modifier, a return type)."
            ),
            "recovery": "Spell those parts in the pattern, or list every node of the kind with a rule.",
        });
        match out["diagnostics"].as_array_mut() {
            Some(diagnostics) => diagnostics.push(diagnostic),
            None => out["diagnostics"] = json!([diagnostic]),
        }
        if let Some(tree) = tree {
            out["calls"]["viewSyntax"] = tree;
        }
        out["calls"]["broadenSearch"] =
            crate::response::result::Continuation::related(ToolId::AstSearch, query).build();
    }
}
