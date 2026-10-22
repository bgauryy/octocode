//! Empty and partial results: the probes, spelling variants and leads a
//! page offers when its query found nothing or less than it asked.
use super::*;
use octocode_contract_types::ErrorCode;

/// Without `regex`, text is literal, so `.unwrap()`, an empty group `()`,
/// `x[0]`, `.get(0)?` and `a|b` find only that text, and an explicit `regex`
/// always wins. An empty literal search mentions the regex reading; a page
/// with hits does only for an alternation that matches more lines
/// ([`a_literal_alternation_with_hits_names_a_wider_regex_reading`]).
#[test]
fn an_unset_regex_reads_every_text_literally() {
    let files = [("a.rs", "x.unwrap();\nunwrap_or(1);\nfoo_unwrap()\n")];
    let run = |fields: serde_json::Value| search_fixture(&files, ls_query(fields, None));
    let lines = |body: &serde_json::Value| body["matchedLineCount"].as_u64().unwrap_or(0);
    let warned = |body: &serde_json::Value, text: &str| {
        body["warnings"].as_array().is_some_and(|all| {
            all.iter()
                .any(|w| w.as_str().is_some_and(|w| w.contains(text)))
        })
    };
    let literal = run(serde_json::json!({"matchString": ".unwrap()"}));
    assert_eq!(lines(&literal), 1, "{literal}");
    assert!(!warned(&literal, "ran as literal text"), "{literal}");
    let missed = run(serde_json::json!({"matchString": ".unwrap_or(2)"}));
    assert!(warned(&missed, "ran as literal text"), "{missed}");
    // Code punctuation is not a regex: indexing and `?` find their text.
    let code = [(
        "b.rs",
        "let a = matches[0];\nlet b = v.get(0)?;\nlet c = x;\n",
    )];
    for text in ["matches[0]", ".get(0)?"] {
        let body = search_fixture(
            &code,
            ls_query(serde_json::json!({"matchString": text}), None),
        );
        assert_eq!(lines(&body), 1, "{text}: {body}");
    }
    assert!(literal["next"].get("searchLiteral").is_none(), "{literal}");
    let group = run(serde_json::json!({"matchString": "()"}));
    assert_eq!(lines(&group), 2, "{group}");
    // `a|b` is no exception: without `regex` it is literal text too.
    let alternation = run(serde_json::json!({"matchString": "unwrap_or|foo_"}));
    assert_eq!(lines(&alternation), 0, "{alternation}");
    assert!(warned(&alternation, "ran as literal text"), "{alternation}");
    // The regex reading is an executable retry, not only prose.
    let retry = &alternation["next"]["findSimilar"];
    let row = retry.pointer("/query/queries/0").unwrap_or(&retry["query"]);
    assert_eq!(row["regex"], "rust", "{alternation}");
    assert_eq!(row["matchString"], "unwrap_or|foo_", "{alternation}");
    let retried = run(row.clone());
    assert_eq!(lines(&retried), 2, "{retried}");
    let alternation = run(serde_json::json!({"matchString": "unwrap_or|foo_", "regex": "rust"}));
    assert_eq!(lines(&alternation), 2, "{alternation}");
    let explicit = run(serde_json::json!({"matchString": ".unwrap()", "regex": "rust"}));
    assert_eq!(lines(&explicit), 2, "{explicit}");
    let exact = run(serde_json::json!({"matchString": "unwrap_or|foo_", "regex": "literal"}));
    assert_eq!(lines(&exact), 0, "{exact}");
    for quiet in [
        &explicit,
        &exact,
        &run(serde_json::json!({"matchString": "unwrap"})),
    ] {
        assert!(
            !warned(quiet, "ran as literal text") && !warned(quiet, "ran as a regex"),
            "{quiet}"
        );
    }
}

/// The hint is not special to `|`: an empty literal search whose text holds
/// any regex metacharacter (the general case of an agent meaning a pattern)
/// warns, and its `findSimilar` lead is a runnable `regex:"rust"` query that
/// finds the text it was meant for.
#[test]
fn an_empty_literal_search_with_any_regex_character_leads_to_a_regex_run() {
    let files = [("a.txt", "abc\nfoo\nxyz\n123\nzz9\n")];
    for (text, found) in [
        ("a.c", 1),
        ("fo+", 1),
        ("^xy", 1),
        ("\\d+", 2),
        ("[xz]yz?$", 1),
        ("abc|xyz", 2),
        ("z{2}", 1),
    ] {
        let body = search_fixture(
            &files,
            ls_query(serde_json::json!({"matchString": text}), None),
        );
        assert_eq!(body["matchedLineCount"], 0, "{text}: {body}");
        let lead = &body["next"]["findSimilar"];
        let row = lead.pointer("/query/queries/0").unwrap_or(&lead["query"]);
        assert_eq!(row["regex"], "rust", "{text}: {body}");
        assert_eq!(row["matchString"], text, "{text}: {body}");
        let retried = search_fixture(&files, ls_query(row.clone(), None));
        assert_eq!(retried["matchedLineCount"], found, "{text}: {retried}");
    }
}

/// A literal page with hits whose text reads as an alternation (`a|b`)
/// warns, with the regex reading as an executable lead, only when one count
/// of that reading finds more matched lines; an explicit `regex:"literal"`
/// and an alternation with an empty side are taken as written.
#[test]
fn a_literal_alternation_with_hits_names_a_wider_regex_reading() {
    let warned = |body: &serde_json::Value| {
        body["warnings"].as_array().into_iter().flatten().any(|w| {
            w.as_str()
                .is_some_and(|w| w.contains("it matches") && w.contains("with regex:"))
        })
    };
    let wider = [("a.txt", "alpha\nbeta\nalpha|beta\n")];
    let body = search_fixture(
        &wider,
        ls_query(serde_json::json!({"matchString": "alpha|beta"}), None),
    );
    assert_eq!(body["matchedLineCount"], 1, "{body}");
    assert!(
        body["warnings"].as_array().into_iter().flatten().any(|w| w
            == "matchString ran as literal text (1 matched line); with regex:\"rust\" it matches 3."),
        "{body}"
    );
    let lead = &body["next"]["findSimilar"];
    assert_eq!(lead["query"]["queries"][0]["regex"], "rust", "{body}");
    let retried = search_fixture(&wider, ls_query(lead["query"]["queries"][0].clone(), None));
    assert_eq!(retried["matchedLineCount"], 3, "{retried}");
    for (files, fields) in [
        // The regex reading matches no more lines.
        (
            [("b.txt", "alpha|beta\nother\n")],
            serde_json::json!({"matchString": "alpha|beta"}),
        ),
        // Literal on purpose.
        (
            [("c.txt", "alpha\nalpha|beta\n")],
            serde_json::json!({"matchString": "alpha|beta", "regex": "literal"}),
        ),
        // An empty alternative is not an alternation of terms.
        (
            [("d.txt", "alpha|\nalpha\nother\n")],
            serde_json::json!({"matchString": "alpha|"}),
        ),
    ] {
        let body = search_fixture(&files, ls_query(fields, None));
        assert!(!warned(&body), "{body}");
        assert!(body["next"].get("findSimilar").is_none(), "{body}");
    }
}

/// An empty literal search offers the regex reading under the engine the
/// text compiles in (lookaround: pcre2), across lines when it matches a line
/// break, and every such lead runs and finds the line.
#[test]
fn a_literal_miss_leads_to_the_regex_reading_that_runs() {
    let files = [(
        "p.txt",
        "price 100 dollars\nprice 200 euros\nstart\nmiddle\n",
    )];
    let run = |fields: serde_json::Value| search_fixture(&files, ls_query(fields, None));
    for (text, engine, across_lines) in [
        ("price (?=\\d+ euros)", "pcre2", false),
        ("(?<=price )200", "pcre2", false),
        ("start\\nmiddle", "rust", true),
    ] {
        let missed = run(serde_json::json!({"matchString": text}));
        assert_eq!(
            missed["matchedLineCount"].as_u64().unwrap_or(0),
            0,
            "{missed}"
        );
        let retry = &missed["next"]["findSimilar"];
        let row = retry.pointer("/query/queries/0").unwrap_or(&retry["query"]);
        assert_eq!(row["regex"], engine, "{text}: {missed}");
        assert_eq!(row["multiline"] == "on", across_lines, "{text}: {missed}");
        let retried = run(row.clone());
        assert_eq!(retried["matchedLineCount"], 1, "{text}: {retried}");
    }
}

/// An empty page the `fileHas`/`fileLacks` terms caused says so, and its
/// `broadenSearch` lead (the same search without them) finds the matches.
#[test]
fn an_empty_page_the_file_terms_caused_names_them() {
    let files = [("a.txt", "needle\n"), ("b.txt", "needle\nother\n")];
    let run = |fields: serde_json::Value| search_fixture(&files, ls_query(fields, None));
    for filter in [
        serde_json::json!({"fileHas": ["absent"]}),
        serde_json::json!({"fileLacks": ["needle"]}),
    ] {
        let mut fields = serde_json::json!({"matchString": "needle"});
        fields
            .as_object_mut()
            .unwrap()
            .extend(filter.as_object().unwrap().clone());
        let empty = run(fields);
        assert_eq!(
            empty["matchedLineCount"].as_u64().unwrap_or(0),
            0,
            "{empty}"
        );
        assert!(
            empty["warnings"]
                .to_string()
                .contains("2 file(s) matched matchString but fileHas/fileLacks removed them"),
            "{empty}"
        );
        let lead = &empty["next"]["broadenSearch"];
        let row = lead.pointer("/query/queries/0").unwrap_or(&lead["query"]);
        assert!(
            row.get("fileHas").is_none() && row.get("fileLacks").is_none(),
            "{row}"
        );
        assert_eq!(run(row.clone())["matchedLineCount"], 2, "{row}");
    }
    let kept = run(serde_json::json!({"matchString": "needle", "fileHas": ["other"]}));
    assert_eq!(kept["matchedLineCount"], 1, "{kept}");
    assert!(kept["next"].get("broadenSearch").is_none(), "{kept}");
}

/// A case-sensitive miss (smart case on uppercase text, or `sensitive`)
/// whose text matches in another case says so, and its `ignoreCase` lead
/// finds it.
#[test]
fn a_case_only_miss_leads_to_ignore_case() {
    let files = [("a.ts", "const structuredContent = 1;\n")];
    let run = |fields: serde_json::Value| search_fixture(&files, ls_query(fields, None));
    for fields in [
        serde_json::json!({"matchString": "STRUCTUREDCONTENT"}),
        serde_json::json!({"matchString": "StructuredContent", "caseMode": "sensitive"}),
    ] {
        let empty = run(fields);
        assert_eq!(
            empty["matchedLineCount"].as_u64().unwrap_or(0),
            0,
            "{empty}"
        );
        assert!(
            empty["hints"]
                .to_string()
                .contains("matches ignoring case in 1 file(s)"),
            "{empty}"
        );
        let lead = &empty["next"]["ignoreCase"];
        let row = lead.pointer("/query/queries/0").unwrap_or(&lead["query"]);
        assert_eq!(row["caseMode"], "insensitive", "{empty}");
        assert_eq!(run(row.clone())["matchedLineCount"], 1, "{row}");
    }
}

/// A `maxDepth` cut is disclosed once with the same search at every depth;
/// an empty directory at the limit hides nothing and is not counted.
#[test]
fn a_max_depth_cut_is_disclosed_with_a_deeper_lead() {
    let files = [
        ("top.txt", "needle\n"),
        ("a/b/deep.txt", "needle\n"),
        ("a/c/x.txt", "other\n"),
    ];
    let run = |fields: serde_json::Value| search_fixture(&files, ls_query(fields, None));
    let cut = run(serde_json::json!({"matchString": "needle", "maxDepth": 1}));
    assert_eq!(cut["matchedLineCount"], 1, "{cut}");
    assert!(
        cut["warnings"]
            .to_string()
            .contains("1 dir(s) at maxDepth were not entered"),
        "{cut}"
    );
    let lead = &cut["next"]["expandDepth"];
    let row = lead.pointer("/query/queries/0").unwrap_or(&lead["query"]);
    assert!(row.get("maxDepth").is_none(), "{row}");
    assert_eq!(run(row.clone())["matchedLineCount"], 2, "{row}");
    let whole = run(serde_json::json!({"matchString": "needle", "maxDepth": 3}));
    assert!(whole["next"].get("expandDepth").is_none(), "{whole}");
}

/// An empty `filesWithout` row (every searched file matches) names skipped
/// paths only for files there that lack a match, never the matching files
/// it already searched.
#[test]
fn an_empty_files_without_row_probes_for_files_without_a_match() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir(root.path().join(".git")).expect("repository marker");
    // Every file the default walk skips matches too (the hidden `.gitignore`).
    fs::write(root.path().join(".gitignore"), "# the\nskip/\n").expect("gitignore");
    fs::write(root.path().join("a.txt"), "the\n").expect("a");
    let (policy, security) = policy_for(root.path());
    let path = root.path().to_string_lossy().into_owned();
    let run = |fields: serde_json::Value| {
        let request = ls_query(fields, None);
        execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
            .expect("search")
    };
    let row = serde_json::json!({"path": path, "matchString": "the", "resultView": "filesWithout"});
    let plain = run(row.clone());
    assert!(plain.files.is_empty());
    assert!(
        !plain.hints.join(" | ").contains("skipped paths"),
        "{:?}",
        plain.hints
    );
    assert!(
        plain
            .next
            .as_ref()
            .is_none_or(|next| next.get("includeSkipped").is_none())
    );
    fs::create_dir(root.path().join("skip")).expect("ignored dir");
    fs::write(root.path().join("skip/b.txt"), "zzz\n").expect("b");
    let found = run(row);
    let hints = found.hints.join(" | ");
    assert!(
        hints.contains("1 file(s) lack a match in skipped paths (ignored)"),
        "{hints}"
    );
}

/// An include whose files all sit in ignored paths is not a scope miss: the
/// row names the skip cause and its lifting lead, and neither claims that
/// include/exclude matched no file nor leads to a listing that skips them too.
#[test]
fn an_include_matching_only_ignored_files_names_the_skip_not_the_scope() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir(root.path().join(".git")).expect("repository marker");
    fs::write(root.path().join(".gitignore"), "repos/\n").expect("gitignore");
    fs::create_dir_all(root.path().join("repos/go")).expect("ignored dir");
    fs::write(root.path().join("repos/go/a.go"), "func (s *T) Run() {}\n").expect("go");
    fs::write(root.path().join("notes.txt"), "other\n").expect("text");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(),
            "matchString": "func (s *T)", "include": ["*.go"]}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    assert!(result.files.is_empty());
    let hints = result.hints.join(" | ");
    assert!(hints.contains("match in skipped paths"), "{hints}");
    assert!(
        !hints.contains("include/exclude matched no file"),
        "{hints}"
    );
    let next = result.next.as_ref().expect("lead");
    assert_eq!(
        next["includeSkipped"]["query"]["queries"][0]["includeIgnored"], true,
        "{next}"
    );
    assert!(next.get("viewTree").is_none(), "{next}");
}

/// An empty search whose text lives only in ignored or hidden files says
/// so, and its lead finds them.
#[test]
fn an_empty_search_names_matches_under_ignored_and_hidden_paths() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir(root.path().join(".git")).expect("repository marker");
    fs::write(root.path().join(".gitignore"), "logs/\n").expect("gitignore");
    fs::create_dir_all(root.path().join("logs")).expect("ignored dir");
    fs::create_dir_all(root.path().join(".notes")).expect("hidden dir");
    fs::write(root.path().join("logs/log.txt"), "needle-here\n").expect("fixture");
    fs::write(root.path().join(".notes/x.txt"), "needle-here\n").expect("fixture");
    fs::write(root.path().join("src.txt"), "other\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle-here"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    assert!(result.files.is_empty());
    // The probe's finding is the row's first hint (rows keep one).
    let hint = &result.hints[0];
    assert!(
        hint == "2 file(s) match in skipped paths (ignored, hidden).",
        "{hint}"
    );
    assert!(hint.len() <= 120, "{hint}");
    let body = serde_json::to_value(&result).expect("result");
    let entries = body["skippedPaths"]["entries"].as_array().expect("skips");
    for (path, cause, reason) in [
        ("logs", "ignored", ".gitignore:1 'logs/'"),
        (".notes", "hidden", "hidden .notes/"),
    ] {
        assert!(
            entries.iter().any(
                |entry| entry["path"] == skipped_source_path(root.path(), path)
                    && entry["cause"] == cause
                    && entry["reason"] == reason
            ),
            "{body}"
        );
    }
    let next = result.next.as_ref().expect("lead");
    let lead = &next["includeSkipped"];
    assert_eq!(
        lead["query"]["queries"][0]["includeIgnored"], true,
        "{lead}"
    );
    assert_eq!(lead["query"]["queries"][0]["includeHidden"], true, "{lead}");
    let retried: LocalSearchQuery =
        serde_json::from_value(lead["query"]["queries"][0].clone()).expect("lead query");
    let found = execute_local_search(&retried, &policy, &security, &NeverCancel, None, None, None)
        .expect("retry");
    assert_eq!(found.files.len(), 2, "{found:?}");
    // Nothing to find elsewhere: no probe hint and no lead.
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "absent-everywhere"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    assert!(result.next.is_none(), "{:?}", result.next);
    assert_eq!(result.hints.len(), 1, "{:?}", result.hints);
}

/// An empty search whose text lives only in a default-excluded directory
/// (`build/`) is not a proven absence: the disclosure names the directory and the
/// lead searches it with `defaultExcludes:false`.
#[test]
fn an_empty_search_names_default_excluded_directories_that_match() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir_all(root.path().join("build")).expect("build dir");
    fs::write(root.path().join("build/out.js"), "getElectronVersion()\n").expect("fixture");
    fs::write(root.path().join("src.js"), "other\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "getElectronVersion"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    assert!(result.files.is_empty());
    let hint = &result.hints[0];
    assert!(
        hint == "1 file(s) match in skipped paths (default-excluded).",
        "{hint}"
    );
    let body = serde_json::to_value(&result).expect("result");
    assert_eq!(
        body["skippedPaths"]["entries"],
        serde_json::json!([{"path":skipped_source_path(root.path(),"build"),"cause":"default-excluded","reason":"build/"}]),
        "{body}"
    );
    let lead = &result.next.as_ref().expect("lead")["includeSkipped"]["query"]["queries"][0];
    assert_eq!(lead["defaultExcludes"], false, "{lead}");
    let retried: LocalSearchQuery = serde_json::from_value(lead.clone()).expect("lead query");
    let found = execute_local_search(&retried, &policy, &security, &NeverCancel, None, None, None)
        .expect("retry");
    assert_eq!(found.files.len(), 1, "{found:?}");
}

/// A probe that runs out of time proves nothing about the ignored paths it
/// could not search: the row still names them and its lead lifts exactly the
/// ignore rules, under the key that says so. It is not a scope miss either.
#[test]
fn an_unproven_probe_names_the_ignored_paths_and_leads_with_include_ignored() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir(root.path().join(".git")).expect("repository marker");
    // In a git work tree a default-excluded name is pruned where git
    // ignores it.
    fs::write(root.path().join(".gitignore"), "repos/*\nbuild/\n").expect("gitignore");
    fs::create_dir_all(root.path().join("repos/go")).expect("ignored dir");
    fs::write(root.path().join("repos/go/a.go"), "func Run() {}\n").expect("go");
    fs::create_dir_all(root.path().join("build")).expect("build dir");
    fs::write(root.path().join("build/out.go"), "func Out() {}\n").expect("built");
    fs::write(root.path().join("notes.txt"), "other\n").expect("text");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(),
            "matchString": "func", "include": ["*.go"]}),
        None,
    );
    let result = crate::tools::empty_walk::with_probe_budget(std::time::Duration::ZERO, || {
        execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
            .expect("search")
    });
    assert!(result.files.is_empty());
    let hints = result.hints.join(" | ");
    let body = serde_json::to_value(&result).expect("result");
    assert_eq!(
        body["skippedPaths"]["entries"],
        serde_json::json!([{"path":skipped_source_path(root.path(),"repos/go"),"cause":"ignored","reason":".gitignore:1 'repos/*'"}]),
        "{body}"
    );
    assert!(
        result.hints.iter().all(|hint| hint.chars().count() <= 120),
        "{hints}"
    );
    assert!(!hints.contains("matched no file"), "{hints}");
    assert!(!hints.contains("No matches in"), "{hints}");
    let next = result.next.as_ref().expect("lead");
    let lead = &next["includeSkipped"]["query"]["queries"][0];
    assert_eq!(lead["includeIgnored"], true, "{next}");
    assert!(next.get("viewTree").is_none(), "{next}");
    let retried: LocalSearchQuery = serde_json::from_value(lead.clone()).expect("lead query");
    let found = execute_local_search(&retried, &policy, &security, &NeverCancel, None, None, None)
        .expect("retry");
    assert!(
        found.files.iter().any(|file| file.path.ends_with("a.go")),
        "{found:?}"
    );
    // No ignored path to name: an unproven probe says nothing about them.
    let plain = tempfile::tempdir().expect("fixture directory");
    fs::write(plain.path().join("notes.txt"), "other\n").expect("text");
    let (policy, security) = policy_for(plain.path());
    let request = ls_query(
        serde_json::json!({"path": plain.path().to_string_lossy().into_owned(), "matchString": "absent"}),
        None,
    );
    let result = crate::tools::empty_walk::with_probe_budget(std::time::Duration::ZERO, || {
        execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
            .expect("search")
    });
    assert!(result.next.is_none(), "{:?}", result.next);
    // A hidden path is named too, and the lead lifts it.
    fs::create_dir(plain.path().join(".hid")).expect("hidden dir");
    fs::write(plain.path().join(".hid/a.txt"), "absent\n").expect("hidden");
    let result = crate::tools::empty_walk::with_probe_budget(std::time::Duration::ZERO, || {
        execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
            .expect("search")
    });
    let hints = result.hints.join(" | ");
    assert!(
        hints.contains("1 hidden path(s) were not searched"),
        "{hints}"
    );
    let next = result.next.as_ref().expect("lead");
    let lead = &next["includeSkipped"]["query"]["queries"][0];
    assert_eq!(lead["includeHidden"], true, "{next}");
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
    assert!(hints.contains("match in skipped paths"), "{hints}");
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

/// The withheld notice lives once, in warnings; the empty hint does
/// not repeat it and gives no spelling advice.
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
    let text = body.to_string();
    assert_eq!(text.matches("withheld by path policy").count(), 1, "{body}");
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
