use crate::policy::prune;
use octocode_engine::search::walk::{IgnoreRules, Prune};

use std::path::{Path, PathBuf};

use super::{Cause, Skip};

/// Why the walk skipped `file`, by the first of `causes` that applies;
/// paths in the reason spelled by `shown`.
pub(super) fn skip_of(
    root: &Path,
    rules: &IgnoreRules,
    file: &Path,
    causes: &[Cause],
    shown: &dyn Fn(&Path) -> String,
) -> Option<Skip> {
    [Cause::Ignored, Cause::Hidden, Cause::DefaultExcluded]
        .into_iter()
        .filter(|cause| causes.contains(cause))
        .find_map(|cause| {
            let (entry, reason) = match cause {
                Cause::Ignored => ignored_entry(rules, file, shown),
                Cause::Hidden => hidden_entry(root, file, shown),
                Cause::DefaultExcluded => excluded_entry(root, file, shown),
            }?;
            Some(Skip {
                cause,
                entry,
                reason,
            })
        })
}

/// The path's ancestors below `root` and itself, shortest first, each with
/// whether it is a directory on the way to `file`.
pub(super) fn prefixes(root: &Path, file: &Path) -> Vec<(PathBuf, bool)> {
    let Ok(relative) = file.strip_prefix(root) else {
        return Vec::new();
    };
    let count = relative.components().count();
    let mut prefix = root.to_path_buf();
    relative
        .components()
        .enumerate()
        .map(|(index, part)| {
            prefix.push(part);
            (prefix.clone(), index + 1 < count)
        })
        .collect()
}

/// The topmost ignored entry on the way to `file`, and its rule.
fn ignored_entry(
    rules: &IgnoreRules,
    file: &Path,
    shown: &dyn Fn(&Path) -> String,
) -> Option<(PathBuf, String)> {
    let reason = rules.why(file, false)?;
    let entry = reason.entry.clone();
    Some((entry, spell(&reason, shown)))
}

fn hidden_entry(
    root: &Path,
    file: &Path,
    shown: &dyn Fn(&Path) -> String,
) -> Option<(PathBuf, String)> {
    prefixes(root, file)
        .into_iter()
        .find_map(|(prefix, is_dir)| {
            let name = prefix.file_name()?.to_string_lossy();
            let slash = if is_dir { "/" } else { "" };
            name.starts_with('.')
                .then(|| (prefix.clone(), format!("hidden {}{slash}", shown(&prefix))))
        })
}

/// The first pruned directory on the way to `file`, else the file itself
/// when it is a generated one the default excludes skip (`*.min.js`).
fn excluded_entry(
    root: &Path,
    file: &Path,
    shown: &dyn Fn(&Path) -> String,
) -> Option<(PathBuf, String)> {
    let prune = Prune::new(prune::directories(true));
    let directory = prefixes(root, file)
        .into_iter()
        .filter(|(prefix, is_dir)| {
            *is_dir && prefix.file_name().is_some_and(|name| prune.names(name))
        })
        .map(|(prefix, _)| {
            let reason = format!("{}/", shown(&prefix));
            (prefix, reason)
        })
        .next();
    directory.or_else(|| {
        let name = file.file_name()?.to_string_lossy();
        let pattern = crate::policy::discovery::generated_file_pattern(&name)?;
        Some((file.to_path_buf(), pattern))
    })
}

/// The ignore rule that keeps `path` out of a walk under `root`: the deciding
/// ignore file, line and pattern, the file relative to `root` (through `../`
/// for one above it). `None` when no rule applies.
pub(crate) fn ignore_rule(root: &Path, path: &Path, is_dir: bool) -> Option<String> {
    rule_shown(&IgnoreRules::new(root), path, is_dir, &|path| {
        below(root, path)
    })
}

/// [`ignore_rule`] under prepared rules, the ignore file named by `shown`.
fn rule_shown(
    rules: &IgnoreRules,
    path: &Path,
    is_dir: bool,
    shown: &dyn Fn(&Path) -> String,
) -> Option<String> {
    rules.why(path, is_dir).map(|reason| spell(&reason, shown))
}

/// `file:line 'pattern'`, the file named by `shown`.
fn spell(
    reason: &octocode_engine::search::walk::IgnoreReason,
    shown: &dyn Fn(&Path) -> String,
) -> String {
    let line = reason
        .line
        .map(|line| format!(":{line}"))
        .unwrap_or_default();
    let source = reason.source.as_deref().map(shown).unwrap_or_default();
    format!("{source}{line} '{}'", reason.pattern)
}

/// `path` relative to `root`, through `../` for a path above it: a row
/// never prints an absolute path.
pub(super) fn below(root: &Path, path: &Path) -> String {
    root.ancestors()
        .enumerate()
        .find_map(|(up, base)| {
            let rest = path.strip_prefix(base).ok()?;
            Some("../".repeat(up) + &octocode_engine::text::slash_path(rest))
        })
        .unwrap_or_else(|| octocode_engine::text::slash_path(path))
}
