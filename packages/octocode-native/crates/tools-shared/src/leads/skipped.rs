//! The one disclosure of what a local walk skipped (`localSearch`,
//! `structureSearch`, `astSearch` symbols and match, `graphBuild`): the typed
//! `skippedPaths` record of the paths themselves ([`SkippedEntry`],
//! [`skipped_paths`]), the one sentence an empty answer says of them
//! ([`unsearched`]), and the one `includeSkipped` lead that lifts exactly
//! their causes. Each tool's own walk records what it left out, in the admission
//! ledger (`policy::discovery::WalkAdmission`); nothing behind a skipped
//! path is read to say so.

use crate::empty_walk::{Cause, lift};
use octocode_contract_types::{ErrorCode, SkippedPath, SkippedPathCause, SkippedPaths};
use octocode_engine::num::saturating_i64;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::{Continuation, ToolError, query_row};
use serde_json::Value;

/// One path a walk skipped, or `count` paths behind one cause and rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedEntry {
    /// Absolute; the response spells it workspace-relative.
    pub path: String,
    pub cause: SkippedPathCause,
    /// The rule that skipped it, when the walk names one.
    pub reason: Option<String>,
    pub count: usize,
}

impl SkippedEntry {
    pub fn path(path: impl Into<String>, cause: SkippedPathCause) -> Self {
        Self {
            path: path.into(),
            cause,
            reason: None,
            count: 1,
        }
    }

    pub fn because(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

/// A walk of `root`'s skipped paths of `cause`, root-relative (a
/// directory's trailing `/` dropped).
pub fn under<'a>(
    root: &'a std::path::Path,
    cause: SkippedPathCause,
    relative: impl IntoIterator<Item = &'a String> + 'a,
) -> impl Iterator<Item = SkippedEntry> + 'a {
    relative.into_iter().map(move |path| {
        let path = path.strip_suffix('/').unwrap_or(path);
        SkippedEntry::path(root.join(path).to_string_lossy().into_owned(), cause)
    })
}

/// A walk of `root`'s default-excluded paths, each with the rule that left
/// it out as its reason: a pruned directory by its name (`node_modules/`), a
/// generated file by the pattern it matches (`*.min.js`). Entries group by
/// rule ([`grouped`]), so each rule's paths are counted under the directory
/// holding them.
pub fn default_excluded<'a>(
    root: &'a std::path::Path,
    pruned: impl IntoIterator<Item = &'a String> + 'a,
    generated: impl IntoIterator<Item = &'a String> + 'a,
) -> impl Iterator<Item = SkippedEntry> + 'a {
    let pruned = under(root, SkippedPathCause::DefaultExcluded, pruned).map(|entry| {
        let rule = file_name(&entry.path).map(|name| format!("{name}/"));
        ruled(entry, rule)
    });
    let generated = under(root, SkippedPathCause::DefaultExcluded, generated).map(|entry| {
        let rule = file_name(&entry.path).and_then(|name| {
            octocode_runtime_base::policy::discovery::generated_file_pattern(&name)
        });
        ruled(entry, rule)
    });
    pruned.chain(generated)
}

/// The last component of `path`.
fn file_name(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

/// `entry` with `rule` as its reason, when there is one.
fn ruled(entry: SkippedEntry, rule: Option<String>) -> SkippedEntry {
    match rule {
        Some(rule) => entry.because(rule),
        None => entry,
    }
}

/// The causes a walk skipped paths by (what a lead over them lifts): a dot
/// directory the default prune also names (`.github`, `.cache`), or a dot
/// file the default excludes match, sits behind both.
pub fn skipped_causes(skipped: &octocode_runtime_base::policy::discovery::WalkSkips) -> Vec<Cause> {
    let excluded = skipped.hidden_excluded
        || skipped.ignored_excluded
        || !skipped.generated.is_empty()
        || !skipped.pruned.is_empty();
    [
        (!skipped.ignored.is_empty(), Cause::Ignored),
        (
            !skipped.hidden.is_empty() || skipped.ignored_hidden,
            Cause::Hidden,
        ),
        (excluded, Cause::DefaultExcluded),
    ]
    .into_iter()
    .filter_map(|(hit, cause)| hit.then_some(cause))
    .collect()
}

/// The `skippedPaths` entries of a walk's ledger under `root`: what a flag
/// lifts (ignored, default-excluded and hidden paths, grouped per cause and
/// rule by [`skipped_paths`]), then what no flag lifts (Octocode's store
/// directories, what the path policy withheld, and the walk's read
/// `errors`).
pub fn ledger_entries<'a>(
    root: &'a std::path::Path,
    skipped: &'a octocode_runtime_base::policy::discovery::WalkSkips,
    errors: WalkErrors,
) -> impl Iterator<Item = SkippedEntry> + 'a {
    use SkippedPathCause::{Hidden, Ignored, OctocodeArtifact};
    under(root, Ignored, &skipped.ignored)
        .chain(default_excluded(root, &skipped.pruned, &skipped.generated))
        .chain(under(root, Hidden, &skipped.hidden))
        .chain(under(root, OctocodeArtifact, &skipped.artifacts))
        .chain(unlifted(&root.to_string_lossy(), &skipped.withheld, errors))
}

/// [`unsearched`] of a walk's ledger: the causes it skipped paths by
/// ([`skipped_causes`]).
pub fn unsearched_ledger(
    skipped: &octocode_runtime_base::policy::discovery::WalkSkips,
) -> Option<Disclosure> {
    unsearched(&skipped_causes(skipped))
}

/// What the path policy withheld from a walk of `root`: each policy
/// directory name with its count at the walked scope, and every other
/// withheld entry as the directory that holds it (never its own name), with
/// the rule that withheld it and how many it holds there.
fn withheld(
    root: &str,
    withheld: &octocode_runtime_base::policy::discovery::Withheld,
) -> Vec<SkippedEntry> {
    let dirs = withheld.dirs.iter().map(|(name, count)| SkippedEntry {
        path: root.to_owned(),
        cause: SkippedPathCause::Withheld,
        reason: Some(format!(
            "security-policy dir {}/",
            name.trim_end_matches('/')
        )),
        count: *count,
    });
    let entries = withheld
        .holders()
        .map(|(kind, holder, count)| SkippedEntry {
            path: held_in(root, holder),
            cause: SkippedPathCause::Withheld,
            reason: Some(kind.reason().to_owned()),
            count,
        });
    dirs.chain(entries).collect()
}

/// The directory `holder` as every skipped path is recorded
/// ([`SkippedEntry::path`]): absolute, so the response spells it from the
/// workspace root like the paths beside it. A holder the walk recorded from
/// its root is joined to `root`.
fn held_in(root: &str, holder: &str) -> String {
    let holder = std::path::Path::new(holder);
    if holder.is_absolute() {
        return holder.to_string_lossy().into_owned();
    }
    std::path::Path::new(root)
        .join(holder)
        .to_string_lossy()
        .into_owned()
}

/// What no flag lifts from a walk of `root`: what the path policy withheld
/// ([`withheld`]) and its read errors ([`unreadable`]).
pub fn unlifted(
    root: &str,
    policy: &octocode_runtime_base::policy::discovery::Withheld,
    errors: WalkErrors,
) -> Vec<SkippedEntry> {
    let mut skips = withheld(root, policy);
    skips.extend(unreadable(root, errors));
    skips
}

/// The entries a walk could not read, and how many of them for want of
/// permission.
#[derive(Clone, Copy, Debug, Default)]
pub struct WalkErrors {
    pub(crate) skipped: u32,
    pub(crate) denied: u32,
}

impl From<&octocode_engine::structural::WalkErrors> for WalkErrors {
    fn from(errors: &octocode_engine::structural::WalkErrors) -> Self {
        Self {
            skipped: errors.skipped,
            denied: errors.permission_denied,
        }
    }
}

impl From<(u32, u32)> for WalkErrors {
    fn from((skipped, denied): (u32, u32)) -> Self {
        Self { skipped, denied }
    }
}

/// A walk's read errors under `root`. Absence below them is unproven.
fn unreadable(root: &str, WalkErrors { skipped, denied }: WalkErrors) -> Vec<SkippedEntry> {
    let other = skipped.saturating_sub(denied);
    [(denied, "permission denied"), (other, "read error")]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, reason)| SkippedEntry {
            path: root.to_owned(),
            cause: SkippedPathCause::Unreadable,
            reason: Some(reason.to_owned()),
            count: count as usize,
        })
        .collect()
}

/// One `skippedPaths` entry in words, its path spelled `shown`, saying what
/// its fields say. An entry of several paths names the directory holding
/// them all ([`grouped`]), so it counts what sits under that directory; so
/// does a withheld or unreadable entry, whose path is always the directory
/// holding what was left out (the policy never names a withheld entry). Any
/// other entry of one path names that path.
/// The path and the reason (a rule's text, an error) are a walk's, quoted
/// as judged ([`octocode_runtime_base::security::quoted`]).
pub(crate) fn disclosure(entry: &SkippedPath, shown: &str) -> String {
    let cause = serde_json::to_value(entry.cause)
        .ok()
        .and_then(|cause| cause.as_str().map(str::to_owned))
        .unwrap_or_default();
    let count = entry.count.unwrap_or(1);
    let shown = octocode_runtime_base::security::quoted(shown);
    let quoted_reason = entry
        .reason
        .as_deref()
        .map(octocode_runtime_base::security::quoted);
    let reason = quoted_reason
        .as_ref()
        .map(|reason| format!(" ({reason})"))
        .unwrap_or_default();
    match entry.cause {
        SkippedPathCause::Withheld | SkippedPathCause::Unreadable => {
            format!("{count} {cause} under `{shown}`{reason}")
        }
        _ if count > 1 => format!("{count} {cause} under `{shown}`{reason}"),
        _ => match &quoted_reason {
            Some(reason) => format!("`{shown}` ({cause}: {reason})"),
            None => format!("`{shown}` ({cause})"),
        },
    }
}

/// The `skippedPaths` of `skips` ([`grouped`]); `None` when the walk
/// skipped nothing. A path skipped both as ignored and as default-excluded
/// is one `ignored-default-excluded` entry ([`both_causes`]).
pub fn skipped_paths(
    skips: impl IntoIterator<Item = SkippedEntry>,
    every_path: bool,
    incomplete: bool,
) -> Result<Option<SkippedPaths>, ToolError> {
    let entries = both_causes(skips)
        .into_iter()
        .map(|skip| {
            Ok(SkippedPath {
                path: skip.path.try_into().map_err(skipped_path_error)?,
                cause: skip.cause,
                count: (skip.count > 1).then(|| saturating_i64(skip.count)),
                reason: skip.reason,
            })
        })
        .collect::<Result<Vec<_>, ToolError>>()?;
    Ok(grouped(entries, every_path, incomplete))
}

/// `skips` with each path skipped both as ignored and as default-excluded
/// named once, `ignored-default-excluded`, where the first of its two entries
/// stood: lifting either flag alone does not reach it. The ignore rule, when
/// one is named, is its reason; else the default exclusion's rule
/// (`node_modules/`), whichever entry came first.
fn both_causes(skips: impl IntoIterator<Item = SkippedEntry>) -> Vec<SkippedEntry> {
    use SkippedPathCause::{DefaultExcluded, Ignored, IgnoredDefaultExcluded};
    let mut entries: Vec<SkippedEntry> = Vec::new();
    let mut single = std::collections::HashMap::<(String, SkippedPathCause), usize>::new();
    for skip in skips {
        let other = match skip.cause {
            Ignored => DefaultExcluded,
            DefaultExcluded => Ignored,
            _ => {
                entries.push(skip);
                continue;
            }
        };
        if skip.count == 1
            && let Some(&at) = single.get(&(skip.path.clone(), other))
        {
            let kept = &mut entries[at];
            kept.cause = IgnoredDefaultExcluded;
            let (ignore_rule, exclude_rule) = if skip.cause == Ignored {
                (skip.reason, kept.reason.take())
            } else {
                (kept.reason.take(), skip.reason)
            };
            kept.reason = ignore_rule.or(exclude_rule);
            single.remove(&(kept.path.clone(), other));
            continue;
        }
        if skip.count == 1 {
            single
                .entry((skip.path.clone(), skip.cause))
                .or_insert(entries.len());
        }
        entries.push(skip);
    }
    entries
}

/// `entries` first seen first, one per cause and rule, or with `every_path`
/// each path on its own. A group of several paths is the deepest directory
/// holding them all, with their summed count, which marks the collapse:
/// whatever its cause, the entry's path names where every path it counts
/// sits. `sampled` only when the walk did not finish (`incomplete`): its
/// counts are floors. A result stored with every path is shown either way
/// through this. `None` when empty.
pub fn grouped(
    entries: impl IntoIterator<Item = SkippedPath>,
    every_path: bool,
    incomplete: bool,
) -> Option<SkippedPaths> {
    let mut groups: Vec<SkippedPath> = Vec::new();
    for entry in entries {
        match groups
            .iter_mut()
            .find(|group| group.cause == entry.cause && group.reason == entry.reason)
        {
            Some(group) if !every_path || group.path == entry.path => {
                if group.path != entry.path {
                    let shared = shared_dir(&group.path, &entry.path);
                    if let Ok(shared) = shared.try_into() {
                        group.path = shared;
                    }
                }
                group.count = Some(group.count.unwrap_or(1) + entry.count.unwrap_or(1));
            }
            _ => groups.push(entry),
        }
    }
    (!groups.is_empty()).then_some(SkippedPaths {
        entries: groups,
        sampled: incomplete.then_some(true),
    })
}

/// Skipped entries a scoped disclosure names at most ([`scoped_paths`]): the
/// ones holding the most files; the rest are counted, per cause, at the
/// walk's root.
pub const NAMED_TOP: usize = 8;

/// Files counted behind one named entry at most: past it an entry ranks by
/// walk order among the others past it.
const COUNTED_FILES: usize = 1_000;

/// The `skippedPaths` of a walk of `root`'s ledger ([`ledger_entries`]), each
/// cause and rule grouped per directory at the top of the walk
/// ([`scoped_paths`]). An ignored directory the default prune names too
/// (`node_modules/`) is `ignored-default-excluded`: lifting either flag
/// alone does not reach it.
pub fn scoped_ledger(
    root: &std::path::Path,
    skipped: &octocode_runtime_base::policy::discovery::WalkSkips,
    errors: WalkErrors,
    incomplete: bool,
) -> Result<Option<SkippedPaths>, ToolError> {
    let pruned = octocode_runtime_base::policy::prune::walk_names(true, &[]);
    let pruned_dirs = skipped
        .ignored
        .iter()
        .filter_map(|path| path.strip_suffix('/'))
        .filter(|path| {
            std::path::Path::new(path)
                .file_name()
                .is_some_and(|name| pruned.iter().any(|pruned| name == pruned.as_str()))
        })
        .map(|path| root.join(path).to_string_lossy().into_owned())
        .collect::<std::collections::HashSet<_>>();
    let entries = both_causes(ledger_entries(root, skipped, errors))
        .into_iter()
        .map(|mut entry| {
            if entry.cause == SkippedPathCause::Ignored
                && entry.count == 1
                && pruned_dirs.contains(&entry.path)
            {
                entry.cause = SkippedPathCause::IgnoredDefaultExcluded;
                entry.reason = entry
                    .reason
                    .or_else(|| file_name(&entry.path).map(|name| format!("{name}/")));
            }
            entry
        });
    scoped_paths(root, entries, incomplete)
}

/// The `skippedPaths` of `skips`, a walk of `root`'s, naming where a lead
/// could find a match. Each cause a flag lifts is grouped per directory at
/// the top of the walk (the deepest directory holding that group's paths,
/// with their summed count; a rule shared by every path of a group is its
/// reason): grouped into the one directory holding them all, a cause spread
/// across the walk would name only its root. Of those groups, only ones
/// holding a file can hold a match: at most [`NAMED_TOP`] are named, those
/// holding the most files first (counted up to [`COUNTED_FILES`] each, links
/// not followed), and every other skipped path of a cause is counted in one
/// entry at the root. What no flag lifts (withheld, unreadable, Octocode's
/// own store) is grouped per cause and rule as [`skipped_paths`] does. A path
/// skipped both as ignored and as default-excluded is one entry
/// ([`both_causes`]). `sampled` when the walk did not finish; `None` when it
/// skipped nothing.
pub fn scoped_paths(
    root: &std::path::Path,
    skips: impl IntoIterator<Item = SkippedEntry>,
    incomplete: bool,
) -> Result<Option<SkippedPaths>, ToolError> {
    use SkippedPathCause::{DefaultExcluded, Hidden, Ignored, IgnoredDefaultExcluded};
    struct Group {
        top: Option<std::ffi::OsString>,
        entry: SkippedEntry,
        members: Vec<String>,
    }
    let top = |path: &str| {
        std::path::Path::new(path)
            .strip_prefix(root)
            .ok()
            .and_then(|below| below.components().next())
            .map(|first| first.as_os_str().to_owned())
    };
    let (mut lifted, mut kept): (Vec<Group>, Vec<SkippedEntry>) = (Vec::new(), Vec::new());
    for skip in both_causes(skips) {
        if !matches!(
            skip.cause,
            Ignored | Hidden | DefaultExcluded | IgnoredDefaultExcluded
        ) {
            kept.push(skip);
            continue;
        }
        let at = top(&skip.path);
        match lifted
            .iter_mut()
            .find(|group| group.top == at && group.entry.cause == skip.cause)
        {
            Some(group) => {
                if group.entry.path != skip.path {
                    group.entry.path = shared_dir(&group.entry.path, &skip.path);
                }
                if group.entry.reason != skip.reason {
                    group.entry.reason = None;
                }
                group.entry.count += skip.count;
                group.members.push(skip.path);
            }
            None => lifted.push(Group {
                top: at,
                members: vec![skip.path.clone()],
                entry: skip,
            }),
        }
    }
    // The groups that hold a file, the fullest first: those are named.
    let mut ranked = lifted
        .iter()
        .enumerate()
        .filter(|(_, group)| group.top.is_some())
        .map(|(index, group)| (index, counted_files(&group.members)))
        .filter(|(_, files)| *files > 0)
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let named = ranked
        .iter()
        .take(NAMED_TOP)
        .map(|(index, _)| *index)
        .collect::<Vec<_>>();
    let mut entries: Vec<SkippedEntry> = named
        .iter()
        .map(|index| lifted[*index].entry.clone())
        .collect();
    // Every other path of a cause: one counted entry at the root.
    let mut rest: Vec<SkippedEntry> = Vec::new();
    for (index, group) in lifted.into_iter().enumerate() {
        if named.contains(&index) {
            continue;
        }
        match rest.iter_mut().find(|held| held.cause == group.entry.cause) {
            Some(held) => {
                held.count += group.entry.count;
                if held.reason != group.entry.reason {
                    held.reason = None;
                }
            }
            None => rest.push(SkippedEntry {
                path: root.to_string_lossy().into_owned(),
                ..group.entry
            }),
        }
    }
    entries.extend(rest);
    let mut shown = entries
        .into_iter()
        .map(|skip| {
            Ok(SkippedPath {
                path: skip.path.try_into().map_err(skipped_path_error)?,
                cause: skip.cause,
                count: (skip.count > 1).then(|| saturating_i64(skip.count)),
                reason: skip.reason,
            })
        })
        .collect::<Result<Vec<_>, ToolError>>()?;
    if let Some(unlifted) = skipped_paths(kept, false, false)? {
        shown.extend(unlifted.entries);
    }
    Ok((!shown.is_empty()).then_some(SkippedPaths {
        entries: shown,
        sampled: incomplete.then_some(true),
    }))
}

/// The files under `paths` (each a skipped file or directory), counted up
/// to [`COUNTED_FILES`]; links are not followed and an unreadable directory
/// holds none that can be read.
fn counted_files(paths: &[String]) -> usize {
    let mut files = 0;
    let mut dirs = Vec::new();
    for path in paths {
        match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.is_dir() => dirs.push(std::path::PathBuf::from(path)),
            Ok(meta) if meta.is_file() => files += 1,
            _ => {}
        }
    }
    while files < COUNTED_FILES
        && let Some(dir) = dirs.pop()
    {
        let Ok(listing) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in listing.flatten() {
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => dirs.push(entry.path()),
                Ok(kind) if kind.is_file() => files += 1,
                _ => {}
            }
            if files >= COUNTED_FILES {
                break;
            }
        }
    }
    files.min(COUNTED_FILES)
}

/// Unparsed files named one by one up to this many; past it they group by
/// type (and directory), as every other cause does, so a scope of thousands
/// of data files states a few counted entries, not one per file.
pub const NAMED_UNPARSED: usize = 20;

/// The `skippedPaths` of a walk's unparsed files ([`skipped_paths`]): each
/// named while there are at most [`NAMED_UNPARSED`], else grouped.
pub fn unparsed_paths(
    entries: Vec<SkippedEntry>,
    incomplete: bool,
) -> Result<Option<SkippedPaths>, ToolError> {
    let every_path = entries.len() <= NAMED_UNPARSED;
    skipped_paths(entries, every_path, incomplete)
}

/// `first`'s entries followed by `then`'s, as one `skippedPaths`: causes
/// grouped by rule beside causes listed path by path ([`skipped_paths`]).
/// `sampled` when either walk did not finish.
pub fn appended(first: Option<SkippedPaths>, then: Option<SkippedPaths>) -> Option<SkippedPaths> {
    match (first, then) {
        (Some(mut first), Some(then)) => {
            first.entries.extend(then.entries);
            first.sampled =
                (first.sampled == Some(true) || then.sampled == Some(true)).then_some(true);
            Some(first)
        }
        (first, then) => first.or(then),
    }
}

/// The deepest directory holding both `a` and `b`.
fn shared_dir(a: &str, b: &str) -> String {
    std::path::Path::new(a)
        .components()
        .zip(std::path::Path::new(b).components())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a)
        .collect::<std::path::PathBuf>()
        .to_string_lossy()
        .into_owned()
}

pub fn skipped_path_error(error: impl std::fmt::Display) -> ToolError {
    ToolError::new(
        ErrorCode::OutputContractViolation,
        format!("Invalid skipped-path evidence: {error}"),
    )
}

/// What an empty answer says of the paths its walk skipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disclosure {
    /// The causes the lead lifts.
    pub causes: Vec<Cause>,
    /// The sentence: the skipped paths were not searched.
    pub text: String,
}

/// An empty answer's word on the paths its walk skipped by `causes`: the
/// row's `skippedPaths` names and counts them, so the sentence states only
/// that they were not searched and what the one `related[]` request does
/// (the same request with exactly the flags that lift `causes`, [`lead`]),
/// which walks everything those flags hide. Nothing behind them is read, as
/// ripgrep leaves `-u` to its user. `None` when the walk skipped nothing.
pub fn unsearched(causes: &[Cause]) -> Option<Disclosure> {
    let flags = causes
        .iter()
        .map(|cause| {
            let (field, value) = cause.flag();
            format!("{field}:{value}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    (!causes.is_empty()).then(|| Disclosure {
        causes: causes.to_vec(),
        text: format!(
            "skippedPaths were not searched; the related[] request searches them too ({flags}), which can take far longer."
        ),
    })
}

/// The `includeSkipped` lead: `query` as a fresh first call (no cursor
/// field) with exactly `causes` lifted.
pub fn lead(tool: ToolId, query: &impl serde::Serialize, causes: &[Cause]) -> Value {
    let mut row = query_row(
        query,
        &octocode_runtime_base::contracts::state_fields(tool)
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    lift(&mut row, causes);
    Continuation::related(tool, row).build()
}

/// Place `disclosure` on an internal result whose `status` is settled, by
/// the one rule of every tool: an empty row carries the sentence as its
/// hint beside the call (its recovery); a row with evidence carries it as a
/// warning (it limits what the evidence proves absent), with the call
/// marked as one the row needs. The requested scope was walked whole, so
/// the skipped paths leave it complete.
pub fn attach(out: &mut Value, disclosure: &Disclosure, lead: Value) {
    if out["status"] == "empty" {
        out["calls"]["includeSkipped"] = lead;
        octocode_runtime_base::response::result::add_hints(out, [disclosure.text.as_str()]);
    } else {
        out["calls"]["includeSkipped"] =
            octocode_runtime_base::response::result::required_call(lead);
        octocode_runtime_base::response::paging::push_warnings(out, [disclosure.text.clone()]);
    }
}

#[cfg(test)]
mod tests;
