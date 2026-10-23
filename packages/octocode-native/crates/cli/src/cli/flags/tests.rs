use super::*;
use clap::{Arg, ArgAction};

fn parse(tool: &str, argv: &[&str]) -> Result<Option<Value>, String> {
    let command = augment(
        clap::Command::new(tool.to_owned())
            .arg(Arg::new("args").num_args(0..).action(ArgAction::Append)),
        tool,
    );
    let matches = command
        .try_get_matches_from(std::iter::once(tool).chain(argv.iter().copied()))
        .map_err(|error| error.to_string())?;
    let args: Vec<String> = matches
        .get_many::<String>("args")
        .into_iter()
        .flatten()
        .cloned()
        .collect();
    query(tool, &matches, &args)
}

#[test]
fn rg_style_local_search_builds_the_json_query() {
    assert_eq!(
        parse(
            "localSearch",
            &[
                "SearcherBuilder",
                "crates",
                "-w",
                "-i",
                "-g",
                "*.rs",
                "-C",
                "1"
            ]
        )
        .unwrap(),
        Some(json!({
            "matchString": ["SearcherBuilder"], "path": "crates", "wholeWord": true,
            "caseMode": "insensitive", "include": ["*.rs"], "contextLines": 1
        }))
    );
    // `-e` sets the pattern, so the first positional is the path; the path defaults to `.`.
    assert_eq!(
        parse("localSearch", &["-e", "a", "-e", "b", "src"]).unwrap(),
        Some(json!({"matchString": ["a", "b"], "path": "src"}))
    );
    assert_eq!(
        parse("localSearch", &["TODO", "-l"]).unwrap(),
        Some(json!({"matchString": ["TODO"], "path": ".", "resultView": "files"}))
    );
    // A long flag beats a preset; `--flag=false` is false.
    assert_eq!(
        parse(
            "localSearch",
            &[
                "x",
                "-i",
                "--caseMode",
                "sensitive",
                "--defaultExcludes=false"
            ]
        )
        .unwrap(),
        Some(
            json!({"matchString": ["x"], "path": ".", "caseMode": "sensitive", "defaultExcludes": false})
        )
    );
}

#[test]
fn operations_and_split_values_follow_gh() {
    assert_eq!(
        parse(
            "ghSearchHistory",
            &[
                "pullRequest",
                "fix",
                "bug",
                "-R",
                "o/r",
                "-s",
                "merged",
                "-L",
                "3"
            ]
        )
        .unwrap(),
        Some(json!({
            "operation": "pullRequest", "keywords": ["fix", "bug"], "owner": "o",
            "repo": "r", "state": "merged", "pageSize": 3
        }))
    );
    assert_eq!(
        parse("ghGetHistoryItem", &["pullRequest", "12", "-R", "o/r"]).unwrap(),
        Some(json!({"operation": "pullRequest", "number": 12, "owner": "o", "repo": "r"}))
    );
    assert_eq!(
        parse("ghGetHistoryItem", &["compare", "v1...v2", "-R", "o/r"]).unwrap(),
        Some(
            json!({"operation": "compare", "base": "v1", "head": "v2", "owner": "o", "repo": "r"})
        )
    );
    assert_eq!(
        parse("ghCloneRepo", &["vercel/next.js"]).unwrap(),
        Some(json!({"owner": "vercel", "repo": "next.js"}))
    );
    assert!(parse("ghSearchCode", &["x", "-R", "noslash"]).is_err());
}

#[test]
fn line_suffixes_read_into_ranges_or_line() {
    assert_eq!(
        parse("localFetch", &["no/such/file.rs:10-40"]).unwrap(),
        Some(json!({"path": "no/such/file.rs", "ranges": ["10-40"]}))
    );
    // Several ranges, comma-joined.
    assert_eq!(
        parse("localFetch", &["no/such/file.rs:10-20,40,50-60"]).unwrap(),
        Some(json!({"path": "no/such/file.rs", "ranges": ["10-20", "40-40", "50-60"]}))
    );
    // workspaceSymbol scoped by --workspaceRoot: one argument is the symbol.
    assert_eq!(
        parse(
            "lspSearch",
            &["workspaceSymbol", "--workspaceRoot", ".", "RootedPrune"]
        )
        .unwrap(),
        Some(
            json!({"operation": "workspaceSymbol", "workspaceRoot": ".", "symbolName": "RootedPrune"})
        )
    );
    assert_eq!(
        parse("lspSearch", &["workspaceSymbol", "src", "RootedPrune"]).unwrap(),
        Some(json!({"operation": "workspaceSymbol", "path": "src", "symbolName": "RootedPrune"}))
    );
    assert!(parse("lspSearch", &["references", "no/such/a.ts:10,20", "run"]).is_err());
    assert_eq!(
        parse("lspSearch", &["references", "no/such/a.ts:10", "run"]).unwrap(),
        Some(
            json!({"operation": "references", "path": "no/such/a.ts", "line": 10, "symbolName": "run"})
        )
    );
}

#[test]
fn review_edge_cases() {
    // `--bool false` with a space is refused with the `=` spelling.
    assert!(
        parse("localSearch", &["foo", "--wholeWord", "false"])
            .is_err_and(|e| e.contains("--wholeWord=false"))
    );
    // `-e` takes a pattern that starts with `-`.
    assert_eq!(
        parse("localSearch", &["-e", "-flag", "src"]).unwrap(),
        Some(json!({"matchString": ["-flag"], "path": "src"}))
    );
    // Repeated short letters: the last one wins.
    assert_eq!(
        parse("localSearch", &["x", "-i", "-s", "-i"]).unwrap(),
        Some(json!({"matchString": ["x"], "path": ".", "caseMode": "insensitive"}))
    );
    // An explicit --operation keeps a positional named like an operation.
    assert_eq!(
        parse("structureSearch", &["--operation", "tree", "files"]).unwrap(),
        Some(json!({"operation": "tree", "path": "files"}))
    );
    // PATH:N beside --ranges, and a range for a single line, are errors.
    assert!(parse("localFetch", &["no/such/a.txt:1", "--ranges", "4-5"]).is_err());
    assert!(parse("lspSearch", &["references", "no/such/a.ts:10-20", "run"]).is_err());
    // `+10` is not a line number; flags win over OWNER/REPO.
    assert_eq!(
        parse("localFetch", &["no/such/a.txt:+10"]).unwrap(),
        Some(json!({"path": "no/such/a.txt:+10"}))
    );
    assert_eq!(
        parse("ghCloneRepo", &["--owner", "o", "--repo", "r"]).unwrap(),
        Some(json!({"owner": "o", "repo": "r"}))
    );
    // A GitHub path with a line suffix is never checked on this disk.
    assert_eq!(
        parse("ghGetFileContent", &["src/a.rs:1-5", "-R", "o/r"]).unwrap(),
        Some(json!({"owner": "o", "repo": "r", "path": "src/a.rs", "ranges": ["1-5"]}))
    );
}

#[test]
fn enum_lists_take_commas_and_l_points_to_t() {
    assert_eq!(
        parse(
            "ghGetHistoryItem",
            &["pullRequest", "1", "-R", "o/r", "--sections", "body,files"]
        )
        .unwrap()
        .map(|query| query["sections"].clone()),
        Some(json!(["body", "files"]))
    );
    // Identifier lists split on commas.
    assert_eq!(
        parse(
            "astSearch",
            &["symbols", "src", "--symbolName", "prunes,pruned"]
        )
        .unwrap()
        .map(|q| q["symbolName"].clone()),
        Some(json!(["prunes", "pruned"]))
    );
    // `fd -g` habit: a name glob.
    assert_eq!(
        parse("structureSearch", &["files", "src", "-g", "fs_*"])
            .unwrap()
            .map(|q| q["include"].clone()),
        Some(json!(["fs_*"]))
    );
    // Free text keeps its commas: a glob brace is one value.
    assert_eq!(
        parse("localSearch", &["x", "-g", "*.{ts,tsx}"])
            .unwrap()
            .map(|q| q["include"].clone()),
        Some(json!(["*.{ts,tsx}"]))
    );
    assert!(
        parse("localSearch", &["x", "src", "-l", "rust"])
            .is_err_and(|e| e.contains("for a language use -t rust"))
    );
    // Help names a switch once, with no `...`.
    let mut command = augment(clap::Command::new("localSearch"), "localSearch");
    let help = command.render_help().to_string();
    assert!(
        help.contains("-F literal, -E rust, -P pcre2") && !help.contains("-F..."),
        "{help}"
    );
    assert!(
        !help.contains("--snapshot") && !help.contains("--page "),
        "{help}"
    );
}

#[test]
fn no_flags_and_no_arguments_is_the_json_form() {
    assert_eq!(parse("localSearch", &[]).unwrap(), None);
    assert!(
        parse("localSearch", &["a", "b", "c"])
            .is_err_and(|e| e.contains("unexpected argument 'c' found"))
    );
}
