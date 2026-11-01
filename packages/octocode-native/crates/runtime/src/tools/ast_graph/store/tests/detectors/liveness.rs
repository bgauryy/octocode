//! Liveness that spreads from files loaded by name: a bundler entry named
//! in a build script keeps the files it imports as live as itself.
use super::*;

/// The agents-communication shape: `bin/cli.mjs` runs the build output
/// (`../dist/cli.js`), and `src/build.ts` names `src/communicate.ts` as an esbuild
/// entry in a string. `src/communicate.ts` is unreachable but mentioned (tier 60);
/// `src/runtime.ts`, which it imports and a test also imports, is no
/// tier-90 `test-only` file: it is reached from the mentioned entry, so it
/// drops to tier 60 and names that entry. A file only a test reaches stays
/// tier 90.
#[test]
fn files_a_mentioned_unreachable_file_reaches_are_not_test_only() {
    let dir = git_app(&[
        (
            "package.json",
            r#"{"name":"comms","type":"module","bin":{"comms":"bin/cli.mjs"}}"#,
        ),
        ("bin/cli.mjs", "import '../dist/cli.js';\n"),
        (
            "src/build.ts",
            "export const entryPoints = ['src/communicate.ts'];\n",
        ),
        (
            "src/communicate.ts",
            "import { serve } from './runtime';\nserve();\n",
        ),
        ("src/runtime.ts", "export const serve = () => 1;\n"),
        ("src/only-tested.ts", "export const helper = () => 2;\n"),
        (
            "src/runtime.test.ts",
            "import { serve } from './runtime';\nimport { helper } from './only-tested';\nserve(); helper();\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    for detectors in [vec!["test-only"], vec!["unreachable-file", "test-only"]] {
        let issues = ask(root, "issues", None, |o| {
            o.detectors = detectors.iter().map(|d| (*d).to_owned()).collect();
            o.min_tier = Some(0);
        });
        let row = |subject: &str| {
            issues.value["rows"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|row| row["detector"] == "test-only" && row["subject"] == subject)
                .cloned()
                .unwrap_or_else(|| panic!("{subject}: {}", issues.value))
        };
        let reached = row("src/runtime.ts");
        assert_eq!(reached["tier"], 60, "{detectors:?}: {reached}");
        assert_eq!(
            reached["evidence"]["reachedFrom"], "src/communicate.ts",
            "{detectors:?}: {reached}"
        );
        let tested = row("src/only-tested.ts");
        assert_eq!(tested["tier"], 90, "{detectors:?}: {tested}");
        assert!(tested["evidence"].get("reachedFrom").is_none(), "{tested}");
        assert_eq!(
            findings(&issues, "unreachable-file").is_empty(),
            !detectors.contains(&"unreachable-file"),
            "only the requested detectors are listed: {}",
            issues.value
        );
    }
}
