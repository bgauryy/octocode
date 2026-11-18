//! The one disclosure of what a local walk skipped (`localSearch`,
//! `structureSearch`, `astSearch` symbols and match): the typed
//! `skippedPaths` record of the paths themselves ([`SkippedEntry`],
//! [`skipped_paths`]), the sentence that says what sits behind them, the
//! `includeSkipped` lead that lifts exactly those causes, and the partial
//! policy when the check of them spent its work budget. Each tool supplies only its
//! subject (what it looks for) and its query; the diagnosis is `empty_walk`'s,
//! fed by the walk's admission ledger (`policy::discovery::WalkAdmission`).

use crate::empty_walk::{Cause, Verdict, lift};
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

/// A walk's skip cause as `skippedPaths` names it.
pub fn skipped_cause(cause: Cause) -> SkippedPathCause {
    match cause {
        Cause::Ignored => SkippedPathCause::Ignored,
        Cause::Hidden => SkippedPathCause::Hidden,
        Cause::DefaultExcluded => SkippedPathCause::DefaultExcluded,
    }
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

/// What one walk's skipped paths hold, as every local tool states it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disclosure {
    /// The causes the lead lifts.
    pub causes: Vec<Cause>,
    /// The sentence: what the check found, or that it was cut.
    pub text: String,
    /// The check spent its work budget first, so the skipped paths stay
    /// unsearched.
    pub unchecked: bool,
}

/// What the walk skipped, for a check cut before it found anything: the
/// causes it skipped paths by (what the lead lifts) and how the sentence
/// names them (`2 ignored, 1 hidden`, `node_modules/`).
#[derive(Clone, Copy, Debug)]
pub struct Unchecked<'a> {
    pub causes: &'a [Cause],
    pub named: &'a str,
}

/// What a lead built from a cut probe costs: the probe proved a prefix of
/// the skipped paths at most, so the lead lifts the causes for the whole
/// scope (narrowing to the sampled entries would drop matches past them).
/// The sentence names the lead as the agent finds it: the `related[]`
/// request that sets exactly the flags lifting `causes` ([`lead`]).
pub fn walks_all_skipped(causes: &[Cause]) -> String {
    let flags = causes
        .iter()
        .map(|cause| {
            let (field, value) = cause.flag();
            format!("{field}:{value}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "The related[] request with {flags} walks everything those causes hide, not only these files, and can take far longer than this search."
    )
}

/// The disclosure of `verdict` about what `subject` (the noun phrase for
/// one and for several: `["entry", "entries"]`, `["file that matches",
/// "files that match"]`) the skipped paths hold; `unchecked` is what a cut
/// check leaves unproven. `None` when they hold nothing, or the check was
/// cut and the walk skipped nothing.
pub fn disclose(
    verdict: &Verdict,
    subject: [&str; 2],
    unchecked: Unchecked<'_>,
) -> Option<Disclosure> {
    match verdict {
        Verdict::Absent => None,
        Verdict::Skipped(diagnosis) => {
            let found = diagnosis.found();
            let say = |why: &str| {
                format!(
                    "{}{} {} behind skipped paths ({why}).",
                    if found.cut { "At least " } else { "" },
                    found.count,
                    subject[usize::from(found.count != 1)],
                )
            };
            let mut text = say(&diagnosis.because());
            if found.cut {
                text = format!("{text} {}", walks_all_skipped(diagnosis.causes()));
            }
            Some(Disclosure {
                causes: diagnosis.causes().to_vec(),
                text,
                unchecked: false,
            })
        }
        Verdict::Unproven => unsearched(unchecked.causes, unchecked.named),
    }
}

/// An empty answer's word on the paths its walk skipped by `causes` (`named`
/// as the sentence names them: `2 ignored, 1 hidden`, `node_modules/`):
/// they were not searched, so absence there is unproven, and the lead
/// searches them with those causes lifted. Nothing behind them is read, as
/// ripgrep leaves `-u` to its user. `None` when the walk skipped nothing.
pub fn unsearched(causes: &[Cause], named: &str) -> Option<Disclosure> {
    (!causes.is_empty()).then(|| Disclosure {
        causes: causes.to_vec(),
        text: format!(
            "Skipped paths ({named}) were not searched, so absence there is unproven. {}",
            walks_all_skipped(causes)
        ),
        unchecked: true,
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

/// Place `disclosure` on an internal result: the explicit related call in
/// its call bundle and the sentence as a warning. The requested scope was walked
/// whole, so what a check of the skipped paths left unchecked leaves it
/// complete.
pub fn attach(out: &mut Value, disclosure: &Disclosure, lead: Value) {
    out["calls"]["includeSkipped"] = lead;
    octocode_runtime_base::response::paging::push_warnings(out, [disclosure.text.clone()]);
}

#[cfg(test)]
mod tests;
