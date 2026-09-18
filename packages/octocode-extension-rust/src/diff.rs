use crate::{DiffArtifacts, DiffOperation};
use std::fmt::Write;

pub fn line_diff(old: &str, new: &str) -> Vec<DiffOperation> {
    let old_lines: Vec<_> = old.split('\n').collect();
    let new_lines: Vec<_> = new.split('\n').collect();
    let diff = similar::TextDiff::from_slices(&old_lines, &new_lines);
    diff.iter_all_changes()
        .map(|change| DiffOperation {
            op_type: match change.tag() {
                similar::ChangeTag::Equal => "same",
                similar::ChangeTag::Delete => "remove",
                similar::ChangeTag::Insert => "add",
            }
            .into(),
            line: change.value().to_owned(),
        })
        .collect()
}

pub fn diff_artifacts(path: &str, old: &str, new: &str) -> DiffArtifacts {
    let old_lines: Vec<_> = old.split('\n').collect();
    let new_lines: Vec<_> = new.split('\n').collect();
    let text_diff = similar::TextDiff::from_slices(&old_lines, &new_lines);
    let changes: Vec<_> = text_diff.iter_all_changes().collect();
    let Some(first) = changes
        .iter()
        .position(|change| change.tag() != similar::ChangeTag::Equal)
    else {
        return DiffArtifacts {
            diff: String::new(),
            patch: format!(
                "--- {path}\n+++ {path}\n@@ -{},0 +{},0 @@\n",
                old_lines.len() + 1,
                new_lines.len() + 1
            ),
        };
    };
    let last = changes
        .iter()
        .rposition(|change| change.tag() != similar::ChangeTag::Equal)
        .expect("first change exists");
    let hunk = &changes[first..=last];
    let old_count = hunk
        .iter()
        .filter(|change| change.tag() != similar::ChangeTag::Insert)
        .count();
    let new_count = hunk
        .iter()
        .filter(|change| change.tag() != similar::ChangeTag::Delete)
        .count();
    let start = first + 1;
    let mut patch =
        format!("--- {path}\n+++ {path}\n@@ -{start},{old_count} +{start},{new_count} @@\n");
    let mut diff = String::new();
    for change in hunk {
        let prefix = match change.tag() {
            similar::ChangeTag::Equal => ' ',
            similar::ChangeTag::Insert => '+',
            similar::ChangeTag::Delete => '-',
        };
        writeln!(&mut patch, "{prefix}{}", change.value())
            .expect("String formatting is infallible");
        if prefix != ' ' {
            if !diff.is_empty() {
                diff.push('\n');
            }
            write!(&mut diff, "{prefix} {}", change.value())
                .expect("String formatting is infallible");
        }
    }
    DiffArtifacts { diff, patch }
}
