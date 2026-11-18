//! The exact-miss rule: an empty declaration pattern's nearest matches
//! (declarations of its kind and name that differ in a part it spells), and
//! the rule listing its kind. Each note states only what its own count
//! proved.
use super::continuations::tree_lead;
use super::query::MatchQuery;
use super::reads::row_file;
use super::rows::push_diagnostic;
use super::scan::{scan_directory, scan_file};
use super::scope::{resolve_scope, scope_extension};
use super::types::{Scan, Scope};
use octocode_contract_types::AstSearchQueryMatch0;
use octocode_engine::structural::StructuralDetailedMatch;
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::policy::path::PathPolicy;
use serde_json::{Value, json};

/// A declaration pattern that matched nothing, and the census of its kind's
/// nodes in the scope: a pattern leaves out what it does not spell, but each
/// part it spells must agree, so a node that differs in one is not a match.
/// The rule that lists the counted nodes.
pub(super) struct ExactMiss {
    kind: String,
    count: usize,
    files: usize,
    query: AstSearchQueryMatch0,
    /// The syntax tree of the first counted node, where the pattern's shape
    /// is compared with the source.
    tree: Option<Value>,
    /// The fields of the first counted node that differ from what the
    /// pattern spells there (`type_parameters`); empty when none is known.
    differs: Vec<String>,
    /// What the census narrowed the kind's nodes to, and the rule listing
    /// every node of the kind (offered, never run here); `None` when the
    /// census counted every node of the kind.
    narrowed: Option<(Narrowing, AstSearchQueryMatch0)>,
    /// The `symbols` row that lists what the scope declares, for a full
    /// census without a wrapper that counted no node of the kind.
    symbols: Option<Value>,
    /// The wrapper the pattern spells around the declaration, whose nodes
    /// alone the census counted, and the rule listing the kind's nodes in
    /// any parent (offered on a zero count, never run here).
    wrapper: Option<(String, AstSearchQueryMatch0)>,
    /// What the census scan did not read, when it did not read every file:
    /// its count is then a floor and its zero proves no absence.
    cut: Option<String>,
    /// The summary `view` the call asked for, which its leads keep
    /// ([`asked_view`]).
    view: Option<Value>,
}

/// The nodes of the kind an [`ExactMiss`] census counted.
enum Narrowing {
    /// A named pattern's: those that bear its name.
    Name(String),
    /// An unnamed pattern's that spells a literal name: those whose text,
    /// or their wrapper's when the pattern spells one, contains the longest
    /// such name.
    Text(String),
}

/// [`ExactMiss`] of an empty `rule.pattern` scan, counted by one scan of its
/// rule over the same scope; `None` when the pattern spells no declaration
/// or the census did not run.
///
/// A named pattern counts the declarations of its name. An unnamed one that
/// spells a literal name (a type or path it names, never a keyword) counts
/// the nodes of its kind that contain its longest such name, in the wrapper
/// the pattern spells around the declaration when it spells one (the wrapper
/// holds the declaration's text and its own). Either rule's literal `regex`
/// lets the scan parse only the files that spell it, so neither counts the
/// kind's other nodes: the rule listing them is offered, not run. Any other
/// pattern counts every node of its kind.
pub(super) fn exact_miss(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Option<ExactMiss> {
    let declaration = pattern_declaration(q, scope)?;
    let mut rule = kind_rule(&declaration);
    let narrowing = if let Some((kind, name)) = &declaration.name {
        rule["has"] = json!({"kind": kind, "regex": format!("^{}$", regex::escape(name))});
        Some(Narrowing::Name(name.clone()))
    } else {
        longest_name(q, scope).map(|literal| {
            let regex = json!(regex::escape(&literal));
            match &declaration.inside {
                Some(_) => rule["inside"]["regex"] = regex,
                None => rule["regex"] = regex,
            }
            Narrowing::Text(literal)
        })
    };
    let wrapper = match &declaration.inside {
        Some(inside) => Some((
            inside.clone(),
            with_rule(q, scope, kinds_rule(&census_kinds(&declaration)))?,
        )),
        None => None,
    };
    let narrowed = match narrowing {
        Some(narrowing) => Some((narrowing, with_rule(q, scope, kind_rule(&declaration))?)),
        None => None,
    };
    let query = with_rule(q, scope, rule)?;
    let scan = census_scan(&query, paths, cancel)?;
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
    // The first counted node: where its syntax tree is offered and which
    // spelled part of it differs from the pattern.
    let first = scan
        .files
        .iter()
        .find(|file| !file.matches.is_empty())
        .and_then(|file| {
            let path = row_file(scope, &json!({"path": file.path}))?;
            let bytes = super::super::source::read_parse_source(&path).ok()??;
            let source = super::super::source::matched_text(&path, &bytes)
                .ok()?
                .into_owned();
            // A directory scan keeps only row prices: the file is matched
            // once more for the declaration's line.
            let first_line =
                |matches: &[StructuralDetailedMatch]| matches.first().map(|found| found.start_line);
            let line = first_line(file.matches.held()).or_else(|| {
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
            let differs = line
                .zip(q.pattern_text())
                .zip(scope_extension(scope))
                .map(|((line, pattern), extension)| {
                    octocode_engine::structural::pattern_differences(
                        &pattern, &extension, &source, line,
                    )
                })
                .unwrap_or_default();
            Some((tree_lead(q, scope, &path, line.map(u64::from)), differs))
        });
    let (tree, differs) = match first {
        Some((tree, differs)) => (tree, differs),
        None => (None, Vec::new()),
    };
    let symbols = (count == 0 && narrowed.is_none() && wrapper.is_none()).then(|| symbols_row(q));
    Some(ExactMiss {
        kind: census_kinds(&declaration).join(" or "),
        count,
        files,
        query,
        tree,
        differs,
        narrowed,
        symbols,
        wrapper,
        cut: census_cut(&scan),
        view: asked_view(q),
    })
}

/// What a census scan did not read, `None` when it read every file it could
/// hold a node in: a time-budget cut, unreadable or over-large files, and
/// entries the walk could not read. Files no grammar parses hold no node of
/// the kind, so they leave the census complete.
fn census_cut(scan: &Scan) -> Option<String> {
    let (_, unreadable, oversized) = scan.skips;
    let mut cut = Vec::new();
    if scan.truncated {
        cut.push("the census stopped before it read every file".to_owned());
    }
    if unreadable + oversized > 0 {
        cut.push(format!(
            "{} file(s) were unreadable or too large to parse",
            unreadable + oversized
        ));
    }
    if scan.walk_errors.skipped > 0 {
        cut.push(format!(
            "{} entry(ies) could not be read",
            scan.walk_errors.skipped
        ));
    }
    (!cut.is_empty()).then(|| cut.join("; "))
}

/// The longest literal name an unnamed pattern spells (a type or path it
/// names, never a keyword or primitive type).
fn longest_name(q: MatchQuery<'_>, scope: &Scope) -> Option<String> {
    q.pattern_text()
        .zip(scope_extension(scope))
        .and_then(|(pattern, extension)| {
            octocode_engine::structural::pattern_qualifier(&pattern, &extension)
        })
        .and_then(|qualified| {
            qualified
                .names()
                .into_iter()
                .max_by_key(|name| name.len())
                .map(str::to_owned)
        })
}

#[cfg(test)]
#[path = "exact_miss_tests.rs"]
mod tests;

/// The kinds a census counts: the declaration's own, the others the pattern
/// matches as it, and the fuller forms a near miss may take (a definition
/// beside a signature pattern).
fn census_kinds(declaration: &octocode_engine::structural::PatternDeclaration) -> Vec<&str> {
    let mut kinds = vec![declaration.kind.as_str()];
    for kind in declaration
        .alternate_kinds
        .iter()
        .chain(&declaration.fuller_kinds)
    {
        if !kinds.contains(&kind.as_str()) {
            kinds.push(kind);
        }
    }
    kinds
}

/// The rule matching a node of any of `kinds`.
fn kinds_rule(kinds: &[&str]) -> Value {
    match kinds {
        [kind] => json!({ "kind": kind }),
        _ => json!({ "any": kinds.iter().map(|kind| json!({ "kind": kind })).collect::<Vec<_>>() }),
    }
}

/// The rule listing every declaration of a pattern's kinds
/// ([`census_kinds`]), in the wrapper the pattern spells around it.
fn kind_rule(declaration: &octocode_engine::structural::PatternDeclaration) -> Value {
    let mut rule = kinds_rule(&census_kinds(declaration));
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
    let mut query =
        octocode_runtime_base::response::result::fresh_query_row(ToolId::AstSearch, &q.to_value());
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

/// The `view` a summary call asked for (`files`, `countMatches`); `None`
/// for content. A lead asks the call's question again, so it keeps it.
pub(super) fn asked_view(q: MatchQuery<'_>) -> Option<Value> {
    match q {
        MatchQuery::Summary(summary) => serde_json::to_value(summary.view).ok(),
        MatchQuery::Content(_) => None,
    }
}

/// `query`, a [`with_rule`] listing, as the row a lead runs: in `view`, the
/// summary the call asked for ([`asked_view`]), when there is one.
pub(super) fn lead_row(query: &AstSearchQueryMatch0, view: Option<&Value>) -> Option<Value> {
    let mut row = serde_json::to_value(query).ok()?;
    if let Some(view) = view {
        row["view"] = view.clone();
    }
    Some(row)
}

/// [`lead_row`] as a related call.
pub(super) fn lead_call(
    query: AstSearchQueryMatch0,
    view: Option<&Value>,
) -> octocode_runtime_base::response::result::Continuation {
    use octocode_runtime_base::response::result::Continuation;
    match view.and_then(|view| lead_row(&query, Some(view))) {
        Some(row) => Continuation::related_input(ToolId::AstSearch, json!({"queries": [row]})),
        None => Continuation::related_query(query),
    }
}

/// The census scan of an [`ExactMiss`]: one [`rule_scan`], cut after
/// [`census_window`] files in tests.
fn census_scan(
    query: &AstSearchQueryMatch0,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Option<Scan> {
    #[cfg(test)]
    let _window = CENSUS_WINDOW
        .with(std::cell::Cell::get)
        .map(super::super::scan_window::test_window);
    rule_scan(query, paths, cancel)
}

#[cfg(test)]
thread_local! {
    static CENSUS_WINDOW: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

/// Census scans on this thread evaluate `files` files per window until the
/// guard drops (the page's own scan is not cut): a deterministic stand-in
/// for a census the time budget cut.
#[cfg(test)]
fn census_window(files: u32) -> impl Drop {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            CENSUS_WINDOW.with(|window| window.set(None));
        }
    }
    CENSUS_WINDOW.with(|window| window.set(Some(files)));
    Reset
}

#[cfg(test)]
thread_local! {
    /// Tests: the probe scans ([`rule_scan`]) this thread ran.
    pub(super) static RULE_SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// One scan of a rule `query` over its scope.
pub(super) fn rule_scan(
    query: &AstSearchQueryMatch0,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Option<Scan> {
    #[cfg(test)]
    RULE_SCANS.with(|scans| scans.set(scans.get() + 1));
    let rule_query = MatchQuery::Content(query);
    let rule_scope = resolve_scope(rule_query, paths, cancel).ok()?;
    if rule_scope.is_file {
        scan_file(rule_query, &rule_scope, paths, cancel)
    } else {
        scan_directory(rule_query, &rule_scope, paths, cancel, None)
    }
    .ok()
}

/// The `symbols` row that walks `q`'s scope: what it declares.
fn symbols_row(q: MatchQuery<'_>) -> Value {
    let row = q.to_value();
    let mut symbols = json!({"operation": "symbols"});
    for field in SYMBOLS_WALK {
        if let Some(value) = row.get(field) {
            symbols[field] = value.clone();
        }
    }
    symbols
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

/// `word` after its indefinite article.
fn with_article(word: &str) -> String {
    let article = if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    };
    format!("{article} {word}")
}

impl ExactMiss {
    /// The diagnostic that states what the census counted, and the rule
    /// queries that list the counted nodes and the kind's other nodes; a
    /// full census that counted none names the pattern's reading and leads
    /// to what the scope declares. A census that did not read every file
    /// proves no absence, and its count is a floor.
    pub(super) fn attach(self, out: &mut Value) {
        let Self {
            kind,
            count,
            files,
            query,
            tree,
            differs,
            narrowed,
            symbols,
            wrapper,
            cut,
            view,
        } = self;
        let (narrowing, every) = narrowed.unzip();
        let (wrapper, any_parent) = wrapper.unzip();
        // The nodes the census counted from: those in the pattern's wrapper.
        let parent = wrapper
            .as_deref()
            .map(|wrapper| format!(" with {} parent", with_article(wrapper)))
            .unwrap_or_default();
        let leads = Leads {
            counted: (count > 0).then_some(query),
            every,
            any_parent,
            symbols,
            view,
        };
        let listed = leads.listed(&kind, &parent);
        let syntax = "syntaxTree shows how a declaration parses.";
        let replace = "Replace a spelled part with $X or $$$";
        let differ = match differs.as_slice() {
            [] => "differ from the pattern in a part it spells: its parameters, its body, a type or modifier it names.".to_owned(),
            fields => format!(
                "differ from the pattern in a part it spells; the first differs in its {}.",
                fields.join(", ")
            ),
        };
        let text = |literal: &str| format!("the text `{literal}` (the pattern's longest name)");
        // What a zero count found none of, as one noun phrase.
        let none_of = match &narrowing {
            None => format!("{kind} node{parent}"),
            Some(Narrowing::Name(name)) => format!("{kind} node{parent} named `{name}`"),
            Some(Narrowing::Text(literal)) if parent.is_empty() => {
                format!("{kind} node that contains {}", text(literal))
            }
            Some(Narrowing::Text(literal)) => {
                format!("{kind} node{parent} that contains {}", text(literal))
            }
        };
        let (severity, message, recovery) = match (&narrowing, count) {
            (_, 0) if cut.is_some() => (
                "warning",
                format!(
                    "The census of {kind} nodes is incomplete ({}), so absence is unproven: it counted no {none_of} in what it read.",
                    cut.as_deref().unwrap_or_default()
                ),
                format!("Narrow the path and rerun; {listed}; {syntax}"),
            ),
            (None, 0) if parent.is_empty() => (
                "info",
                format!("The pattern reads as a {kind} node, and the scope holds none."),
                format!("{}; {syntax}", capitalized(&listed)),
            ),
            (None, 0) => (
                "info",
                format!(
                    "The scope holds no {kind} node{parent}; the pattern reads as a {kind} node{parent}. Nodes of the kind in other parents were not counted."
                ),
                format!("{}; {syntax}", capitalized(&listed)),
            ),
            (None, _) => (
                "warning",
                format!("{count} {kind} node(s){parent} in {files} file(s) {differ}"),
                format!("{replace}; {listed}."),
            ),
            (Some(Narrowing::Name(name)), 0) => (
                "info",
                format!(
                    "No {kind} node{parent} in this scope is named `{name}`. The pattern reads as a {kind} node{parent}; nodes of the kind {} were not counted.",
                    if parent.is_empty() {
                        "with other names"
                    } else {
                        "with other names or parents"
                    }
                ),
                format!("{}; {syntax}", capitalized(&listed)),
            ),
            (Some(Narrowing::Name(name)), _) => (
                "warning",
                format!(
                    "{count} {kind} node(s){parent} named `{name}` in {files} file(s) {differ}"
                ),
                format!("{replace}; {listed}."),
            ),
            (Some(Narrowing::Text(literal)), 0) => (
                "info",
                format!(
                    "No {kind} node in this scope {} {}. Nodes of the kind without it were not counted.",
                    match &wrapper {
                        Some(wrapper) =>
                            format!("has {} parent that contains", with_article(wrapper)),
                        None => "contains".to_owned(),
                    },
                    text(literal)
                ),
                format!("{}; {syntax}", capitalized(&listed)),
            ),
            (Some(Narrowing::Text(literal)), _) => {
                let verb = match &wrapper {
                    Some(wrapper) => format!("have {} parent that contains", with_article(wrapper)),
                    None => "contain".to_owned(),
                };
                (
                    "warning",
                    format!(
                        "{count} {kind} node(s) {verb} {}; in {files} file(s), they {differ}",
                        text(literal)
                    ),
                    format!("{replace}; {listed}."),
                )
            }
        };
        let message = match (&cut, count) {
            (Some(cut), 1..) => {
                format!("{message} The census is incomplete ({cut}), so the count is a floor.")
            }
            _ => message,
        };
        push_diagnostic(
            out,
            json!({
                "code": "structural.pattern.exact",
                "severity": severity,
                "stage": "match",
                "message": message,
                "recovery": recovery,
            }),
        );
        if let Some(tree) = tree {
            out["calls"]["viewSyntax"] = tree;
        }
        place_leads(out, leads.calls());
    }
}

/// The leads an [`ExactMiss`] attaches, in key order. [`Leads::listed`]
/// names exactly these, so a recovery never promises a lead that is absent.
struct Leads {
    /// The census rule, when it counted nodes.
    counted: Option<AstSearchQueryMatch0>,
    /// The rule listing every node of the kind, in the pattern's wrapper.
    every: Option<AstSearchQueryMatch0>,
    /// The rule listing the kind's nodes in any parent (a wrapped pattern).
    any_parent: Option<AstSearchQueryMatch0>,
    /// The `symbols` row listing what the scope declares.
    symbols: Option<Value>,
    /// The summary `view` the rule leads keep ([`asked_view`]).
    view: Option<Value>,
}

impl Leads {
    /// One clause naming each lead, in key order: "the related call lists
    /// A", or "the related calls list A, then B".
    fn listed(&self, kind: &str, parent: &str) -> String {
        let every = if parent.is_empty() {
            "every node of the kind".to_owned()
        } else {
            format!("every {kind} node{parent}")
        };
        let phrases: Vec<String> = [
            self.counted.as_ref().map(|_| "these nodes".to_owned()),
            self.every.as_ref().map(|_| every),
            self.any_parent
                .as_ref()
                .map(|_| "the kind's nodes in any parent".to_owned()),
            self.symbols
                .as_ref()
                .map(|_| "what the scope declares".to_owned()),
        ]
        .into_iter()
        .flatten()
        .collect();
        match phrases.as_slice() {
            [] => "no related call is attached".to_owned(),
            [one] => format!("the related call lists {one}"),
            many => format!("the related calls list {}", many.join(", then ")),
        }
    }

    /// The leads as related calls, in the order [`Self::listed`] names them.
    fn calls(self) -> impl Iterator<Item = Value> {
        use octocode_runtime_base::response::result::Continuation;
        let view = self.view;
        [self.counted, self.every, self.any_parent]
            .into_iter()
            .flatten()
            .map(move |query| lead_call(query, view.as_ref()).build())
            .chain(self.symbols.map(|symbols| {
                Continuation::related_input(ToolId::AstSearch, json!({"queries": [symbols]}))
                    .build()
            }))
    }
}

/// `text` with its first letter upper-cased, to open a sentence.
fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Each lead under the next free numbered `broadenSearch` key, after any
/// lead already on the row (a qualified-spelling note's).
fn place_leads(out: &mut Value, leads: impl IntoIterator<Item = Value>) {
    for lead in leads {
        octocode_runtime_base::response::result::place_numbered(out, "broadenSearch", lead);
    }
}
