//! The walker a search runs (overrides, types, depth) and the ignore and dot-entry rule it applies
//! itself.

use crate::error::{Error, ErrorKind, Result};
use crate::search::walk::{DirRules, IgnoreRules, ListedRules, Probe, walk_builder};
use crate::types::IgnorePolicy;
use crate::types::{SearchSort, TextSearchOptions};
use ignore::WalkBuilder;
use ignore::overrides::Override;
use ignore::types::{Types, TypesBuilder};
use std::path::Path;
use std::time::SystemTime;

/// Build the walker with `-g` overrides and `-t` types, and the rule the
/// walk applies itself for ignored and dot entries (see [`EntryRule`]).
/// Results are sorted after parallel traversal to reproduce
/// `--sort`/`--sortr` deterministically.
pub(super) fn build_walk_builder(
    opts: &TextSearchOptions,
) -> Result<(WalkBuilder, Option<EntryRule>, Selection)> {
    let mut wb = walk_builder(
        Path::new(&opts.path),
        IgnorePolicy::ALL,
        // The public tool contract counts files in the root as maxDepth 0.
        opts.max_depth.map(|max_depth| max_depth as usize + 1),
    );

    let mut selection = Selection::default();
    let rule = EntryRule {
        hidden: !opts.ignore.include_hidden,
        ignored: (!opts.ignore.include_ignored)
            .then(|| IgnoreRules::with_global(Path::new(&opts.path), opts.global_ignore)),
    };
    if let Some(lang) = opts.lang_type.as_deref().filter(|l| !l.is_empty()) {
        let mut tb = TypesBuilder::new();
        tb.add_defaults();
        tb.select(lang);
        let types = tb
            .build()
            .map_err(|error| Error::with_kind(ErrorKind::InvalidInput, error.to_string()))?;
        wb.types(types.clone());
        selection.types = Some(types);
    }

    let has_globs = opts.include.as_ref().is_some_and(|v| !v.is_empty())
        || opts.exclude.as_ref().is_some_and(|v| !v.is_empty());
    if has_globs {
        let overrides = crate::search::glob::compile(
            Path::new(&opts.path),
            opts.include.as_deref().unwrap_or_default(),
            opts.exclude.as_deref().unwrap_or_default(),
        )?
        .overrides();
        wb.overrides(overrides.clone());
        selection.overrides = Some(overrides);
    }

    Ok((
        wb,
        (rule.hidden || rule.ignored.is_some()).then_some(rule),
        selection,
    ))
}

/// What the `ignore` walker drops inside itself, apart from the ignore and
/// dot rules the search applies: the include and exclude globs, and the
/// `language` type. A walk of its own applies it the way the walker does, so
/// what it drops is silent, as it is there.
#[derive(Clone, Default)]
pub(super) struct Selection {
    overrides: Option<Override>,
    types: Option<Types>,
}

impl Selection {
    /// Whether the walker drops the entry `path` before the search sees it:
    /// a glob that leaves it out, or, when no glob selects it, a type that
    /// does not hold it.
    pub(super) fn prunes(&self, path: &Path, is_dir: bool) -> bool {
        if let Some(overrides) = self.overrides.as_ref().filter(|o| !o.is_empty()) {
            let matched = overrides.matched(path, is_dir);
            if matched.is_ignore() {
                return true;
            }
            if matched.is_whitelist() {
                return false;
            }
        }
        self.types
            .as_ref()
            .filter(|types| !types.is_empty())
            .is_some_and(|types| types.matched(path, is_dir).is_ignore())
    }
}

/// Why the walk leaves an entry out on its own (see [`EntryRule`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Skipped {
    Ignored,
    Hidden,
}

/// The ignore rules and the dot-entry rule, applied by the search instead
/// of inside the `ignore` walker (which walks everything its overrides and
/// types admit), so every entry either rule leaves out is reported to the
/// path filter instead of vanishing inside the walker.
///
/// An include glob or a `language` type only narrows ([`Selection`]): it
/// never admits an entry these rules leave out, so a dot file needs
/// `includeHidden` and an ignored one `includeIgnored`, as on
/// structureSearch (unlike ripgrep's `-g` and `-t`). An ignore rule decides
/// first ([`IgnoreRules`], the walker's rules for one path); else an
/// ignore-file `!` re-include admits the entry; else a dot entry below the
/// root (on Windows also one with the hidden attribute) is hidden.
pub(crate) struct EntryRule {
    /// Dot entries are left out (no `includeHidden`).
    pub(super) hidden: bool,
    /// The ignore rules; `None` with `includeIgnored`.
    ignored: Option<IgnoreRules>,
}

impl EntryRule {
    /// The rule of a walk of `root` under `policy` and `global`; `None`
    /// when the policy lifts both the ignore and the dot rule.
    pub(crate) fn of(
        root: &Path,
        policy: IgnorePolicy,
        global: crate::search::walk::GlobalIgnore,
    ) -> Option<Self> {
        let rule = Self {
            hidden: !policy.include_hidden,
            ignored: (!policy.include_ignored).then(|| IgnoreRules::with_global(root, global)),
        };
        (rule.hidden || rule.ignored.is_some()).then_some(rule)
    }

    /// The ignore files the walk met that exist but could not be read: they
    /// applied no rules, so the entries they would leave out were searched.
    pub(super) fn unreadable_ignore_files(&self) -> Vec<crate::search::walk::UnreadableRule> {
        self.ignored
            .as_ref()
            .map(IgnoreRules::unreadable_rule_files)
            .unwrap_or_default()
    }

    /// The rules of the directory `dir` a walk enters, from what its
    /// listing shows of its rule files; `None` with `includeIgnored`.
    pub(super) fn enter(&self, dir: &Path, listed: Option<ListedRules>) -> Option<DirRules> {
        self.ignored.as_ref().map(|rules| rules.enter(dir, listed))
    }

    /// Whether the rules leave `entry` out, and which. With `rules` (its
    /// directory's, [`Self::enter`]) the decision has no cache lookup; both
    /// decide the same, and neither has an effect.
    pub(crate) fn skips(
        &self,
        rules: Option<&DirRules>,
        entry: Probe<'_>,
        is_dir: bool,
    ) -> Option<Skipped> {
        if entry.depth == 0 {
            return None;
        }
        let path = entry.path;
        let decided = self
            .ignored
            .as_ref()
            .map_or(ignore::Match::None, |ignored| match rules {
                Some(rules) => ignored.decide_in(rules, path, is_dir),
                None => ignored.decide_entry(path, is_dir),
            });
        if decided.is_ignore() {
            return Some(Skipped::Ignored);
        }
        (self.hidden && !decided.is_whitelist() && is_hidden(path)).then_some(Skipped::Hidden)
    }
}

fn is_hidden(path: &Path) -> bool {
    if path
        .file_name()
        .is_some_and(|name| name.as_encoded_bytes().first() == Some(&b'.'))
    {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        if std::fs::symlink_metadata(path)
            .is_ok_and(|meta| meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
        {
            return true;
        }
    }
    false
}

pub(super) fn capture_sort_time(opts: &TextSearchOptions, path: &Path) -> Option<SystemTime> {
    match opts.sort {
        SearchSort::Modified => std::fs::symlink_metadata(path).ok()?.modified().ok(),
        SearchSort::Accessed => std::fs::symlink_metadata(path).ok()?.accessed().ok(),
        SearchSort::Created => std::fs::symlink_metadata(path).ok()?.created().ok(),
        SearchSort::Path | SearchSort::Traversal | SearchSort::MatchCount => None,
    }
}
