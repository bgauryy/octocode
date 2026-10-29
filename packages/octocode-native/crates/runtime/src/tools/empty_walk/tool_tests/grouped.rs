//! A result that found its matches discloses the paths its walk skipped once
//! per cause and rule, never once per path: the repeated entries carry no
//! evidence, and the lead that lifts the cause reaches every one of them.

use super::*;

#[test]
fn skipped_paths_of_one_cause_are_one_counted_entry() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "gen1/\ngen2/\ngen3/\n").expect("gitignore");
    for dir in ["gen1", "gen2", "gen3", "src"] {
        std::fs::create_dir(root.0.join(dir)).expect("dir");
        std::fs::write(root.0.join(dir).join("needle.ts"), "function needle() {}\n").expect("file");
    }
    for tool in tools() {
        let out = (tool.run)(&root.0, &(tool.query)(&root.0));
        assert!((tool.found)(&out), "{}: {out}", tool.name);
        let ignored = out["skippedPaths"]["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| entry["cause"] == "ignored")
            .collect::<Vec<_>>();
        assert!(ignored.len() <= 1, "{}: {out}", tool.name);
        match ignored.first() {
            Some(entry) => {
                assert_eq!(entry["count"], 3, "{}: {out}", tool.name);
                assert_eq!(out["skippedPaths"]["sampled"], true, "{}: {out}", tool.name);
            }
            // astSearch discloses only what its walk prunes or no flag lifts.
            None => assert!(tool.name.starts_with("astSearch"), "{}: {out}", tool.name),
        }
    }
}
