//! When a scan is too broad for one snapshot, and what to ingest instead.
use super::super::build_scope::BuildScope;
use super::super::limits::{Limits, Tally};
use crate::response::result::ToolError;
use octocode_contract_types::ErrorCode;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

/// The snapshot refuses past its bounds only after every file is parsed; a
/// running lower bound of what it will hold stops the scan as soon as that
/// refusal is certain: the [`Tally`] of nodes and `contains` edges every
/// linked file projects. Source correspondence is not tallied against the
/// snapshot: readers admit its origins one block at a time, so its size
/// alone never refuses a snapshot. The build's own working set is: every
/// kept file is held until projection, so each is charged its
/// [`super::super::limits::build_charge`] and the scan stops once the next
/// file passes the build budget, before linking, not in swap.
pub(super) struct Admission<'a> {
    root: &'a Path,
    scope: BuildScope,
    limits: Limits,
    charged: Mutex<Charged>,
    walked: Vec<String>,
}

/// What the files admitted so far charge.
#[derive(Default)]
struct Charged {
    files: usize,
    tally: Tally,
    /// The build charge of every file admitted.
    bytes: usize,
    /// The refusal of the file that first passed a bound: parallel workers
    /// admitting after it get the same one, so the answer names the files
    /// that passed it whichever worker reports first.
    refused: Option<ToolError>,
}

impl<'a> Admission<'a> {
    pub(super) fn new(
        scan: &super::GraphScan,
        root: &'a Path,
        entries: &[octocode_engine::types::FileSystemEntry],
    ) -> Self {
        Self {
            root,
            scope: BuildScope::of_scan(scan),
            limits: scan.limits,
            charged: Mutex::new(Charged::default()),
            walked: entries
                .iter()
                .filter(|entry| entry.entry_type == octocode_engine::types::FileKind::File)
                .map(|entry| entry.relative_path.clone())
                .collect(),
        }
    }

    /// Charges one file kept for linking, holding `declarations` and
    /// costing the build `bytes` ([`super::super::limits::build_charge`]);
    /// `scopeTooBroad`, naming narrower scopes, once the snapshot can no
    /// longer publish or the build would pass its memory budget.
    pub(super) fn admit(&self, declarations: usize, bytes: usize) -> Result<(), ToolError> {
        let mut charged = self.charged.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(refused) = &charged.refused {
            return Err(refused.clone());
        }
        charged.files += 1;
        charged.tally.add(Tally::file(declarations));
        charged.bytes = charged.bytes.saturating_add(bytes);
        let Err(over) = self
            .limits
            .admit(&charged.tally)
            .and_then(|()| self.limits.admit_build(charged.bytes))
        else {
            return Ok(());
        };
        let refused = self.too_broad(&format!(
            "the first {} of {} files are too many for one graph snapshot: they {over}",
            charged.files,
            self.walked.len()
        ));
        charged.refused = Some(refused.clone());
        Err(refused)
    }

    /// What the admitted files certainly project, carried into linking.
    pub(super) fn tally(&self) -> Tally {
        self.charged
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .tally
    }

    fn too_broad(&self, message: &str) -> ToolError {
        too_broad(
            self.root,
            &self.scope,
            self.walked.iter().map(String::as_str),
            message,
        )
    }
}

/// `scopeTooBroad` for a scan of `root` walking `files`, with one
/// narrower ingest per hint.
pub(super) fn too_broad<'f>(
    root: &Path,
    scope: &BuildScope,
    files: impl IntoIterator<Item = &'f str>,
    message: &str,
) -> ToolError {
    let mut error = ToolError::new(ErrorCode::ScopeTooBroad, message);
    error.hints = narrower_scope_hints(root, scope, files);
    error
}

/// Directories listed when a scope is too broad.
const SUGGESTED: usize = 5;
/// A directory holding at least this share of the files (in percent) is
/// itself too broad: the suggestions descend into it.
const DOMINANT_PERCENT: usize = 80;

/// The directories under `root` that split `files` (root-relative) into
/// narrower scopes, largest first, with their file counts. One directory
/// holding nearly every file is descended into, so the suggestions are the
/// first level where the files actually divide.
pub(crate) fn narrower_scopes<'a>(
    files: impl IntoIterator<Item = &'a str>,
) -> Vec<(String, usize)> {
    let files = files.into_iter().collect::<Vec<_>>();
    let mut prefix = String::new();
    loop {
        let mut counts = BTreeMap::<&str, usize>::new();
        let mut total = 0;
        for rest in files
            .iter()
            .filter_map(|file| file.strip_prefix(prefix.as_str()))
        {
            total += 1;
            if let Some((dir, _)) = rest.split_once('/') {
                *counts.entry(dir).or_default() += 1;
            }
        }
        let mut ranked = counts.into_iter().collect::<Vec<_>>();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        match ranked.first() {
            Some((dir, count)) if count * 100 >= total * DOMINANT_PERCENT => {
                let next = format!("{prefix}{dir}/");
                // A dominant directory with subdirectories of its own is
                // split further; one without them is listed at this level.
                if files
                    .iter()
                    .filter_map(|file| file.strip_prefix(next.as_str()))
                    .any(|rest| rest.contains('/'))
                {
                    prefix = next;
                    continue;
                }
                return top(ranked, &prefix);
            }
            _ => return top(ranked, &prefix),
        }
    }
}

/// The first [`SUGGESTED`] of `ranked`, as paths under `prefix`.
fn top(ranked: Vec<(&str, usize)>, prefix: &str) -> Vec<(String, usize)> {
    ranked
        .into_iter()
        .take(SUGGESTED)
        .map(|(dir, count)| (format!("{prefix}{dir}"), count))
        .collect()
}

/// The hints of a too-broad scan of `root`: one graphBuild call per
/// narrower scope, each building as the refused build did: its walk,
/// `exclude` and `parserGlobs` (rebased onto the narrower root).
pub(crate) fn narrower_scope_hints<'a>(
    root: &Path,
    scope: &BuildScope,
    files: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let scopes = narrower_scopes(files);
    if scopes.is_empty() {
        return vec!["Build a narrower directory, or exclude generated or vendored trees.".into()];
    }
    let mut hints = vec![
        "Build a narrower directory (files per directory, largest first), or exclude generated or vendored trees:".to_owned(),
    ];
    hints.extend(scopes.into_iter().map(|(dir, count)| {
        let path = root.join(&dir).to_string_lossy().into_owned();
        let mut call = serde_json::json!({"path": path});
        scope.inside(&dir).write_row(&mut call);
        format!(
            "graphBuild {call}  # {}",
            octocode_engine::text::counted(count, "file", "files")
        )
    }));
    hints
}

/// Appends the narrower-scope hints to a `scopeTooBroad` error row of a
/// scan of `root`, after the hints it already carries and without
/// repeating one; any other row is left as it is.
pub(crate) fn hint_narrower_scopes<'a>(
    row: &mut serde_json::Value,
    root: &Path,
    scope: &BuildScope,
    files: impl IntoIterator<Item = &'a str>,
) {
    if row["errorCode"] == serde_json::json!(octocode_contract_types::ErrorCode::ScopeTooBroad) {
        for hint in narrower_scope_hints(root, scope, files) {
            crate::response::result::add_hints(row, [hint]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestions_descend_through_a_directory_holding_nearly_every_file() {
        let files = [
            "packages/native/a.rs",
            "packages/native/b.rs",
            "packages/native/c.rs",
            "packages/web/a.ts",
            "packages/web/b.ts",
            "packages/cli/a.ts",
            "packages/x.ts",
            "README.md",
        ];
        assert_eq!(
            narrower_scopes(files),
            [
                ("packages/native".to_owned(), 3),
                ("packages/web".to_owned(), 2),
                ("packages/cli".to_owned(), 1),
            ]
        );
    }

    #[test]
    fn suggestions_stop_at_the_level_where_files_divide() {
        let files = ["a/1.ts", "a/2.ts", "b/1.ts", "b/2.ts", "c/1.ts", "top.ts"];
        assert_eq!(
            narrower_scopes(files),
            [
                ("a".to_owned(), 2),
                ("b".to_owned(), 2),
                ("c".to_owned(), 1)
            ]
        );
    }

    #[test]
    fn a_flat_tree_has_no_narrower_directory() {
        assert!(narrower_scopes(["a.ts", "b.ts"]).is_empty());
        let hints = narrower_scope_hints(Path::new("/w"), &BuildScope::default(), ["a.ts"]);
        assert_eq!(hints.len(), 1, "{hints:?}");
        assert!(hints[0].contains("exclude "), "{hints:?}");
    }

    #[test]
    fn narrower_scope_hints_join_the_hints_an_error_already_carries() {
        let files = ["a/1.ts", "a/2.ts", "b/1.ts"];
        let mut row = serde_json::json!({
            "errorCode": octocode_contract_types::ErrorCode::ScopeTooBroad,
            "hints": ["earlier hint"],
        });
        let scope = BuildScope::default();
        hint_narrower_scopes(&mut row, Path::new("/w"), &scope, files);
        hint_narrower_scopes(&mut row, Path::new("/w"), &scope, files);
        let hints = row["hints"].as_array().expect("hints");
        let texts = hints.iter().filter_map(|h| h.as_str()).collect::<Vec<_>>();
        assert_eq!(texts[0], "earlier hint", "{texts:?}");
        for hint in narrower_scope_hints(Path::new("/w"), &scope, files) {
            assert_eq!(
                texts.iter().filter(|text| **text == hint).count(),
                1,
                "{hint}: {texts:?}"
            );
        }
    }
}
