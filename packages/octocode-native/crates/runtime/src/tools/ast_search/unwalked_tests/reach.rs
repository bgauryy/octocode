//! The leads of a page cut at the walk ceiling reach every unread file;
//! a scope with no narrower call is a typed terminal limit.

use super::*;

/// What each operation reaches past the ceiling: the page that stops there
/// is terminal and honest about it, one `narrowScope` per unread directory
/// plus one for the unread files of a directory, and the calls together
/// list every candidate (an unread directory that exceeds the ceiling again
/// leads on in the same way).
fn leads_reach_every_unread_file(build: Build) {
    let root = Fixture::new();
    tree(&root);
    let _ceiling = octocode_engine::scan_budget::lower_walk_ceiling(5);
    let first = run(&root.0, &build(&root.0, json!({})));
    // The walk read five of the eleven candidates, in name order.
    assert_eq!(
        listed(&first),
        ["a1.ts", "a2.ts", "b1.ts", "d1.ts", "d2.ts"]
            .map(str::to_owned)
            .into_iter()
            .collect(),
        "{first}"
    );
    assert_eq!(first["terminalLimit"], json!(true), "{first}");
    assert_eq!(first["partialReasons"], json!(["scanBudget"]), "{first}");
    assert!(first["next"].get("expandScan").is_none(), "{first}");
    let found = leads(&first);
    // c's own files, the subdirectory e, and the root's own files.
    let paths: Vec<_> = found
        .iter()
        .map(|(_, lead)| lead_path(&root.0, lead))
        .collect();
    assert_eq!(paths, ["c", "e", ""], "{first}");
    let keys: Vec<_> = found.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(keys, ["narrowScope", "narrowScope2", "narrowScope3"]);
    // A scope of a directory's own files leaves its subdirectories out and
    // reads only what the scan did not.
    let files_only = |index: usize| -> BTreeSet<String> {
        let row = found[index].1["query"]["queries"][0].clone();
        assert_eq!(row["exclude"], json!(["/*/"]), "{row}");
        listed(&run(&root.0, &row))
    };
    assert_eq!(
        files_only(2),
        ["root1.ts", "root2.ts"]
            .map(str::to_owned)
            .into_iter()
            .collect()
    );
    let mut calls = Vec::new();
    let all = reached(&root.0, &build(&root.0, json!({})), &mut calls);
    assert_eq!(all, every_file(), "each file once: {calls:?}");
}

#[test]
fn match_leads_reach_every_file_the_walk_ceiling_left_unread() {
    leads_reach_every_unread_file(match_row);
}

#[test]
fn symbols_leads_reach_every_file_the_walk_ceiling_left_unread() {
    leads_reach_every_unread_file(symbols_row);
}

/// A directory with more candidates than the ceiling has no narrower path:
/// the page says so with a typed reason and names the directory, and offers
/// no call that could not reach its files.
#[test]
fn a_directory_over_the_ceiling_is_a_typed_terminal_limit() {
    let builds: [Build; 2] = [match_row, symbols_row];
    for build in builds {
        let root = Fixture::new();
        for index in 0..7 {
            root.write(&format!("flat/f{index}.ts"), "export const v = 1;\n");
        }
        let _ceiling = octocode_engine::scan_budget::lower_walk_ceiling(5);
        let out = run(&root.0, &build(&root.0, json!({})));
        assert_eq!(out["terminalLimit"], json!(true), "{out}");
        assert_eq!(
            out["partialReasons"],
            json!(["scanBudget", "flatDirectory"]),
            "{out}"
        );
        let warnings = out["warnings"].to_string();
        assert!(
            warnings.contains("flat") && warnings.contains("7 files"),
            "{out}"
        );
        assert!(leads(&out).is_empty(), "{out}");
    }
}

/// A glob that matches by position names no scope: another `path` would
/// change what it matches, so the page says why it offers none.
#[test]
fn an_anchored_glob_gets_no_narrowed_scope() {
    let cases: [(Build, &str); 2] = [(match_row, "include"), (symbols_row, "languageGlobs")];
    for (build, glob) in cases {
        let root = Fixture::new();
        tree(&root);
        let _ceiling = octocode_engine::scan_budget::lower_walk_ceiling(5);
        let extra = if glob == "include" {
            json!({"include":["/a/**", "/b/**", "/c/**", "/e/**", "/*.ts"]})
        } else {
            json!({"languageGlobs":{"typescript":["/**/*.ts"]}})
        };
        let out = run(&root.0, &build(&root.0, extra));
        assert_eq!(out["terminalLimit"], json!(true), "{out}");
        assert!(leads(&out).is_empty(), "{out}");
        assert!(out["warnings"].to_string().contains("by position"), "{out}");
    }
}

/// Past the lead cap the remaining scopes are counted and one
/// structureSearch lead lists them: nothing is dropped silently.
#[test]
fn scopes_past_the_lead_cap_are_counted_and_listed() {
    let root = Fixture::new();
    for index in 0..12 {
        root.write(&format!("d{index:02}/f.ts"), "export const v = 1;\n");
    }
    let _ceiling = octocode_engine::scan_budget::lower_walk_ceiling(1);
    let out = run(&root.0, &match_row(&root.0, json!({})));
    let found = leads(&out);
    let cap = unwalked::MAX_LEADS;
    assert_eq!(found.len(), cap + 1, "{out}");
    assert_eq!(found[cap].1["tool"], "structureSearch", "{out}");
    assert!(
        out["warnings"].to_string().contains("3 more unread scopes"),
        "{out}"
    );
}

/// `maxDepth` counts from the scope's own root, and a scope the depth never
/// reaches is not a lead.
#[test]
fn a_narrowed_scope_rebases_max_depth() {
    let query = json!({"operation":"match","path":"/r","pattern":"x","maxDepth":3});
    let reach = |depth, files_only| unwalked::Reach {
        dir: format!("/r/{depth}"),
        depth,
        files_only,
        skip: 0,
    };
    let depth_of = |reach: &unwalked::Reach| {
        unwalked::narrowed(&query, reach)
            .map(|lead| lead["query"]["queries"][0]["maxDepth"].clone())
    };
    assert_eq!(depth_of(&reach(0, true)), Some(json!(3)));
    assert_eq!(depth_of(&reach(2, false)), Some(json!(1)));
    assert_eq!(depth_of(&reach(3, false)), None);
}
