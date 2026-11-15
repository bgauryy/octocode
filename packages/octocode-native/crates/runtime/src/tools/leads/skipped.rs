//! The one disclosure of what a local walk skipped (`localSearch`,
//! `structureSearch`, `astSearch` symbols and match): the typed
//! `skippedPaths` record of the paths themselves ([`SkippedEntry`],
//! [`skipped_paths`]), the sentence that says what sits behind them, the
//! `includeSkipped` lead that lifts exactly those causes, and the partial
//! policy when the check of them spent its work budget. Each tool supplies only its
//! subject (what it looks for) and its query; the diagnosis is `empty_walk`'s,
//! fed by the walk's admission ledger (`policy::discovery::WalkAdmission`).

use crate::id::ToolId;
use crate::response::result::{Continuation, ToolError, query_row};
use crate::tools::empty_walk::{Cause, Verdict, lift};
use octocode_contract_types::{ErrorCode, SkippedPath, SkippedPathCause, SkippedPaths};
use octocode_engine::num::saturating_i64;
use serde_json::Value;

/// One path a walk skipped, or `count` paths behind one cause and rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SkippedEntry {
    /// Absolute; the response spells it workspace-relative.
    pub(crate) path: String,
    pub(crate) cause: SkippedPathCause,
    /// The rule that skipped it, when the walk names one.
    pub(crate) reason: Option<String>,
    pub(crate) count: usize,
}

impl SkippedEntry {
    pub(crate) fn path(path: impl Into<String>, cause: SkippedPathCause) -> Self {
        Self {
            path: path.into(),
            cause,
            reason: None,
            count: 1,
        }
    }

    pub(crate) fn because(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

/// A walk of `root`'s skipped paths of `cause`, root-relative (a
/// directory's trailing `/` dropped).
pub(crate) fn under<'a>(
    root: &'a std::path::Path,
    cause: SkippedPathCause,
    relative: impl IntoIterator<Item = &'a String> + 'a,
) -> impl Iterator<Item = SkippedEntry> + 'a {
    relative.into_iter().map(move |path| {
        let path = path.strip_suffix('/').unwrap_or(path);
        SkippedEntry::path(root.join(path).to_string_lossy().into_owned(), cause)
    })
}

/// The causes a walk skipped paths by (what a lead over them lifts): a dot
/// directory the default prune also names (`.github`, `.cache`), or a dot
/// file the default excludes match, sits behind both.
pub(crate) fn skipped_causes(skipped: &crate::policy::discovery::WalkSkips) -> Vec<Cause> {
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
pub(crate) fn skipped_cause(cause: Cause) -> SkippedPathCause {
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
fn withheld(root: &str, withheld: &crate::policy::discovery::Withheld) -> Vec<SkippedEntry> {
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
pub(crate) fn unlifted(
    root: &str,
    policy: &crate::policy::discovery::Withheld,
    errors: WalkErrors,
) -> Vec<SkippedEntry> {
    let mut skips = withheld(root, policy);
    skips.extend(unreadable(root, errors));
    skips
}

/// The entries a walk could not read, and how many of them for want of
/// permission.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WalkErrors {
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
/// its fields say. A withheld or unreadable entry's path is the directory
/// that holds what was left out (the policy never names a withheld entry), so
/// it counts what sits under that directory; any other entry's path is the
/// first path left out, and its count is how many share its cause.
/// The path and the reason (a rule's text, an error) are a walk's, quoted
/// as judged ([`crate::security::quoted`]).
pub(crate) fn disclosure(entry: &SkippedPath, shown: &str) -> String {
    let cause = serde_json::to_value(entry.cause)
        .ok()
        .and_then(|cause| cause.as_str().map(str::to_owned))
        .unwrap_or_default();
    let count = entry.count.unwrap_or(1);
    let shown = crate::security::quoted(shown);
    let quoted_reason = entry.reason.as_deref().map(crate::security::quoted);
    let reason = quoted_reason
        .as_ref()
        .map(|reason| format!(" ({reason})"))
        .unwrap_or_default();
    match entry.cause {
        SkippedPathCause::Withheld | SkippedPathCause::Unreadable => {
            format!("{count} {cause} under `{shown}`{reason}")
        }
        _ if count > 1 => format!("`{shown}` and {} more {cause}{reason}", count - 1),
        _ => match &quoted_reason {
            Some(reason) => format!("`{shown}` ({cause}: {reason})"),
            None => format!("`{shown}` ({cause})"),
        },
    }
}

/// The `skippedPaths` of `skips` ([`grouped`]); `None` when the walk
/// skipped nothing.
pub(crate) fn skipped_paths(
    skips: impl IntoIterator<Item = SkippedEntry>,
    every_path: bool,
    incomplete: bool,
) -> Result<Option<SkippedPaths>, ToolError> {
    let entries = skips
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

/// `entries` first seen first, one per cause and rule: its first path with
/// `count` when more sit behind it, or with `every_path` each path on its
/// own. A withheld or unreadable entry's path is the directory that holds
/// what was left out, so a group of them is the deepest directory holding
/// them all, with their summed count. `sampled` when entries were collapsed
/// or the walk did not finish (`incomplete`). A result stored with every
/// path is shown either way through this. `None` when empty.
pub(crate) fn grouped(
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
                if group.path != entry.path && holds_what_was_left_out(entry.cause) {
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
    let collapsed = groups.iter().any(|group| group.count.is_some());
    (!groups.is_empty()).then_some(SkippedPaths {
        entries: groups,
        sampled: collapsed || incomplete,
    })
}

/// Whether an entry of `cause` names the directory holding what was left
/// out ([`disclosure`]), not the first path left out.
fn holds_what_was_left_out(cause: SkippedPathCause) -> bool {
    matches!(
        cause,
        SkippedPathCause::Withheld | SkippedPathCause::Unreadable
    )
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

pub(crate) fn skipped_path_error(error: impl std::fmt::Display) -> ToolError {
    ToolError::new(
        ErrorCode::OutputContractViolation,
        format!("Invalid skipped-path evidence: {error}"),
    )
}

/// What one walk's skipped paths hold, as every local tool states it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Disclosure {
    /// The causes the lead lifts.
    pub(crate) causes: Vec<Cause>,
    /// The sentence: what the check found, or that it was cut.
    pub(crate) text: String,
    /// The check spent its work budget first, so the skipped paths stay
    /// unsearched.
    pub(crate) unchecked: bool,
}

/// What the walk skipped, for a check cut before it found anything: the
/// causes it skipped paths by (what the lead lifts) and how the sentence
/// names them (`2 ignored, 1 hidden`, `node_modules/`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Unchecked<'a> {
    pub(crate) causes: &'a [Cause],
    pub(crate) named: &'a str,
}

/// What a lead built from a cut probe costs: the probe proved a prefix of
/// the skipped paths at most, so the lead lifts the causes for the whole
/// scope (narrowing to the sampled entries would drop matches past them).
/// The sentence names the lead as the agent finds it: the `related[]`
/// request that sets exactly the flags lifting `causes` ([`lead`]).
pub(crate) fn walks_all_skipped(causes: &[Cause]) -> String {
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
pub(crate) fn disclose(
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
        Verdict::Unproven => (!unchecked.causes.is_empty()).then(|| Disclosure {
            causes: unchecked.causes.to_vec(),
            text: format!(
                "Skipped paths ({}) were not checked within the probe budget, so absence there is unproven. {}",
                unchecked.named,
                walks_all_skipped(unchecked.causes)
            ),
            unchecked: true,
        }),
    }
}

/// The `includeSkipped` lead: `query` as a fresh first call (no cursor
/// field) with exactly `causes` lifted.
pub(crate) fn lead(tool: ToolId, query: &impl serde::Serialize, causes: &[Cause]) -> Value {
    let mut row = query_row(
        query,
        &crate::contracts::state_fields(tool)
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
pub(crate) fn attach(out: &mut Value, disclosure: &Disclosure, lead: Value) {
    out["calls"]["includeSkipped"] = lead;
    crate::response::paging::push_warnings(out, [disclosure.text.clone()]);
}

#[cfg(test)]
mod tests;
