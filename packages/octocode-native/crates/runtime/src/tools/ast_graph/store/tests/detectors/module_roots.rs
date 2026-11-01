//! A module root file and its own folder are one directory unit.
use super::*;

/// The agents-communication shape: `src/live.ts` is the facade of
/// `src/live/` and imports its children, which import shared `src/` code.
/// The facade belongs to `src/live`, so `src` and `src/live` form no
/// directory cycle. `src/hosts` and `src/hosts/shared`, whose files import
/// each other, still do.
#[test]
fn a_module_root_file_counts_in_its_own_folder() {
    let dir = git_app(&[
        ("src/support.ts", "export const text = (s: string) => s;\n"),
        (
            "src/live.ts",
            "import { grade } from './live/grading';\nimport { text } from './support';\nexport const run = () => grade(text('x'));\n",
        ),
        (
            "src/live/grading.ts",
            "import { text } from '../support';\nexport const grade = (s: string) => text(s);\n",
        ),
        ("src/hosts/lease-check.ts", "export const lease = 1;\n"),
        (
            "src/hosts/pi.ts",
            "import { guard } from './shared/pi-edit-guard';\nexport const pi = guard;\n",
        ),
        (
            "src/hosts/shared/pi-edit-guard.ts",
            "import { lease } from '../lease-check';\nexport const guard = lease;\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let issues = ask(root, "issues", None, |o| {
        o.detectors = vec!["dir-cycle".into()];
        o.min_tier = Some(0);
    });
    let mut groups = issues.value["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["detector"] == "dir-cycle")
        .map(|row| {
            let mut dirs = row["evidence"]["dirs"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            dirs.sort();
            dirs
        })
        .collect::<Vec<_>>();
    groups.sort();
    assert_eq!(
        groups,
        [vec!["src/hosts".to_owned(), "src/hosts/shared".to_owned()]],
        "{}",
        issues.value
    );
}
