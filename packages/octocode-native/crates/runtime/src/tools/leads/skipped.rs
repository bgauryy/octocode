//! The one disclosure of what a local walk skipped (`localSearch`,
//! `structureSearch`, `astSearch` symbols and match): the typed
//! `skippedPaths` record of the paths themselves ([`SkippedEntry`],
//! [`skipped_paths`]), the sentence that says what sits behind them, the
//! `includeSkipped` lead that lifts exactly those causes, and the partial
//! policy when the check of them ran out of time. Each tool supplies only its
//! subject (what it looks for) and its query; the diagnosis is `empty_walk`'s,
//! fed by the walk's admission ledger (`policy::discovery::WalkAdmission`).

use crate::id::ToolId;
use crate::response::result::{Continuation, ToolError, query_row};
use crate::tools::empty_walk::{Cause, Verdict, lift};
use octocode_contract_types::{ErrorCode, SkippedPath, SkippedPathCause, SkippedPaths};
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

/// A walk's skip cause as `skippedPaths` names it.
pub(crate) fn skipped_cause(cause: Cause) -> SkippedPathCause {
    match cause {
        Cause::Ignored => SkippedPathCause::Ignored,
        Cause::Hidden => SkippedPathCause::Hidden,
        Cause::DefaultExcluded => SkippedPathCause::DefaultExcluded,
    }
}

/// What the path policy withheld from a walk of `root`, one entry per
/// reason at the walked scope: the withheld names stay withheld.
fn withheld(root: &str, withheld: &crate::policy::discovery::Withheld) -> Vec<SkippedEntry> {
    let at = |count: usize, reason: String| SkippedEntry {
        path: root.to_owned(),
        cause: SkippedPathCause::Withheld,
        reason: Some(reason),
        count,
    };
    let dirs = withheld.dirs.iter().map(|(name, count)| {
        at(
            *count,
            format!("security-policy dir {}/", name.trim_end_matches('/')),
        )
    });
    let rest = [
        (withheld.files, "credential file"),
        (
            withheld.git_files,
            "`.git` file (a submodule or worktree pointer)",
        ),
        (withheld.symlinks, "symlink not followed"),
        (withheld.other, "outside the allowed roots or unresolvable"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, reason)| at(count, reason.to_owned()));
    dirs.chain(rest).collect()
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
                count: (skip.count > 1).then(|| i64::try_from(skip.count).unwrap_or(i64::MAX)),
                reason: skip.reason,
            })
        })
        .collect::<Result<Vec<_>, ToolError>>()?;
    Ok(grouped(entries, every_path, incomplete))
}

/// `entries` first seen first, one per cause and rule: its first path with
/// `count` when more sit behind it, or with `every_path`
/// each path on its own. `sampled` when entries were
/// collapsed or the walk did not finish (`incomplete`). A result stored with
/// every path is shown either way through this. `None` when empty.
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

pub(crate) fn skipped_path_error(error: impl std::fmt::Display) -> ToolError {
    ToolError::new(
        ErrorCode::OutputContractViolation,
        format!("Invalid skipped-path evidence: {error}"),
    )
}

/// The `partialReasons` entry of a result whose check of the paths its walk
/// skipped ran out of time: that scope is unchecked, absence there unproven.
pub(crate) const SKIPPED_UNCHECKED: &str = "skippedPathsUnchecked";

/// What one walk's skipped paths hold, as every local tool states it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Disclosure {
    /// The causes the lead lifts.
    pub(crate) causes: Vec<Cause>,
    /// The sentence: a hint when the check found something, a warning when
    /// it was cut ([`Self::unchecked`]).
    pub(crate) text: String,
    /// The check ran out of time: the text is a warning and the result is
    /// partial ([`SKIPPED_UNCHECKED`]).
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
            let text = say(&diagnosis.because());
            Some(Disclosure {
                causes: diagnosis.causes().to_vec(),
                text,
                unchecked: false,
            })
        }
        Verdict::Unproven => (!unchecked.causes.is_empty()).then(|| Disclosure {
            causes: unchecked.causes.to_vec(),
            text: format!(
                "Skipped paths ({}) were not checked in time, so absence there is unproven.",
                unchecked.named
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
/// its call bundle, the sentence as a hint,
/// or, for a cut check, as a warning with the result marked partial.
pub(crate) fn attach(out: &mut Value, disclosure: &Disclosure, lead: Value) {
    out["calls"]["includeSkipped"] = lead;
    if disclosure.unchecked {
        crate::response::paging::mark_partial(
            out,
            SKIPPED_UNCHECKED,
            std::slice::from_ref(&disclosure.text),
        );
    } else {
        super::push_hint(out, disclosure.text.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::empty_walk::Walk;

    const ENTRIES: [&str; 2] = ["entry", "entries"];

    fn verdict(lifted: &[Cause], count: usize) -> Verdict {
        let root = std::path::Path::new("/w");
        crate::tools::empty_walk::diagnose(
            root,
            Walk {
                ignored: false,
                hidden: false,
                default_excludes: true,
            },
            |walk| {
                let reached = lifted.iter().all(|cause| !walk.skipping().contains(cause));
                Ok::<_, ()>(crate::tools::empty_walk::Hits {
                    count: if reached { count } else { 0 },
                    sample: if reached {
                        (0..count)
                            .map(|index| root.join(format!(".h/{index}.rs")))
                            .collect()
                    } else {
                        Vec::new()
                    },
                    cut: false,
                })
            },
        )
        .expect("diagnosis")
    }

    /// One sentence shape whatever the tool: the count, the subject and the
    /// reasons (the lead sits in `next`); a cut check is a warning that leaves the
    /// result partial and lifts every cause the walk skipped by.
    #[test]
    fn every_tool_discloses_skipped_paths_in_one_shape() {
        let none = Unchecked {
            causes: &[],
            named: "",
        };
        let found = disclose(&verdict(&[Cause::Hidden], 2), ENTRIES, none).expect("found");
        assert_eq!(found.causes, [Cause::Hidden]);
        assert_eq!(found.text, "2 entries behind skipped paths (hidden .h/).");
        assert!(!found.unchecked);
        assert_eq!(disclose(&Verdict::Absent, ENTRIES, none), None);
        assert_eq!(disclose(&Verdict::Unproven, ENTRIES, none), None);
        let cut = disclose(
            &Verdict::Unproven,
            ENTRIES,
            Unchecked {
                causes: &[Cause::Ignored, Cause::DefaultExcluded],
                named: "3 ignored",
            },
        )
        .expect("cut");
        assert!(cut.unchecked);
        assert_eq!(cut.causes, [Cause::Ignored, Cause::DefaultExcluded]);
        let mut out = serde_json::json!({});
        attach(
            &mut out,
            &cut,
            lead(
                ToolId::StructureSearch,
                &serde_json::json!({"operation": "files", "path": ".", "page": 3, "snapshot": "x"}),
                &cut.causes,
            ),
        );
        assert_eq!(
            out["partialReasons"],
            serde_json::json!([SKIPPED_UNCHECKED])
        );
        assert!(
            out["warnings"][0]
                .as_str()
                .expect("warning")
                .contains("unproven")
        );
        assert_eq!(out["calls"]["includeSkipped"]["kind"], "related");
        let row = &out["calls"]["includeSkipped"]["input"]["queries"][0];
        assert_eq!(row["includeIgnored"], true, "{row}");
        assert_eq!(row["defaultExcludes"], false, "{row}");
        assert!(
            row.get("page").is_none() && row.get("snapshot").is_none(),
            "{row}"
        );
    }
}
