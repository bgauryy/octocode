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
        [vec![
            "app/src/hosts".to_owned(),
            "app/src/hosts/shared".to_owned()
        ]],
        "{}",
        issues.value
    );
}

/// Each hop of a directory cycle's witness names the file import that
/// closes it at its line, as a file cycle's hop does.
#[test]
fn a_directory_cycle_hop_names_its_import_line() {
    let dir = git_app(&[
        (
            "src/a/one.ts",
            "\nimport { two } from '../b/two';\nexport const one = two;\n",
        ),
        (
            "src/b/two.ts",
            "export const two = 2;\nexport const back = () => import('../a/one');\n",
        ),
        (
            "src/b/three.ts",
            "\n\nimport { one } from '../a/one';\nexport const three = one;\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let issues = ask(root, "issues", None, |o| {
        o.detectors = vec!["dir-cycle".into()];
        o.min_tier = Some(0);
    });
    let hops = issues.value["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["detector"] == "dir-cycle")
        .flat_map(|row| {
            row["evidence"]["witness"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    assert!(!hops.is_empty(), "{}", issues.value);
    for hop in &hops {
        let via = &hop["via"];
        let from = via["from"].as_str().expect("from file");
        let line = via["line"].as_u64().unwrap_or_else(|| panic!("{hop}"));
        let text = std::fs::read_to_string(root.join(from)).expect("source");
        let at = usize::try_from(line).expect("line") - 1;
        let written = text.lines().nth(at).unwrap_or_default();
        assert!(written.contains("import"), "{hop}: {written:?}");
    }
}
