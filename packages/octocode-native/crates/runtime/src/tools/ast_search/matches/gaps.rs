//! What a directory scan can still offer beside its answer: the other
//! grammars the scope holds and the skipped walks, each with its repair.
use super::query::MatchQuery;
use super::types::Scope;
use crate::execution::cancel::CancellationCheck;
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use crate::policy::prune;
use crate::response::result::ToolError;
use crate::tools::ast_rule::{
    GlobParsers, GrammarChoice, choose_grammar, compile_check, has_extension_in, present_grammars,
};
use crate::tools::empty_walk::{FileProbe, Globs, Walk, diagnose};
use serde_json::{Value, json};

/// What a directory scan can still offer beside its answer: the other
/// grammars the scope holds (an empty scan only), and the walk flags that
/// were off while files of the requested grammar sat behind them.
#[derive(Default)]
pub(super) struct ScopeGaps {
    pub(super) repair: Option<Value>,
    pub(super) skipped: Option<SkippedWalk>,
}

/// What the skipped walks hold of the requested grammar, as every local
/// tool discloses it ([`crate::tools::leads::skipped`]), with its lead.
pub(super) struct SkippedWalk {
    pub(super) disclosure: crate::tools::leads::skipped::Disclosure,
    pub(super) lead: Value,
}

impl SkippedWalk {
    /// Place the disclosure and its lead on `out`.
    pub(super) fn attach(self, out: &mut Value) {
        crate::tools::leads::skipped::attach(out, &self.disclosure, self.lead);
    }
}

impl ScopeGaps {
    pub(super) fn of(
        q: MatchQuery<'_>,
        scope: &Scope,
        paths: &PathPolicy,
        cancel: &dyn CancellationCheck,
    ) -> Self {
        Self {
            repair: other_language_repair(q, scope, paths, cancel),
            skipped: skipped_walk(q, scope, paths, cancel),
        }
    }

    /// A scan with no other language to offer (it found matches, it parsed
    /// files, or no `parser` was set): only the skipped walks, no repair.
    pub(super) fn beside_parsed(
        q: MatchQuery<'_>,
        scope: &Scope,
        paths: &PathPolicy,
        cancel: &dyn CancellationCheck,
    ) -> Self {
        Self {
            repair: None,
            skipped: skipped_walk(q, scope, paths, cancel),
        }
    }
}

/// Asks which of the walks the request left off (ignore files, hidden
/// entries, default-excluded directories) hold files of the requested
/// grammar; the lead lifts exactly those.
pub(super) fn skipped_walk(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Option<SkippedWalk> {
    let extensions = scope.extensions.as_ref()?;
    let walk = Walk {
        ignored: q.include_ignored().unwrap_or(false),
        hidden: q.include_hidden().unwrap_or(false),
        default_excludes: q.default_excludes(),
    };
    let budget = usize::MAX;
    let (include, exclude) = (
        q.include().unwrap_or_default(),
        q.exclude().unwrap_or_default(),
    );
    let discovery = paths.discovery_walk();
    let permits = |path: &std::path::Path| discovery.permits(path);
    let stop = || cancel.check().is_err();
    let keep = |path: &std::path::Path| {
        has_extension_in(path, extensions) || scope.parsers.extension(path).is_some()
    };
    let probe = FileProbe::new(&scope.canonical, budget, &permits, &keep)
        .within(Globs {
            include: &include,
            exclude: &exclude,
        })
        .stopping(&stop)
        .beyond(walk);
    let verdict = diagnose(&scope.canonical, walk, |relaxed| probe.find(relaxed)).ok()?;
    let language = scope.language.as_deref().or(scope.inferred.as_deref());
    let subject = language.map_or_else(
        || ["file".to_owned(), "files".to_owned()],
        |name| [format!("{name} file"), format!("{name} files")],
    );
    // A check cut before it found anything proved nothing: every cause the
    // walk skipped by is unproven.
    let skipping = walk.skipping();
    let named = skipping
        .iter()
        .map(|cause| cause.label())
        .collect::<Vec<_>>()
        .join(", ");
    let disclosure = crate::tools::leads::skipped::disclose(
        &verdict,
        [subject[0].as_str(), subject[1].as_str()],
        crate::tools::leads::skipped::Unchecked {
            causes: &skipping,
            named: &named,
        },
    )?;
    let lead =
        crate::tools::leads::skipped::lead(ToolId::AstSearch, &q.to_value(), &disclosure.causes);
    Some(SkippedWalk { disclosure, lead })
}

/// The grammars of a directory scope that occur in it and compile the query,
/// without `parser` in play.
pub(super) fn directory_choice(
    q: MatchQuery<'_>,
    root: &std::path::Path,
    parsers: &GlobParsers,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<GrammarChoice, ToolError> {
    let (include, exclude) = (
        q.include().unwrap_or_default(),
        q.exclude().unwrap_or_default(),
    );
    let prune = prune::walk_names(q.default_excludes(), &include);
    let discovery = paths.discovery_walk();
    let present = present_grammars(
        root,
        &prune,
        Walk {
            ignored: q.include_ignored().unwrap_or(false),
            hidden: q.include_hidden().unwrap_or(false),
            default_excludes: q.default_excludes(),
        },
        usize::MAX,
        Globs {
            include: &include,
            exclude: &exclude,
        },
        parsers,
        &|path| discovery.permits(path),
        &|| cancel.check().is_err(),
    )
    .map_err(super::super::errors::native_error)?;
    let rule = q.rule();
    Ok(choose_grammar(present.into_iter(), |_, extensions| {
        compile_check(extensions, None, rule.as_ref())
    }))
}

/// One repair lead that runs the query once per candidate grammar, as a
/// batch within the input limit (the most frequent grammars lead).
pub(super) fn language_repair(q: MatchQuery<'_>, languages: &[String]) -> Value {
    let base = crate::response::result::fresh_query_row(ToolId::AstSearch, &q.to_value());
    let queries = languages
        .iter()
        .take(crate::response::result::rows_per_call(ToolId::AstSearch))
        .map(|language| {
            let mut row = base.clone();
            row["parser"] = json!(language);
            row
        })
        .collect::<Vec<_>>();
    crate::response::result::Continuation::related_input(
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
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Option<Value> {
    let requested = q.language()?;
    let languages =
        match directory_choice(q, &scope.canonical, &scope.parsers, paths, cancel).ok()? {
            GrammarChoice::One(language) => vec![language],
            GrammarChoice::Several(languages) => languages,
            GrammarChoice::Invalid | GrammarChoice::Absent => return None,
        }
        .into_iter()
        .filter(|language| *language != requested)
        .collect::<Vec<_>>();
    (!languages.is_empty()).then(|| language_repair(q, &languages))
}
