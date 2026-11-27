//! The walk flags every local tool shares (`includeIgnored`,
//! `includeHidden`, `defaultExcludes`) and the three causes a path is out of
//! a walk by: what an empty answer names as skipped and what its
//! `includeSkipped` lead lifts ([`crate::leads::skipped`]). Nothing here
//! walks the skipped paths: each tool's own walk records what it left out.

use octocode_engine::error::Error;
use octocode_engine::search::walk::{GlobalIgnore, Prune, walk_builder_under};
use octocode_engine::types::IgnorePolicy;
use serde_json::{Value, json};
use std::path::Path;

/// Why a path is out of a walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    /// A `.gitignore`, `.ignore` or git exclude rule.
    Ignored,
    /// A dot entry, walked only with `includeHidden`.
    Hidden,
    /// A directory (or generated file) the default excludes prune.
    DefaultExcluded,
}

impl Cause {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ignored => "ignored",
            Self::Hidden => "hidden",
            Self::DefaultExcluded => "default-excluded",
        }
    }

    /// The query field that lifts this cause, and its value.
    pub fn flag(self) -> (&'static str, Value) {
        match self {
            Self::Ignored => ("includeIgnored", json!(true)),
            Self::Hidden => ("includeHidden", json!(true)),
            Self::DefaultExcluded => ("defaultExcludes", json!(false)),
        }
    }
}

/// Which paths a walk takes: the request's flags, or a relaxed copy of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Walk {
    /// `includeIgnored`: `.gitignore`d paths are walked.
    pub ignored: bool,
    /// `includeHidden`: dot paths are walked.
    pub hidden: bool,
    /// `defaultExcludes`: the default directory prune applies.
    pub default_excludes: bool,
    /// The global ignore rules of the call the walk serves.
    pub global: GlobalIgnore,
}

impl Walk {
    /// This walk under the global ignore rules of a call.
    pub fn under(self, global: GlobalIgnore) -> Self {
        Self { global, ..self }
    }

    /// The causes this walk skips by.
    pub fn skipping(self) -> Vec<Cause> {
        [
            (!self.ignored, Cause::Ignored),
            (!self.hidden, Cause::Hidden),
            (self.default_excludes, Cause::DefaultExcluded),
        ]
        .into_iter()
        .filter_map(|(skips, cause)| skips.then_some(cause))
        .collect()
    }

    /// The engine ignore policy of this walk.
    pub fn policy(self) -> IgnorePolicy {
        IgnorePolicy {
            include_ignored: self.ignored,
            include_hidden: self.hidden,
        }
    }

    /// The engine walk of this scope that never descends into a `prune`d
    /// directory, rooted as every local walk roots its default prune
    /// ([`Prune::at`]: inside a git work tree only where git ignores it):
    /// ignore rules and hidden entries follow the flags, and `globs` scope
    /// it like the tool's own scan. The globs only narrow: they are applied
    /// to the entries the flags admit, never as walker overrides, which
    /// would admit a hidden or ignored file an include glob matches.
    pub fn walker(
        self,
        root: &Path,
        prune: &[String],
        globs: Globs<'_>,
    ) -> Result<ignore::Walk, Error> {
        let (mut builder, global) = walk_builder_under(root, self.policy(), self.global, None);
        let prune = Prune::new(prune).at_with(root, self.global);
        let globs = globs.overrides(root)?;
        builder.filter_entry(move |entry| {
            let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
            entry.depth() == 0
                || (!global.hides_entry(entry)
                    && !prune.prunes(entry.path(), is_dir)
                    && !globs.matched(entry.path(), is_dir).is_ignore())
        });
        Ok(builder.build())
    }
}

/// A request's `include` and `exclude` globs: the scope its own scan applies,
/// so a probe never reports files the request filtered out.
#[derive(Clone, Copy, Debug, Default)]
pub struct Globs<'a> {
    pub include: &'a [String],
    pub exclude: &'a [String],
}

impl Globs<'_> {
    /// The override set the engine's walks apply, rooted at `root`.
    fn overrides(self, root: &Path) -> Result<ignore::overrides::Override, Error> {
        Ok(octocode_engine::search::glob::compile(root, self.include, self.exclude)?.overrides())
    }
}

/// Sets the lifting flag of each of `causes` on a lead query that spells walk
/// flags like the contract does (`includeIgnored`, `includeHidden`,
/// `defaultExcludes`), and nothing else.
pub(crate) fn lift(query: &mut Value, causes: &[Cause]) {
    if let Some(fields) = query.as_object_mut() {
        for cause in causes {
            let (field, value) = cause.flag();
            fields.insert(field.to_owned(), value);
        }
    }
}
