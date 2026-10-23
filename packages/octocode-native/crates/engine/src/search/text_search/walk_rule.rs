//! The walker a search runs (overrides, types, depth) and the ignore and dot-entry rule it applies
//! itself.

use crate::error::{Error, ErrorKind, Result};
use crate::search::walk::{IgnorePolicy, IgnoreRules, walk_builder};
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
) -> Result<(WalkBuilder, Option<EntryRule>)> {
    let mut wb = walk_builder(
        Path::new(&opts.path),
        IgnorePolicy::ALL,
        // The public tool contract counts files in the root as maxDepth 0.
        opts.max_depth.map(|max_depth| max_depth as usize + 1),
    );

    let mut rule = EntryRule {
        overrides: None,
        types: None,
        hidden: !opts.ignore.include_hidden,
        ignored: (!opts.ignore.include_ignored).then(|| IgnoreRules::new(Path::new(&opts.path))),
    };
    if let Some(lang) = opts.lang_type.as_deref().filter(|l| !l.is_empty()) {
        let mut tb = TypesBuilder::new();
        tb.add_defaults();
        tb.select(lang);
        let types = tb
            .build()
            .map_err(|error| Error::with_kind(ErrorKind::InvalidInput, error.to_string()))?;
        wb.types(types.clone());
        rule.types = Some(types);
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
        rule.overrides = Some(overrides);
    }

    Ok((wb, (rule.hidden || rule.ignored.is_some()).then_some(rule)))
}

/// Why the walk leaves an entry out on its own (see [`EntryRule`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Skipped {
    Ignored,
    Hidden,
}

/// The ignore rules and the dot-entry rule, applied by the search instead
/// of inside the `ignore` walker (which walks everything its overrides and
/// types admit), so every entry either rule leaves out is reported to the
/// path filter instead of vanishing inside the walker. The precedence is the
/// walker's own: an include glob selecting the entry admits it; else an
/// ignore rule decides it ([`IgnoreRules`], the walker's rules for one
/// path); else a `language` type or an ignore-file `!` re-include selecting
/// it admits it; else a dot entry below the root (on Windows also one with
/// the hidden attribute) is hidden.
pub(super) struct EntryRule {
    pub(super) overrides: Option<Override>,
    pub(super) types: Option<Types>,
    /// Dot entries are left out (no `includeHidden`).
    pub(super) hidden: bool,
    /// The ignore rules; `None` with `includeIgnored`.
    ignored: Option<IgnoreRules>,
}

impl EntryRule {
    pub(super) fn skips(&self, dent: &ignore::DirEntry, is_dir: bool) -> Option<Skipped> {
        if dent.depth() == 0 {
            return None;
        }
        let path = dent.path();
        if self
            .overrides
            .as_ref()
            .is_some_and(|overrides| overrides.matched(path, is_dir).is_whitelist())
        {
            return None;
        }
        let decided = self.ignored.as_ref().map_or(ignore::Match::None, |rules| {
            rules.decide_entry(path, is_dir)
        });
        if decided.is_ignore() {
            return Some(Skipped::Ignored);
        }
        let selected = decided.is_whitelist()
            || self
                .types
                .as_ref()
                .is_some_and(|types| types.matched(path, is_dir).is_whitelist());
        (self.hidden && !selected && is_hidden(dent)).then_some(Skipped::Hidden)
    }
}

fn is_hidden(dent: &ignore::DirEntry) -> bool {
    if dent.file_name().as_encoded_bytes().first() == Some(&b'.') {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        if dent
            .metadata()
            .is_ok_and(|meta| meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
        {
            return true;
        }
    }
    false
}

pub(super) fn capture_sort_time(
    opts: &TextSearchOptions,
    entry: &ignore::DirEntry,
) -> Option<SystemTime> {
    match opts.sort {
        SearchSort::Modified => entry.metadata().ok()?.modified().ok(),
        SearchSort::Accessed => entry.metadata().ok()?.accessed().ok(),
        SearchSort::Created => entry.metadata().ok()?.created().ok(),
        SearchSort::Path
        | SearchSort::Traversal
        | SearchSort::Relevance
        | SearchSort::MatchCount => None,
    }
}
