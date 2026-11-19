//! What an empty directory scan can still offer: the other grammars the
//! scope holds and the skipped walks, each with its repair.
use super::query::MatchQuery;
use super::types::{Scan, Scope};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::policy::prune;
use octocode_runtime_base::response::result::ToolError;
use octocode_tools_shared::ast_rule::{
    GlobParsers, GrammarChoice, choose_grammar, compile_check, present_grammars, rank_grammars,
};
use octocode_tools_shared::empty_walk::{Globs, Walk};
use serde_json::{Value, json};

/// What an empty directory scan can still offer: the other grammars the
/// scope holds, and the paths its walk skipped, each with its repair.
#[derive(Default)]
pub(super) struct ScopeGaps {
    pub(super) repair: Option<Value>,
    pub(super) skipped: Option<SkippedWalk>,
}

/// What the walk skipped (ignore rules, dot entries, default-excluded
/// directories), as every local tool discloses it
/// ([`octocode_tools_shared::leads::skipped`]), with its lead.
pub(super) struct SkippedWalk {
    pub(super) disclosure: octocode_tools_shared::leads::skipped::Disclosure,
    pub(super) lead: Value,
}

impl SkippedWalk {
    /// Place the disclosure and its lead on `out`.
    pub(super) fn attach(self, out: &mut Value) {
        octocode_tools_shared::leads::skipped::attach(out, &self.disclosure, self.lead);
    }
}

impl ScopeGaps {
    /// The gaps of an empty scan: the other grammars when `other_language`
    /// (no file of the requested one was parsed), and the skipped paths.
    pub(super) fn of(
        q: MatchQuery<'_>,
        (scope, scan): (&Scope, &Scan),
        paths: &PathPolicy,
        (states_absence, other_language): (bool, bool),
        cancel: &dyn CancellationCheck,
    ) -> Self {
        Self {
            repair: other_language
                .then(|| other_language_repair(q, scope, scan, paths, cancel))
                .flatten(),
            skipped: skipped_walk(q, scan, states_absence),
        }
    }
}

/// The paths an empty scan's walk skipped, from its own ledger: none of
/// them was read, so absence there is unproven, and the lead lifts exactly
/// the causes that skipped them. Nothing behind them is read, as ripgrep
/// leaves `-u` to its user. The first page of a scan says so, and a later
/// page only when it `states_absence`.
pub(super) fn skipped_walk(
    q: MatchQuery<'_>,
    scan: &Scan,
    states_absence: bool,
) -> Option<SkippedWalk> {
    let later = q.page() > 1 || q.scan_offset() > 0;
    if later && !states_absence {
        return None;
    }
    let disclosure = octocode_tools_shared::leads::skipped::unsearched_ledger(&scan.skipped)?;
    let lead = octocode_tools_shared::leads::skipped::lead(
        ToolId::AstSearch,
        &q.to_value(),
        &disclosure.causes,
    );
    Some(SkippedWalk { disclosure, lead })
}

/// The grammars of a directory scope that occur in it and compile the query,
/// without `parser` in play.
pub(super) fn directory_choice(
    q: MatchQuery<'_>,
    root: &std::path::Path,
    parsers: &GlobParsers,
    scan: &Scan,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<GrammarChoice, ToolError> {
    // The scan's walk took the same rules, so when it met the whole scope (no
    // window, no depth bound) what it counted is the answer.
    let present = match &scan.present {
        Some(present) if !scan.truncated && q.scan_offset() == 0 && q.max_depth().is_none() => {
            rank_grammars(present.clone())
        }
        _ => {
            let (include, exclude) = (
                q.include().unwrap_or_default(),
                q.exclude().unwrap_or_default(),
            );
            let prune = prune::walk_names(q.default_excludes(), &include);
            let discovery = paths.discovery_walk();
            present_grammars(
                root,
                &prune,
                Walk {
                    ignored: q.include_ignored().unwrap_or(false),
                    hidden: q.include_hidden().unwrap_or(false),
                    default_excludes: q.default_excludes(),
                    global: paths.global_ignore(),
                },
                usize::MAX,
                Globs {
                    include: &include,
                    exclude: &exclude,
                },
                parsers,
                &|path, kind| discovery.permits_entry(path, kind),
                &|| cancel.check().is_err(),
            )
            .map_err(super::super::errors::native_error)?
        }
    };
    let rule = q.rule();
    Ok(choose_grammar(present.into_iter(), |_, extensions| {
        compile_check(extensions, None, rule.as_ref())
    }))
}

/// One repair lead that runs the query once per candidate grammar, as a
/// batch within the input limit (the most frequent grammars lead).
pub(super) fn language_repair(q: MatchQuery<'_>, languages: &[String]) -> Value {
    let base =
        octocode_runtime_base::response::result::fresh_query_row(ToolId::AstSearch, &q.to_value());
    let queries = languages
        .iter()
        .take(octocode_runtime_base::response::result::rows_per_call(
            ToolId::AstSearch,
        ))
        .map(|language| {
            let mut row = base.clone();
            row["parser"] = json!(language);
            row
        })
        .collect::<Vec<_>>();
    octocode_runtime_base::response::result::Continuation::related_input(
        ToolId::AstSearch,
        json!({ "queries": queries }),
    )
    .build()
}

/// The repair of a `parser` that parsed no file in a directory: the
/// grammars the scope does hold, when the query compiles in any of them.
pub(super) fn other_language_repair(
    q: MatchQuery<'_>,
    scope: &Scope,
    scan: &Scan,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Option<Value> {
    let requested = q.language()?;
    let languages =
        match directory_choice(q, &scope.canonical, &scope.parsers, scan, paths, cancel).ok()? {
            GrammarChoice::One(language) => vec![language],
            GrammarChoice::Several(languages) => languages,
            GrammarChoice::Invalid | GrammarChoice::Absent => return None,
        }
        .into_iter()
        .filter(|language| *language != requested)
        .collect::<Vec<_>>();
    (!languages.is_empty()).then(|| language_repair(q, &languages))
}
