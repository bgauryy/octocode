//! Leads off an empty page: scanned counts, scope and path misses, walk
//! flags, per-file counts and spelling variants.

use super::*;

/// A plain miss states each fact once: `status` and `filesScanned` say it,
/// so no hint repeats the scanned count.
#[test]
fn a_plain_miss_states_the_scanned_count_once() {
    let body = search_fixture(
        &[("a.rs", "fn a() {}\n"), ("b.rs", "fn b() {}\n")],
        ls_query(serde_json::json!({"matchString": "zzqqxx"}), None),
    );
    assert_eq!(body["filesScanned"], 2, "{body}");
    assert!(body.get("hints").is_none(), "{body}");
}

/// Once the hint names the skipped paths that hold a match, the generic count
/// of searched files does not follow it: the cause leads the row alone.
#[test]
fn a_skipped_match_replaces_the_generic_empty_count() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir(root.path().join(".git")).expect("repository marker");
    fs::write(root.path().join(".gitignore"), "repos/\n").expect("gitignore");
    fs::create_dir_all(root.path().join("repos")).expect("ignored dir");
    fs::write(root.path().join("repos/a.go"), "func Run() {}\n").expect("go");
    fs::write(root.path().join("b.ts"), "export {}\n").expect("ts");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(),
            "matchString": "func", "include": ["*.go", "*.ts"]}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    let hints = result.hints.join(" | ");
    assert!(
        hints.contains("that matches behind skipped paths"),
        "{hints}"
    );
    assert!(!hints.contains("No matches in"), "{hints}");
}

/// An unset `regex` is literal for every text, regex-shaped or not, and
/// an empty literal search names the `regex` field that would run it as one.
#[test]
fn an_unset_regex_is_literal_even_for_regex_shaped_text() {
    for text in [
        "register.*Tool",
        "a|b",
        "\\bfoo",
        "x.+y",
        "pub fn [a-z_]+\\(",
        "^use ",
        "id_[0-9]",
    ] {
        let body = search_fixture(
            &[("a.rs", "register_x_Tool\nfn read\n")],
            ls_query(serde_json::json!({"matchString": text}), None),
        );
        assert_eq!(body["matchCount"], 0, "{text}: {body}");
        assert!(
            body["warnings"].to_string().contains("ran as literal text"),
            "{text}: {body}"
        );
    }
    let pattern = search_fixture(
        &[("a.rs", "register_x_Tool\n")],
        ls_query(
            serde_json::json!({"matchString": "register.*Tool", "regex": "rust"}),
            None,
        ),
    );
    assert_eq!(pattern["matchCount"], 1, "{pattern}");
}

/// What the policy withheld is stated once, in `skippedPaths`; the empty
/// hint does not repeat it and gives no spelling advice.
#[test]
fn withheld_notice_appears_once_on_empty_rows() {
    let body = search_fixture(
        &[
            ("src/credentials.json", "{\"ISecretX\": 1}\n"),
            ("src/b.ts", "export const other = 1;\n"),
        ],
        ls_query(serde_json::json!({"matchString": "ISecretX"}), None),
    );
    assert_eq!(body["matchCount"], 0, "{body}");
    let withheld = crate::tools::test_support::skipped_reasons(&body, "withheld");
    assert_eq!(withheld, [("credential file".into(), 1)], "{body}");
    let hint = body["hints"][0].as_str().expect("empty hint");
    assert!(hint.starts_with("No matches"), "{hint}");
    assert!(!hint.contains("shorter term"), "{hint}");
    assert!(hint.chars().count() <= 120, "{hint}");
}

/// A missing path leads to a tree of its nearest existing parent.
#[test]
fn missing_path_leads_to_nearest_parent_tree() {
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir_all(root.path().join("src/native")).expect("dirs");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().join("src/nativ").to_string_lossy(), "matchString": "x"}),
        None,
    );
    let error = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect_err("missing path");
    assert_eq!(error.code, ErrorCode::PathNotFound);
    let next = error.next.expect("viewTree lead");
    assert_eq!(next["viewTree"]["tool"], "structureSearch", "{next}");
    assert_eq!(
        next["viewTree"]["query"]["queries"][0]["path"], "src",
        "{next}"
    );
}

/// A scope-miss row leads to the scope's listing without the globs that
/// matched nothing, so it shows the names that exist.
#[test]
fn scope_miss_leads_to_the_files_listing_without_its_globs() {
    let body = search_fixture(
        &[("a.rs", "needle\n")],
        ls_query(
            serde_json::json!({"matchString": "needle", "include": ["*.zzz"]}),
            None,
        ),
    );
    assert_eq!(body["filesScanned"], 0, "{body}");
    let lead = &body["next"]["viewTree"];
    assert_eq!(lead["tool"], "structureSearch", "{body}");
    let listing = &lead["query"]["queries"][0];
    assert_eq!(listing["operation"], "files", "{body}");
    assert!(listing.get("include").is_none(), "{body}");
}

/// Each walk flag lifts its own cause, as in every local walk:
/// `includeIgnored` walks `.gitignore`d paths, `includeHidden` dot entries.
#[test]
fn each_walk_flag_lifts_only_its_own_cause() {
    use super::executor::search_options;
    use octocode_engine::search::walk::IgnorePolicy;
    let root = std::path::Path::new(".");
    let policy =
        |fields: serde_json::Value| search_options(&ls_query(fields, None), root, None).ignore;
    assert_eq!(policy(serde_json::json!({})), IgnorePolicy::default());
    assert_eq!(
        policy(serde_json::json!({"includeIgnored": true})),
        IgnorePolicy {
            include_ignored: true,
            include_hidden: false
        }
    );
    assert_eq!(
        policy(serde_json::json!({"includeHidden": true})),
        IgnorePolicy {
            include_ignored: false,
            include_hidden: true
        }
    );
    assert_eq!(
        policy(serde_json::json!({"includeIgnored": true, "includeHidden": true})),
        IgnorePolicy::ALL
    );
}

/// A line view with thousands of hits leads to the per-file counts first: one
/// call ranks the files instead of paging every hit. Small results, later
/// pages, and count views get no such lead.
#[test]
fn a_large_line_result_leads_to_per_file_counts() {
    let many = "needle\n".repeat(1_200);
    let body = search_fixture(
        &[("big.txt", &many), ("small.txt", "needle\n")],
        ls_query(serde_json::json!({"matchString":"needle"}), None),
    );
    let lead = &body["next"]["viewFileCount"];
    assert_eq!(lead["tool"], "localSearch", "{body}");
    let row = lead.pointer("/query/queries/0").unwrap_or(&lead["query"]);
    assert_eq!(row["resultView"], "countMatches", "{lead}");
    assert!(
        row.get("page").is_none() && row.get("snapshot").is_none(),
        "{lead}"
    );
    let hints = body["hints"].to_string();
    assert!(
        hints.contains("matches.") && !hints.contains("hints."),
        "{body}"
    );
    let small = search_fixture(
        &[("a.txt", "needle\n")],
        ls_query(serde_json::json!({"matchString":"needle"}), None),
    );
    assert!(small["next"].get("viewFileCount").is_none(), "{small}");
    let counts = search_fixture(
        &[("big.txt", &many)],
        ls_query(
            serde_json::json!({"matchString":"needle","resultView":"countMatches"}),
            None,
        ),
    );
    assert!(counts["next"].get("viewFileCount").is_none(), "{counts}");
}

/// The spelling probe walks what the query (and its lead) walks: a variant
/// only in a default-excluded file is no reason to offer a lead that finds
/// nothing.
#[test]
fn spelling_variants_in_default_excluded_files_are_not_counted() {
    let files = [("dist.min.js", "parse_config()\n"), ("src/a.py", "x = 1\n")];
    let body = search_fixture(
        &files,
        ls_query(serde_json::json!({"matchString":"parseConfig"}), None),
    );
    assert!(body["next"].get("findSimilar").is_none(), "{body}");
    assert!(
        !body["hints"].to_string().contains("other spellings"),
        "{body}"
    );
}

/// A compound identifier that matches nothing is probed in its other case
/// styles: when `parse_config` exists, `parseConfig` leads to it instead of
/// reading as absent. Single words and hits get no such lead.
#[test]
fn a_missed_identifier_leads_to_its_spelling_variants() {
    let files = [("src/cfg.py", "def parse_config(path):\n    return path\n")];
    let body = search_fixture(
        &files,
        ls_query(serde_json::json!({"matchString":"parseConfig"}), None),
    );
    let lead = &body["next"]["findSimilar"];
    assert_eq!(lead["tool"], "localSearch", "{body}");
    let row = lead.pointer("/query/queries/0").unwrap_or(&lead["query"]);
    let pattern = row["matchString"].as_str().unwrap_or_default();
    assert!(pattern.contains("parse_config"), "{row}");
    assert_eq!(row["regex"], "rust", "{row}");
    let hints = body["hints"].to_string();
    assert!(
        hints.contains("other spellings match in 1 file") && !hints.contains("hints."),
        "{body}"
    );
    // The variant lead runs and finds the file.
    let run = search_fixture(&files, ls_query(row.clone(), None));
    assert_eq!(run["files"][0]["path"], "src/cfg.py", "{run}");
    for miss in ["zzzqqq", "parse_config_missing_entirely"] {
        let none = search_fixture(
            &files,
            ls_query(serde_json::json!({"matchString": miss}), None),
        );
        assert!(none["next"].get("findSimilar").is_none(), "{miss}: {none}");
    }
}

/// An identifier that matches nothing because of a typo (a swap, a
/// missing, extra or wrong char) leads to the spellings one edit away that
/// exist, named in the hint and searched as whole words by the lead: a
/// longer name holding the typo's text is not one of them.
#[test]
fn a_typo_leads_to_the_spelling_one_edit_away() {
    let files = [(
        "src/run.rs",
        "pub fn execute_local_search() {}\nfn fingerprint() {}\nlet xexecute_local_search_all = 1;\n",
    )];
    for (typo, found) in [
        ("exeucte_local_search", "execute_local_search"),
        ("fingerprnt", "fingerprint"),
        ("fingerprints", "fingerprint"),
        ("fingerpront", "fingerprint"),
    ] {
        let body = search_fixture(
            &files,
            ls_query(serde_json::json!({"matchString": typo}), None),
        );
        let hints = body["hints"].to_string();
        assert!(
            hints.contains(&format!("near spellings match in 1 file(s): {found}.")),
            "{typo}: {body}"
        );
        let lead = &body["next"]["findSimilar"];
        let row = lead.pointer("/query/queries/0").unwrap_or(&lead["query"]);
        assert_eq!(row["matchString"], found, "{row}");
        assert_eq!(row["wholeWord"], true, "{row}");
        let run = search_fixture(&files, ls_query(row.clone(), None));
        assert_eq!(run["files"][0]["path"], "src/run.rs", "{run}");
        assert_eq!(
            run["files"][0]["matches"].as_array().map(Vec::len),
            Some(1),
            "{run}"
        );
    }
    // Two edits away, or too short to probe: no lead.
    for miss in ["exeuctelocal_search", "fngrprint", "fnx"] {
        let none = search_fixture(
            &files,
            ls_query(serde_json::json!({"matchString": miss}), None),
        );
        assert!(none["next"].get("findSimilar").is_none(), "{miss}: {none}");
    }
}

/// One-edit spellings: deletions, swaps, substitutions and insertions,
/// sorted and distinct, never the term, always identifiers.
#[test]
fn one_edit_spellings_cover_each_edit_once() {
    let spellings = super::probes::one_edit_spellings("abcd").expect("identifier");
    for expected in ["acd", "bacd", "abdd", "abxcd", "abcde", "_abcd"] {
        assert!(
            spellings.iter().any(|s| s == expected),
            "{expected}: {spellings:?}"
        );
    }
    assert!(!spellings.iter().any(|s| s == "abcd"));
    assert!(
        !spellings
            .iter()
            .any(|s| s.starts_with(|c: char| c.is_ascii_digit()))
    );
    assert!(spellings.windows(2).all(|pair| pair[0] < pair[1]));
    // Uppercase is substituted only into a term that has some.
    assert!(!spellings.iter().any(|s| s == "aBcd"));
    let camel = super::probes::one_edit_spellings("abCd").expect("identifier");
    assert!(camel.iter().any(|s| s == "aBCd"), "{camel:?}");
    for none in ["abc", "a.bcd", "9abc", "ab-cd"] {
        assert!(super::probes::one_edit_spellings(none).is_none(), "{none}");
    }
}

/// Case-style variants of a compound identifier, excluding the term itself.
#[test]
fn identifier_variants_cover_the_common_case_styles() {
    let variants = super::probes::identifier_variants("parseConfig").expect("compound");
    for expected in [
        "parse_config",
        "ParseConfig",
        "PARSE_CONFIG",
        "parse-config",
    ] {
        assert!(
            variants.iter().any(|v| v == expected),
            "{expected}: {variants:?}"
        );
    }
    assert!(!variants.iter().any(|v| v == "parseConfig"));
    assert!(
        super::probes::identifier_variants("needle").is_none(),
        "one word"
    );
    assert!(
        super::probes::identifier_variants("a.b").is_none(),
        "not an identifier"
    );
    let from_snake = super::probes::identifier_variants("HTTP_server_error").expect("snake");
    assert!(
        from_snake.iter().any(|v| v == "httpServerError"),
        "{from_snake:?}"
    );
}

/// A localFetch line page counts bytes, and so does the long-line test: a
/// clipped hit on a line of 6000 three-byte characters (18 KB, under the
/// page in characters) is read by a byte window at the hit, and a long
/// ASCII line under the page in bytes by its line.
#[test]
fn a_line_past_the_byte_page_is_read_at_the_hit_whatever_its_characters() {
    let read_of = |line: String| {
        let body = search_fixture(
            &[("a.txt", &format!("{line} needle\n"))],
            ls_query(serde_json::json!({"matchString": "needle"}), None),
        );
        let read = &body["next"]["read"];
        read.pointer("/query/queries/0")
            .unwrap_or(&read["query"])
            .clone()
    };
    let wide = read_of("中".repeat(6_000));
    assert_eq!(wide["unit"], "bytes", "{wide}");
    assert_eq!(wide["offset"], 18_001, "{wide}");
    let ascii = read_of("x".repeat(10_000));
    assert!(ascii.get("unit").is_none(), "{ascii}");
}
