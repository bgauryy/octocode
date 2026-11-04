//! The exact-miss rule: an empty declaration pattern's nearest matches
//! (declarations of its kind and name that differ in a part it spells), else
//! the rule listing its kind.
use super::continuations::tree_lead;
use super::query::MatchQuery;
use super::reads::row_file;
use super::rows::push_diagnostic;
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
/// declarations of its kind (named as the pattern names it): a pattern leaves
/// out what it does not spell, but each part it spells must agree, so an item
/// that differs in one is not a match. The rule that lists them.
pub(super) struct ExactMiss {
    pub(super) kind: String,
    pub(super) count: usize,
    pub(super) files: usize,
    pub(super) query: AstSearchQueryMatch0,
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
    let mut rule = kind_rule(&declaration);
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
                    MatchQuery::Content(&query).rule().as_ref(),
                );
                first_line(&found.matches)
            });
            tree_lead(q, scope, &path, line)
        });
    (count > 0).then_some(ExactMiss {
        kind: declaration.kind,
        count,
        files,
        query,
        tree,
    })
}

/// The rule listing every declaration of a pattern's kind, in the wrapper the
/// pattern spells around it.
fn kind_rule(declaration: &octocode_engine::structural::PatternDeclaration) -> Value {
    let mut rule = json!({"kind": declaration.kind});
    if let Some(inside) = &declaration.inside {
        rule["inside"] = json!({"kind": inside});
    }
    rule
}

/// The declaration an empty `rule.pattern` spells in the scope's grammar.
pub(super) fn pattern_declaration(
    q: MatchQuery<'_>,
    scope: &Scope,
) -> Option<octocode_engine::structural::PatternDeclaration> {
    octocode_engine::structural::pattern_declaration(&q.pattern_text()?, &scope_extension(scope)?)
}

/// The hint of a pattern that spells the smaller form of a declaration (a
/// signature without its body): it matches that form alone, so the fuller
/// kind the grammar has is what lists the definitions. `None` for a rule
/// that already names its kind, or a pattern of no smaller form.
pub(super) fn fuller_form_hint(q: MatchQuery<'_>, scope: &Scope) -> Option<String> {
    if q.rule()?.get("kind").is_some() {
        return None;
    }
    let declaration = pattern_declaration(q, scope)?;
    let fuller = declaration.fuller_kinds.first()?;
    Some(format!(
        "The pattern spells a {} without the parts of a {fuller}, so it matches only the former. Spell the missing part (a body, `{{ $$$ }}`) or use rule {{\"kind\":\"{fuller}\"}}.",
        declaration.kind
    ))
}

/// `q` as a fresh `rule` query that lists its nodes, over the same scope (a
/// directory scan pins the grammar it inferred). A file view's `view` is not
/// one a listing takes.
pub(super) fn with_rule(
    q: MatchQuery<'_>,
    scope: &Scope,
    rule: Value,
) -> Option<AstSearchQueryMatch0> {
    let mut query = crate::response::result::fresh_query_row(ToolId::AstSearch, &q.to_value());
    let fields = query.as_object_mut()?;
    fields.insert("rule".into(), rule);
    fields.shift_remove("view");
    if !scope.is_file
        && !fields.contains_key("parser")
        && let Some(inferred) = &scope.inferred
    {
        fields.insert("parser".into(), json!(inferred));
    }
    serde_json::from_value(query).ok()
}

/// One scan of a rule `query` over its scope.
pub(super) fn rule_scan(
    query: &AstSearchQueryMatch0,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Option<Scan> {
    let rule_query = MatchQuery::Content(query);
    let rule_scope = resolve_scope(rule_query, paths, cancel).ok()?;
    if rule_scope.is_file {
        scan_file(rule_query, &rule_scope, paths, cancel)
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
    let query = with_rule(q, scope, kind_rule(&declaration))?;
    rule_scan(&query, paths, cancel)?
        .files
        .iter()
        .any(|file| !file.matches.is_empty())
        .then(|| crate::response::result::Continuation::related_query(query).build())
}

/// A declaration pattern over a scope with none of its kind: the pattern may
/// read as another declaration than the one meant (in C#, a method without a
/// return type is a constructor), so the row names its reading and leads to
/// what the scope does declare.
pub(super) fn attach_absent_kind(q: MatchQuery<'_>, scope: &Scope, out: &mut Value) {
    let Some(declaration) = pattern_declaration(q, scope) else {
        return;
    };
    let kind = declaration.kind;
    push_diagnostic(
        out,
        json!({
            "code": "structural.pattern.exact",
            "severity": "info",
            "stage": "match",
            "message": format!("The pattern reads as a {kind} node, and the scope holds none."),
            "recovery": "broadenSearch lists what the scope declares; syntaxTree shows how a declaration parses.",
        }),
    );
    let row = q.to_value();
    let mut symbols = json!({"operation": "symbols"});
    for field in SYMBOLS_WALK {
        if let Some(value) = row.get(field) {
            symbols[field] = value.clone();
        }
    }
    out["calls"]["broadenSearch"] = crate::response::result::Continuation::related_input(
        ToolId::AstSearch,
        json!({"queries": [symbols]}),
    )
    .build();
}

/// The fields of a match that a `symbols` row walks the same scope by.
const SYMBOLS_WALK: [&str; 8] = [
    "path",
    "exclude",
    "defaultExcludes",
    "includeIgnored",
    "includeHidden",
    "testCode",
    "parser",
    "parserGlobs",
];

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
                "{count} {kind} node(s) in {files} file(s) differ from the pattern in a part it spells: its parameters, its body, a type or modifier it names."
            ),
            "recovery": "Replace a spelled part with $X or $$$, or list every node of the kind with broadenSearch.",
        });
        push_diagnostic(out, diagnostic);
        if let Some(tree) = tree {
            out["calls"]["viewSyntax"] = tree;
        }
        out["calls"]["broadenSearch"] =
            crate::response::result::Continuation::related_query(query).build();
    }
}
