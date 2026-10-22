//! Contract and transport tests for response rows.

#[test]
fn envelope_unions_keep_each_data_requirement_combination() {
    let schema = json!({
        "$defs":{"rows":{"type":"array","items":{"properties":{"data":{
            "allOf":[
                {"required":["base"]},
                {"oneOf":[{"required":["left"]},{"required":["right"]}]}
            ]
        }}}}},
        "anyOf":[
            {"required":["error","errorCode"]},
            {"properties":{"results":{"$ref":"#/$defs/rows"}}}
        ]
    });
    let mut variants = data_variants_of(&schema)
        .into_iter()
        .map(|fields| fields.into_iter().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    variants.sort();
    assert_eq!(variants, vec![vec!["base", "left"], vec!["base", "right"]]);
}

#[test]
fn minimal_rows_keep_every_field_the_contract_requires() {
    let history = contract_data_variants(ToolId::GhSearchHistory);
    assert!(
        history
            .iter()
            .all(|variant| variant.contains("pullRequests")
                || variant.contains("issues")
                || variant.contains("commits")
                || variant.contains("error")),
        "{history:?}"
    );
    // A clone row requires its checkout, not the caller's own owner/repo.
    let clone = contract_data_variants(ToolId::GhCloneRepo);
    assert!(
        clone
            .iter()
            .any(|variant| variant.contains("location") && variant.contains("bytes")),
        "{clone:?}"
    );
    let row = minimized(
        ToolId::GhSearchHistory,
        json!({"operation":"issue","owner":"o","repo":"r"}),
        json!({"owner":"o","repo":"r","issues":[],"effectiveQuery":"is:issue"}),
    );
    assert_eq!(row["data"]["issues"], json!([]), "{row}");
    assert!(row["data"].get("effectiveQuery").is_none(), "{row}");
    let clone_row = minimized(
        ToolId::GhCloneRepo,
        json!({"owner":"o","repo":"r"}),
        json!({"bytes":1,"location":{"localPath":"/x"}}),
    );
    assert_eq!(
        clone_row["data"]["location"]["localPath"], "/x",
        "{clone_row}"
    );
    assert_eq!(clone_row["data"]["bytes"], 1, "{clone_row}");
}

/// The last page of a window read keeps its pagination: its offset
/// places the text, and a byte unit keeps that text verbatim.
#[test]
fn a_later_window_page_keeps_its_pagination() {
    let page = json!({"unit":"bytes","offset":8000,"length":12,"hasMore":false});
    let last = minimized(
        ToolId::GhGetFileContent,
        json!({}),
        json!({"path":"a","content":"x","pagination":page.clone()}),
    );
    assert_eq!(last["data"]["pagination"], page, "{last}");
    let whole = minimized(
        ToolId::LocalFetch,
        json!({}),
        json!({"path":"a","content":"x","pagination":{"unit":"lines","offset":0,"length":1,"hasMore":false}}),
    );
    assert!(whole["data"].get("pagination").is_none(), "{whole}");
}

/// Every tool's pagination block keeps the same keys on every page
/// (where it stands, its size, what is known of the total, whether more
/// exists) and drops only what `next` carries or what asserts nothing.
#[test]
fn pagination_blocks_keep_stable_keys_on_every_tool() {
    let next =
        json!({"nextPage":{"tool":"ghSearchCode","query":{"queries":[{"owner":"o","page":2}]}}});
    for tool in [ToolId::GhSearchCode, ToolId::LocalSearch, ToolId::LspSearch] {
        // The block is a stats extra; asked for, its keys are stable.
        let row = minimized(
            tool,
            json!({"extras":["stats"]}),
            json!({"files":[{"path":"a"}],"next":next.clone(),"pagination":{"currentPage":1,"totalPages":2,"pageSize":5,"totalItems":7,
                "totalItemsCapped":false,"hasMore":true,"nextPage":2,"snapshot":"s","resultId":"r"}}),
        );
        assert_eq!(
            row["data"]["pagination"],
            json!({"currentPage":1,"totalPages":2,"pageSize":5,"totalItems":7,"hasMore":true}),
            "{tool}"
        );
    }
    let capped = minimized(
        ToolId::GhSearchRepo,
        json!({"extras":["stats"]}),
        json!({"repositories":[],"pagination":{"totalItems":1000,"totalItemsCapped":true,"hasMore":true,"currentPage":3},
            "providerLimit":{"reason":"providerResultCap","maxResults":1000}}),
    );
    assert_eq!(
        capped["data"]["pagination"],
        json!({"totalItems":1000,"hasMore":true,"currentPage":3}),
        "a true cap repeats providerLimit"
    );
}

fn minimized(tool: ToolId, query: Value, data: Value) -> Value {
    let mut row = json!({"index":0,"data":data,"cache":1});
    // The engine order: the extras stage, then the minimizer.
    super::super::extras::prune_row(&mut row, tool, &query);
    minimize_row(&mut row, tool, &query);
    row
}

/// Which rows carry scan totals is the contract's `scanTotals` for the tool,
/// not a name or trait branch: every localSearch row, an astSearch row only
/// beside `matchCount`, and no row of a tool without the marker.
#[test]
fn scan_totals_follow_the_contract_marker() {
    let totals = |tool, data: Value| reports_scan_totals(tool, data.as_object().expect("row"));
    assert!(totals(ToolId::LocalSearch, json!({"filesScanned": 3})));
    assert!(totals(ToolId::AstSearch, json!({"matchCount": 1})));
    assert!(!totals(ToolId::AstSearch, json!({"filesScanned": 3})));
    assert!(!totals(ToolId::StructureSearch, json!({"matchCount": 1})));
}

#[test]
fn minimal_rows_drop_what_asserts_nothing_and_keep_what_continues() {
    let row = minimized(
        ToolId::LocalSearch,
        json!({"path":"/r","matchString":"x"}),
        json!({"files":[{"path":"a.rs"}],"matchCount":1,"filesScanned":9,
            "pagination":{"currentPage":1,"totalPages":1,"hasMore":false},
            "snapshot":"s",
            "diagnostics":[{"severity":"info","message":"routine"},{"severity":"warning","message":"keep"}]}),
    );
    assert!(row.get("cache").is_none(), "{row}");
    let data = &row["data"];
    // The occurrence total stays on every view; scan accounting goes.
    assert_eq!(data["matchCount"], 1, "{row}");
    for gone in ["filesScanned", "pagination", "snapshot"] {
        assert!(data.get(gone).is_none(), "{gone}: {row}");
    }
    assert_eq!(
        data["diagnostics"],
        json!([{"severity":"warning","message":"keep"}])
    );
    assert_eq!(data["files"][0]["path"], "a.rs");
}

/// The parent list and emails are `extras: ["details"]`; a committer
/// row field the tool emitted (it differs from the author) stays.
/// A failed row states the error and what is evidence; empty arrays, a
/// finished single page and the snapshot assert nothing there.
#[test]
fn terminal_error_rows_drop_empty_single_page_and_snapshot_fields() {
    for tool in [ToolId::AstSearch, ToolId::LocalSearch, ToolId::GhSearchCode] {
        let mut row = json!({"index":0,"status":"error","data":{
            "error":"boom","errorCode":"executionFailed","path":"a.rs",
            "nodes":[],"files":[],"snapshot":"s",
            "pagination":{"currentPage":1,"totalPages":1,"pageSize":100,"totalItems":0,"hasMore":false},
            "diagnostics":[{"severity":"error","message":"kept evidence"}],
            "warnings":["kept"]}});
        minimize_row(&mut row, tool, &json!({"path":"a.rs"}));
        let data = row["data"].as_object().expect("data");
        let mut keys: Vec<&str> = data.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["diagnostics", "error", "errorCode", "path", "warnings"],
            "{tool:?}: {row}"
        );
    }
}

/// An error row that still has a page to walk keeps its pagination.
#[test]
fn terminal_error_rows_keep_an_open_pagination() {
    let mut row = json!({"index":0,"status":"error","data":{
        "error":"boom","errorCode":"executionFailed",
        "pagination":{"currentPage":1,"totalPages":3,"hasMore":true}}});
    minimize_row(&mut row, ToolId::LocalSearch, &json!({"path":"a"}));
    assert_eq!(row["data"]["pagination"]["hasMore"], true, "{row}");
}

/// An error row is not a partial result: the page it offers (a bounded
/// read of a file too large to read whole) is the way past the error.
#[test]
fn an_error_row_with_a_page_is_not_partial() {
    let query = json!({"path":"big.log","matchString":"needle"});
    let data = json!({"error":"too large","errorCode":"largeSourceWindowOnly","path":"big.log",
        "next":{"readBoundedLines":{"tool":"localFetch","query":{"queries":[
            {"path":"big.log","matchString":"needle","offset":0}]}}}});
    let row = result_row(ToolId::LocalFetch, 0, &query, data, Some("error"));
    assert_eq!(row["status"], "error", "{row}");
    assert!(
        row["data"]
            .get("isPartial")
            .is_none_or(|flag| flag == false),
        "{row}"
    );
}

/// The parent list and emails are `extras: ["details"]`; a committer
/// row field the tool emitted (it differs from the author) stays.
#[test]
fn commit_rows_keep_a_differing_committer_and_detail_the_parents() {
    let author = json!({"name":"Ada","email":"ada@x.dev","date":"2024-01-01T00:00:00Z"});
    let commit = |committer: Value, extras: Value| {
        minimized(
            ToolId::GhGetHistoryItem,
            json!({"operation":"commit","owner":"o","repo":"r","ref":"abc","extras":extras}),
            json!({"type":"commit","sha":"abc","message":"m","author":author,
                "committer":committer,"parents":["p1","p2"]}),
        )
    };
    let mut other_email = author.clone();
    other_email["email"] = json!("ada@y.dev");
    let same = commit(other_email, json!([]));
    assert!(same["data"]["committer"].get("email").is_none(), "{same}");
    assert!(same["data"].get("parents").is_none(), "{same}");
    assert!(same["data"]["author"].get("email").is_none(), "{same}");
    let detailed = commit(author.clone(), json!(["details"]));
    assert_eq!(
        detailed["data"]["parents"],
        json!(["p1", "p2"]),
        "{detailed}"
    );
    assert_eq!(
        detailed["data"]["author"]["email"], "ada@x.dev",
        "{detailed}"
    );
    let mut rebased = author.clone();
    rebased["date"] = json!("2024-02-01T00:00:00Z");
    let kept = commit(rebased, json!([]));
    assert_eq!(
        kept["data"]["committer"]["date"], "2024-02-01T00:00:00Z",
        "{kept}"
    );
}

#[test]
fn open_pages_keep_pagination_and_totals() {
    let row = minimized(
        ToolId::LocalSearch,
        json!({}),
        json!({"files":[{"path":"a.rs"}],"matchCount":40,"matchedLineCount":31,"filesScanned":9,"bytesSearched":7,
            "pagination":{"currentPage":1,"hasMore":true},"next":{"nextPage":{"tool":"localSearch","query":{"queries":[{"snapshot":"s"}]}}}}),
    );
    let data = &row["data"];
    // An open page shows its
    // position by default, beside the page in `next`.
    assert_eq!(
        data["pagination"],
        json!({"currentPage":1,"hasMore":true}),
        "{data}"
    );
    // Occurrences and matched lines (the rows a walk shows) are both
    // named, so the two totals never read as one disagreeing count.
    assert_eq!(data["matchCount"], 40, "{data}");
    assert_eq!(data["matchedLineCount"], 31, "{data}");
    for gone in ["filesScanned", "bytesSearched"] {
        assert!(data.get(gone).is_none(), "{gone}: {data}");
    }
    assert_eq!(
        data["next"]["nextPage"]["query"]["queries"][0]["snapshot"],
        "s"
    );
    // A later page stays located without any extra.
    let later = minimized(
        ToolId::LocalSearch,
        json!({}),
        json!({"files":[],"pagination":{"currentPage":3,"hasMore":false}}),
    );
    assert_eq!(
        later["data"]["pagination"]["currentPage"], 3,
        "last page stays located"
    );
}

#[test]
fn empty_searches_keep_their_scan_scope() {
    let row = minimized(
        ToolId::LocalSearch,
        json!({}),
        json!({"matchCount":0,"filesScanned":176,"bytesSearched":9}),
    );
    assert_eq!(row["data"]["matchCount"], 0, "{row}");
    assert_eq!(row["data"]["filesScanned"], 176, "{row}");
    assert!(row["data"].get("bytesSearched").is_none(), "{row}");
}

/// countLines states no `matchCount` and filesWithout neither total; their
/// pages minimize like every other view: rows drop the scan scope, an empty
/// page keeps it.
#[test]
fn views_without_an_occurrence_total_minimize_like_every_view() {
    let lines = minimized(
        ToolId::LocalSearch,
        json!({"resultView":"countLines"}),
        json!({"files":[{"path":"a.rs","matchCount":2}],"matchedLineCount":2,"filesScanned":9}),
    );
    assert_eq!(lines["data"]["matchedLineCount"], 2, "{lines}");
    assert!(lines["data"].get("matchCount").is_none(), "{lines}");
    assert!(lines["data"].get("filesScanned").is_none(), "{lines}");
    let without = minimized(
        ToolId::LocalSearch,
        json!({"resultView":"filesWithout"}),
        json!({"files":[{"path":"b.rs"}],"filesScanned":9}),
    );
    assert_eq!(without["data"]["files"][0]["path"], "b.rs", "{without}");
    assert!(without["data"].get("filesScanned").is_none(), "{without}");
    let empty = minimized(
        ToolId::LocalSearch,
        json!({"resultView":"filesWithout"}),
        json!({"files":[],"filesScanned":9}),
    );
    assert_eq!(empty["data"]["filesScanned"], 9, "{empty}");
}

#[test]
fn capped_searches_keep_limit_diagnostics_on_first_final_and_empty_pages() {
    for (files, more) in [
        (json!([{ "path": "a.rs" }]), true),
        (json!([{ "path": "a.rs" }]), false),
        (json!([]), false),
    ] {
        let row = minimized(
            ToolId::LocalSearch,
            json!({}),
            json!({
                "files": files,
                "partialReasons": ["maxCollectedFiles"], "matchCount": 10002,
                "filesScanned": 10002, "bytesSearched": 70014,
                "pagination": {"currentPage": if more { 1 } else { 20 }, "hasMore": more},
                "isPartial": true,
                "terminalLimit": !more,
            }),
        );
        assert_eq!(
            row["data"]["partialReasons"],
            json!(["maxCollectedFiles"]),
            "{row}"
        );
        assert_eq!(row["data"]["isPartial"], true, "{row}");
        assert_eq!(row["data"]["matchCount"], 10002, "{row}");
        assert_eq!(row["data"]["terminalLimit"] == true, !more, "{row}");
        assert!(row["data"].get("bytesSearched").is_none(), "{row}");
        crate::contracts::validate_output("localSearch", &json!({"results": [row]}))
            .expect("minimized cap diagnostics obey the canonical output schema");
    }
}

/// The unreadable-path warning names `fileErrors.first`: the failure
/// count and first failure stay public on every page shape.
#[test]
fn unreadable_paths_keep_their_error_stats_on_every_page() {
    for (files, more) in [
        (json!([{ "path": "a.rs" }]), true),
        (json!([{ "path": "a.rs" }]), false),
        (json!([]), false),
    ] {
        let row = minimized(
            ToolId::LocalSearch,
            json!({}),
            json!({
                "files": files,
                "matchCount": 1, "filesScanned": 2, "bytesSearched": 9,
                "fileErrors": {"count": 1,
                    "first": "/r/b.txt: Permission denied (os error 13)"},
                "pagination": {"currentPage": 1, "hasMore": more},
                "warnings": ["1 path(s) could not be read (see fileErrors.first), so absence is not proven."],
                "isPartial": true,
                "terminalLimit": !more,
            }),
        );
        let errors = &row["data"]["fileErrors"];
        assert_eq!(errors["count"], 1, "{row}");
        assert_eq!(
            errors["first"], "/r/b.txt: Permission denied (os error 13)",
            "{row}"
        );
        assert!(row["data"].get("bytesSearched").is_none(), "{row}");
        crate::contracts::validate_output("localSearch", &json!({"results": [row]}))
            .expect("minimized error stats obey the canonical output schema");
    }
}

#[test]
fn a_complete_page_that_keeps_binary_skipped_keeps_the_required_match_count() {
    let row = minimized(
        ToolId::LocalSearch,
        json!({}),
        json!({
            "files": [{ "path": "a.rs", "matches": [{"line": 1, "value": "fn parse()"}] }],
            "matchCount": 1, "matchedLineCount": 1,
            "filesScanned": 3, "bytesSearched": 9, "binarySkipped": 2,
            "pagination": {"currentPage": 1, "hasMore": false},
        }),
    );
    let data = &row["data"];
    assert_eq!(data["binarySkipped"], 2, "{row}");
    assert_eq!(data["matchCount"], 1, "{row}");
    assert!(data.get("bytesSearched").is_none(), "{row}");
    crate::contracts::validate_output("localSearch", &json!({"results": [row]}))
        .expect("surviving totals obey the canonical output schema");
}

#[test]
fn read_rows_leave_byte_accounting_to_stats_extras() {
    let fetch = minimized(
        ToolId::LocalFetch,
        json!({}),
        json!({"content":"x","modified":"t","sourceBytes":1,"totalLines":9}),
    );
    assert_eq!(fetch["data"], json!({"content":"x","totalLines":9}));
    // A GitHub read row is the same read: its byte accounting is a stats extra too.
    let file = json!({"path":"a","content":"x","sourceBytes":1,"returnedBytes":1,"selectedMatchCount":1,"totalLines":9});
    let gh = minimized(ToolId::GhGetFileContent, json!({}), file.clone());
    assert_eq!(gh["data"], json!({"path":"a","content":"x","totalLines":9}));
    let stats = minimized(
        ToolId::GhGetFileContent,
        json!({"extras":["stats"]}),
        file.clone(),
    );
    assert_eq!(stats["data"], file);
}

/// `extras: ["stats"]` returns the whole statistics block, also on a
/// complete page where the default keeps only the match totals.
#[test]
fn the_stats_extra_keeps_full_stats() {
    let totals = json!({"matchCount":1,"matchedLineCount":1,
        "filesScanned":24,"bytesSearched":442552});
    let mut data = json!({"path":"/r","files":[{"path":"a","matches":[{"line":1}]}]});
    data.as_object_mut()
        .expect("object")
        .extend(totals.as_object().expect("object").clone());
    let row = minimized(
        ToolId::LocalSearch,
        json!({"path":"/r","matchString":"x","extras":["stats"]}),
        data.clone(),
    );
    for (key, value) in totals.as_object().expect("object") {
        assert_eq!(&row["data"][key], value, "{key}: {row}");
    }
    let default = minimized(
        ToolId::LocalSearch,
        json!({"path":"/r","matchString":"x"}),
        data,
    );
    // The occurrence and matched-line totals show on every view with
    // rows; only the scan accounting is a stats extra.
    assert_eq!(default["data"]["matchCount"], 1, "{default}");
    assert_eq!(default["data"]["matchedLineCount"], 1, "{default}");
    for gone in ["filesScanned", "bytesSearched"] {
        assert!(default["data"].get(gone).is_none(), "{gone}: {default}");
    }
}

#[test]
fn echoes_go_only_when_they_equal_the_request_and_echo_extras_keep_them() {
    let data = json!({"owner":"o","repo":"other","path":"a.rs"});
    let row = minimized(
        ToolId::GhGetFileContent,
        json!({"owner":"o","repo":"r"}),
        data.clone(),
    );
    assert!(row["data"].get("owner").is_none());
    assert_eq!(
        row["data"]["repo"], "other",
        "a different repo is information"
    );
    let echo = minimized(
        ToolId::GhGetFileContent,
        json!({"owner":"o","extras":["echo"]}),
        data.clone(),
    );
    assert_eq!(echo["data"], data);
    // Echo keeps request echoes only: snapshots and finished stats that
    // restate nothing of the request still go.
    let echo = minimized(
        ToolId::LocalSearch,
        json!({"path":"/r","matchString":"x","extras":["echo"]}),
        json!({"path":"/r","snapshot":"s","files":[{"path":"a","matches":[{"line":1}]}],
            "matchCount":1,"bytesSearched":9}),
    );
    assert!(echo["data"].get("snapshot").is_none(), "{echo}");
    assert!(echo["data"].get("bytesSearched").is_none(), "{echo}");
    // A finished single page drops its pagination by default and
    // states it (`hasMore:false`) when the caller asks for echoes.
    let finished = json!({"commits":[{"commitSha":"a"}],
        "pagination":{"currentPage":1,"pageSize":100,"hasMore":false}});
    let query = json!({"operation":"commit","owner":"o","repo":"r","pageSize":100});
    let default = minimized(ToolId::GhSearchHistory, query.clone(), finished.clone());
    assert!(default["data"].get("pagination").is_none(), "{default}");
    let mut asked = query;
    asked["extras"] = json!(["echo"]);
    let echo = minimized(ToolId::GhSearchHistory, asked, finished.clone());
    assert_eq!(echo["data"]["pagination"], finished["pagination"], "{echo}");
    let mut error = json!({"index":0,"status":"error","data":{"error":"x","snapshot":"s"}});
    minimize_row(&mut error, ToolId::LocalSearch, &json!({}));
    assert!(
        error["data"].get("snapshot").is_none(),
        "an error row has no pages to pin: {error}"
    );
    assert_eq!(error["data"]["error"], "x", "{error}");
}

use super::*;

/// A cancelled call has one recovery on every tool: retry only when the
/// result is still needed and what cancelled it changed, else report it.
#[test]
fn a_cancellation_bounds_its_retry_and_otherwise_reports() {
    for tool in [ToolId::LocalSearch, ToolId::Clasify, ToolId::GhSearchCode] {
        assert_eq!(error_code_hint(tool, "cancelled"), Some(CANCELLED_HINT));
    }
    for slot in ["still needed", "changed", "otherwise report"] {
        assert!(CANCELLED_HINT.contains(slot), "{slot}: {CANCELLED_HINT}");
    }
    assert!(CANCELLED_HINT.chars().count() <= MAX_GUIDANCE_CHARS);
}

#[test]
fn relative_row_paths_drop_dot_segments_only() {
    let row = |path: &str| json!({"data": {"path": path, "matches": [{"path": path}]}});
    let value = envelope(
        ToolId::LocalSearch,
        vec![row("./src/a.ts"), row("src/./b/../c.ts"), row(".")],
    );
    let paths = value["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["data"]["path"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["src/a.ts", "src/b/../c.ts", "."]);
    assert_eq!(
        value["results"][0]["data"]["matches"][0]["path"],
        "src/a.ts"
    );
}

fn error_hint(tool: &str, query: &Value, code: &str, message: &str) -> String {
    let mut row = json!({
        "index": 0,
        "status": "error",
        "data": {"error": message, "errorCode": code}
    });
    apply_hint_policy(
        &mut row,
        ToolId::from_name(tool).expect("known tool"),
        query,
    );
    row["data"]["hints"][0]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn error_hints_match_exact_codes_not_substrings() {
    let query = json!({"owner":"o","keywords":["octocode"]});
    assert!(error_hint("ghSearchCode", &query, "rateLimited", "slow down").contains("Retry-After"));
    // A code merely containing "auth" is not an authentication failure.
    // The tool's fallback names its own advice.
    let fallback = error_hint("ghSearchCode", &query, "authorizationPending", "x");
    assert!(fallback.contains("unproven absence"), "{fallback}");
    // The sandbox hint keys on the dedicated code, never on message text.
    for tool in ["localSearch", "localFetch", "lspSearch"] {
        assert!(
            error_hint(
                tool,
                &json!({"path":"/etc"}),
                "outsideAllowedRoots",
                "denied"
            )
            .contains("ALLOWED_PATHS"),
            "{tool}"
        );
    }
    assert!(
        SANDBOX_HINT.contains("~/.octocode/.env")
            && SANDBOX_HINT.contains("not a workspace file")
            && SANDBOX_HINT.chars().count() <= MAX_GUIDANCE_CHARS,
        "the hint names the trusted place to widen roots and is delivered uncut: {SANDBOX_HINT}"
    );
    assert!(
        !error_hint(
            "localSearch",
            &json!({"path":"/etc"}),
            "fileAccessFailed",
            "Path '/etc' is outside allowed directories"
        )
        .contains("ALLOWED_PATHS")
    );
}

/// A workspace with `packages/app/{src,tests}` and an allowed root
/// outside it.
struct Workspace {
    _dirs: (tempfile::TempDir, tempfile::TempDir),
    root: std::path::PathBuf,
    outside: std::path::PathBuf,
    paths: PathPolicy,
}

fn workspace() -> Workspace {
    let dir = tempfile::tempdir().expect("fixture");
    let other = tempfile::tempdir().expect("fixture");
    let root = std::fs::canonicalize(dir.path()).expect("root");
    let outside = std::fs::canonicalize(other.path()).expect("outside");
    for file in ["src/a.rs", "src/runtime/b.rs", "tests/x.ts"] {
        let path = root.join("packages/app").join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(&path, "x").expect("file");
    }
    std::fs::write(outside.join("o.rs"), "x").expect("file");
    let paths = PathPolicy::new(crate::policy::path::PathPolicyConfig {
        workspace_root: Some(root.join("packages/app")),
        additional_roots: vec![outside.clone()],
        include_home: false,
        home_dir: None,
    })
    .expect("policy");
    Workspace {
        _dirs: (dir, other),
        root: root.join("packages/app"),
        outside,
        paths,
    }
}

/// Every relative row path must be what localFetch resolves: the
/// workspace root joined with the path.
fn assert_resolvable(ws: &Workspace, value: &Value, path: &str) {
    assert_eq!(value["root"], ws.root.to_string_lossy().as_ref(), "{value}");
    assert!(ws.paths.validate(path).is_ok(), "{path} in {value}");
}

/// Continuation queries name local paths relative to the workspace root
/// (`base`), which resolves them to the same files; outside paths stay.
#[test]
fn continuation_query_paths_are_workspace_relative() {
    let ws = workspace();
    let root = ws.root.to_string_lossy().into_owned();
    let outside = ws.outside.join("o.rs").to_string_lossy().into_owned();
    let row = json!({"index":0,"data":{
        "files":[{"path":format!("{root}/src/a.rs")}],
        "next":{"nextPage":{"tool":"localSearch","query":{"queries":[{"path":format!("{root}/src"),"matchString":"x","page":2}]}}},
        "hints":{"read":{"tool":"localFetch","query":{"queries":[{"path":outside.clone()}]}},
                 "refs":{"tool":"lspSearch","query":{"queries":[{"path":format!("{root}/src/a.rs"),"workspaceRoot":root.clone()}]}}}
    }});
    let value = envelope_in(
        vec![row],
        ToolId::LocalSearch,
        &[Some(&json!({"path": format!("{root}/src")}))],
        &ws.paths,
    );
    let data = &value["results"][0]["data"];
    assert_eq!(
        data["next"]["nextPage"]["query"]["queries"][0]["path"], "src",
        "{value}"
    );
    assert_eq!(
        data["hints"]["read"]["query"]["queries"][0]["path"],
        outside.as_str(),
        "{value}"
    );
    let refs = &data["hints"]["refs"]["query"]["queries"][0];
    assert_eq!(refs["path"], "src/a.rs", "{value}");
    assert_eq!(refs["workspaceRoot"], root.as_str(), "{value}");
    assert_resolvable(&ws, &value, "src");
}

/// With `extras: ["receipts"]`, local reads report the checked-out
/// commit once per response: shared when every row reads the same
/// repository, per row otherwise, and not at all outside a repository,
/// for other tools, or for rows that did not ask.
#[test]
fn local_rows_report_the_checked_out_head_once() {
    let ws = workspace();
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let git = ws.root.join(".git");
    std::fs::create_dir_all(git.join("refs/heads")).expect("git");
    std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").expect("head");
    std::fs::write(git.join("refs/heads/main"), format!("{sha}\n")).expect("ref");
    let root = ws.root.to_string_lossy().into_owned();
    let row =
        |index: u64| json!({"index":index,"data":{"files":[{"path":format!("{root}/src/a.rs")}]}});
    let src = json!({"path": format!("{root}/src"), "extras": ["receipts"]});
    let tests = json!({"path": format!("{root}/tests"), "extras": ["receipts"]});
    let unasked = json!({"path": format!("{root}/src")});
    let default = envelope_in(
        vec![row(0)],
        ToolId::LocalSearch,
        &[Some(&unasked)],
        &ws.paths,
    );
    assert!(default.get("shared").is_none(), "{default}");
    assert!(
        default["results"][0]["data"].get("commitSha").is_none(),
        "{default}"
    );
    for tool in [
        ToolId::LocalSearch,
        ToolId::LocalFetch,
        ToolId::StructureSearch,
    ] {
        let value = envelope_in(
            vec![row(0), row(1)],
            tool,
            &[Some(&src), Some(&tests)],
            &ws.paths,
        );
        assert_eq!(value["shared"]["commitSha"], sha, "{tool:?} {value}");
        assert!(
            value["results"][0]["data"].get("commitSha").is_none(),
            "{value}"
        );
    }
    let outside = json!({"path": ws.outside.to_string_lossy(), "extras": ["receipts"]});
    let mixed = envelope_in(
        vec![row(0), row(1)],
        ToolId::LocalSearch,
        &[Some(&src), Some(&outside)],
        &ws.paths,
    );
    assert!(mixed["shared"].get("commitSha").is_none(), "{mixed}");
    assert_eq!(mixed["results"][0]["data"]["commitSha"], sha, "{mixed}");
    assert!(
        mixed["results"][1]["data"].get("commitSha").is_none(),
        "{mixed}"
    );
    let other = envelope_in(vec![row(0)], ToolId::AstSearch, &[Some(&src)], &ws.paths);
    assert!(other.get("shared").is_none(), "{other}");
    let plain = envelope_in(
        vec![row(0)],
        ToolId::LocalSearch,
        &[Some(&outside)],
        &ws.paths,
    );
    assert!(plain.get("shared").is_none(), "{plain}");
}

#[test]
fn structure_and_ast_rows_are_workspace_relative_for_any_query_root() {
    let ws = workspace();
    let root = ws.root.to_string_lossy().into_owned();
    // `.` is the workspace root itself: rows lead with its own name.
    let name = ws
        .root
        .file_name()
        .expect("name")
        .to_string_lossy()
        .into_owned();
    for (tool, query, row, expected) in [
        (
            "structureSearch",
            json!({"path":"."}),
            json!({"path": name, "files":[{"path": format!("{name}/tests/x.ts")}]}),
            (".", "tests/x.ts"),
        ),
        (
            "structureSearch",
            json!({"path": format!("{root}/src")}),
            json!({"path":"src","files":[{"path":"src/runtime/b.rs"}]}),
            ("src", "src/runtime/b.rs"),
        ),
        (
            "astSearch",
            json!({"path":"src/runtime"}),
            json!({"path":"runtime","files":[{"path":"runtime/b.rs"}]}),
            ("src/runtime", "src/runtime/b.rs"),
        ),
        // A file root names the file relative to its parent.
        (
            "astSearch",
            json!({"path":"src/a.rs"}),
            json!({"path":"a.rs","files":[{"path":"a.rs"}]}),
            ("src/a.rs", "src/a.rs"),
        ),
    ] {
        let value = envelope_in(
            vec![json!({"index":0,"data":row})],
            ToolId::from_name(tool).expect("known tool"),
            &[Some(&query)],
            &ws.paths,
        );
        let data = &value["results"][0]["data"];
        assert_eq!(
            (
                data["path"].as_str().unwrap(),
                data["files"][0]["path"].as_str().unwrap()
            ),
            expected,
            "{tool} {query}"
        );
        assert_resolvable(&ws, &value, expected.1);
    }
}

/// structureSearch `files` groups name `dir` relative to the row's
/// `path` (like `tree`), so the envelope leaves them as they are: only
/// `path` resolves against `base`, and a batch never hoists a `dir`.
#[test]
fn structure_listing_dirs_stay_relative_to_the_row_path() {
    let ws = workspace();
    let src = json!({"path":"src"});
    let rows = vec![
        json!({"index":0,"data":{"path":"src","files":["a.rs (1)",
            {"dir":"runtime","files":["b.rs (1)"]}]}}),
        json!({"index":1,"data":{"path":"src","files":[{"dir":"runtime","files":["b.rs (1)"]}]}}),
    ];
    let value = envelope_in(
        rows,
        ToolId::StructureSearch,
        &[Some(&src), Some(&src)],
        &ws.paths,
    );
    assert!(value.get("shared").is_none(), "{value}");
    for row in 0..2 {
        let data = &value["results"][row]["data"];
        let files = data["files"].as_array().expect("files");
        let group = files.iter().find(|item| item.is_object()).expect("group");
        assert_eq!(group["dir"], "runtime", "{value}");
        assert_eq!(data["path"], "src", "{value}");
    }
    for path in ["src/a.rs", "src/runtime/b.rs"] {
        assert_resolvable(&ws, &value, path);
    }
}

#[test]
fn mixed_root_batches_resolve_every_row_even_after_a_rejected_row() {
    let ws = workspace();
    let outside = ws.outside.to_string_lossy().into_owned();
    let src = json!({"path":"src"});
    let nested = json!({"path": ws.root.join("src/runtime").to_string_lossy()});
    let far = json!({"path": outside});
    let rows = vec![
        json!({"index":0,"data":{"files":[{"path":"src/a.rs"}]}}),
        json!({"index":1,"status":"error","data":{"error":"x","errorCode":"invalidInput"}}),
        json!({"index":2,"data":{"symbols":[{"path":"runtime/b.rs"}]}}),
        json!({"index":3,"data":{"files":[{"path": format!("{}/o.rs", ws.outside.file_name().unwrap().to_string_lossy())}]}}),
    ];
    let value = envelope_in(
        rows,
        ToolId::AstSearch,
        &[Some(&src), None, Some(&nested), Some(&far)],
        &ws.paths,
    );
    assert_resolvable(&ws, &value, "src/a.rs");
    assert_eq!(value["results"][0]["data"]["files"][0]["path"], "src/a.rs");
    assert_eq!(
        value["results"][2]["data"]["symbols"][0]["path"],
        "src/runtime/b.rs"
    );
    // Outside the workspace a path stays absolute.
    let far_path = value["results"][3]["data"]["files"][0]["path"]
        .as_str()
        .unwrap();
    assert_eq!(far_path, format!("{outside}/o.rs"));
    assert!(ws.paths.validate(far_path).is_ok());
}

#[test]
fn local_search_and_lsp_rows_are_workspace_relative_and_outside_rows_absolute() {
    let ws = workspace();
    let root = ws.root.to_string_lossy().into_owned();
    let row = json!({"index":0,"data":{"files":[{"path": format!("{root}/src/runtime/b.rs")}],
        "next":{"nextPage":{"tool":"localSearch","query":{"queries":[{"path":"src/runtime"}]}}}}});
    let value = envelope_in(
        vec![row],
        ToolId::LocalSearch,
        &[Some(&json!({"path":"src/runtime"}))],
        &ws.paths,
    );
    assert_eq!(
        value["results"][0]["data"]["files"][0]["path"],
        "src/runtime/b.rs"
    );
    assert_eq!(
        value["results"][0]["data"]["next"]["nextPage"]["query"]["queries"][0]["path"],
        "src/runtime"
    );
    assert_resolvable(&ws, &value, "src/runtime/b.rs");

    // An empty row carries no path, but its lead's relative `path`
    // resolves against the workspace: the response names it as `root`.
    let empty = json!({"index":0,"status":"empty","data":{"files":[],
        "next":{"includeSkipped":{"tool":"localSearch","query":{"queries":[{"path": format!("{root}/src/runtime"),"includeIgnored":true}]}}}}});
    let value = envelope_in(
        vec![empty],
        ToolId::LocalSearch,
        &[Some(&json!({"path":"src/runtime"}))],
        &ws.paths,
    );
    assert_eq!(
        value["results"][0]["data"]["next"]["includeSkipped"]["query"]["queries"][0]["path"],
        "src/runtime"
    );
    assert_eq!(value["root"], root.as_str(), "{value}");

    let lsp = json!({"index":0,"data":{"type":"documentSymbols","uri":format!("file://{root}/src/a.rs")}});
    let value = envelope_in(vec![lsp], ToolId::LspSearch, &[Some(&json!({}))], &ws.paths);
    assert_eq!(value["results"][0]["data"]["path"], "src/a.rs");
    assert_resolvable(&ws, &value, "src/a.rs");

    let outside = format!("{}/o.rs", ws.outside.to_string_lossy());
    let row = json!({"index":0,"data":{"files":[{"path": outside}]}});
    let value = envelope_in(
        vec![row],
        ToolId::LocalSearch,
        &[Some(&json!({"path": ws.outside.to_string_lossy()}))],
        &ws.paths,
    );
    assert!(value.get("root").is_none(), "{value}");
    assert_eq!(
        value["results"][0]["data"]["files"][0]["path"],
        outside.as_str()
    );

    // LSP locations outside the workspace name the same `path` field,
    // absolute, beside workspace-relative ones.
    let lsp = json!({"index":0,"data":{"type":"callers","items":[
        {"name":"inside","uri":format!("file://{root}/src/a.rs")},
        {"name":"outside","uri":format!("file://{outside}")}
    ]}});
    let value = envelope_in(vec![lsp], ToolId::LspSearch, &[Some(&json!({}))], &ws.paths);
    let items = &value["results"][0]["data"]["items"];
    assert_eq!(items[0]["path"], "src/a.rs", "{value}");
    assert_eq!(items[1]["path"], outside.as_str(), "{value}");
    assert!(items[1].get("uri").is_none(), "{value}");
}

#[test]
fn uri_path_compaction_preserves_field_order_and_appends_path() {
    let tool = ToolId::from_name("lspSearch").expect("known tool");
    let output = envelope(
        tool,
        vec![result_row(
            tool,
            0,
            &json!({}),
            json!({"type":"documentSymbols","uri":"file:///repo/src/a.ts","lsp":{},"pagination":{"hasMore":false}}),
            None,
        )],
    );
    let keys = output["results"][0]["data"]
        .as_object()
        .expect("data object")
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    assert_eq!(keys, ["type", "lsp", "pagination", "path"]);
}

/// localSearch per-file entries must always carry their own
/// totalMatchRows/returnedMatchRows: hoisting them into `shared` when the
/// values coincide (typical on page 1) made the row shape data-dependent,
/// so CLI page-1 output drifted from MCP and from page-2 output.
#[test]
fn match_row_accounting_is_never_hoisted_into_shared() {
    let tool = ToolId::from_name("localSearch").expect("known tool");
    let output = envelope(
        tool,
        vec![result_row(
            tool,
            0,
            &json!({}),
            json!({"files":[
                {"path":"/repo/a.txt","matches":[{"line":1,"column":0,"value":"x"}],"totalMatchRows":1,"returnedMatchRows":1},
                {"path":"/repo/b.txt","matches":[{"line":1,"column":0,"value":"x"}],"totalMatchRows":1,"returnedMatchRows":1}
            ]}),
            None,
        )],
    );
    for file in output["results"][0]["data"]["files"]
        .as_array()
        .expect("files")
    {
        assert_eq!(file["totalMatchRows"], 1, "{output}");
        assert_eq!(file["returnedMatchRows"], 1, "{output}");
    }
    assert!(
        output
            .get("shared")
            .and_then(|shared| shared.get("totalMatchRows"))
            .is_none(),
        "{output}"
    );
}

#[test]
fn path_compaction_never_rewrites_evidence_or_executable_queries() {
    let data = json!({"path":"/repo/src/a.ts","content":"/repo/src/a.ts","pagination":{"hasMore":true},"next":{"continueWindow":{"tool":"localFetch","query":{"queries":[{"path":"/repo/src/a.ts","offset":2}]}}}});
    let tool = ToolId::from_name("localFetch").expect("known tool");
    let output = envelope(
        tool,
        vec![result_row(
            tool,
            0,
            &json!({"extras":["diagnostics"]}),
            data,
            None,
        )],
    );
    assert_eq!(output["root"], "/repo/src");
    assert_eq!(output["results"][0]["data"]["path"], "a.ts");
    assert_eq!(output["results"][0]["data"]["content"], "/repo/src/a.ts");
    assert_eq!(
        output.pointer("/results/0/data/next/continueWindow/query/queries/0/path"),
        Some(&json!("/repo/src/a.ts"))
    );
    assert_eq!(
        output.pointer("/results/0/meta/diagnostics"),
        Some(&json!({"partial":true}))
    );
}

#[test]
fn shared_compaction_preserves_required_pull_request_row_fields() {
    let tool = ToolId::from_name("ghSearchHistory").expect("known tool");
    let output = envelope(
        tool,
        vec![result_row(
            tool,
            0,
            &json!({"operation":"pullRequest"}),
            json!({
                "type":"pullRequests",
                "pullRequests":[
                    {"number":1,"title":"One","state":"merged","author":"octocode","labels":[],"createdAt":"2026-01-01","mergedAt":"2026-01-02","commentsCount":0},
                    {"number":2,"title":"Two","state":"merged","author":"octocode","labels":[],"createdAt":"2026-01-03","mergedAt":"2026-01-04","commentsCount":0}
                ]
            }),
            None,
        )],
    );
    for row in output["results"][0]["data"]["pullRequests"]
        .as_array()
        .expect("pull requests")
    {
        for field in [
            "number",
            "title",
            "state",
            "author",
            "labels",
            "createdAt",
            "mergedAt",
            "commentsCount",
        ] {
            assert!(row.get(field).is_some(), "missing {field}: {output}");
        }
    }
}
/// Never-trim invariant: a clipped value must carry a continuation that
/// reaches it. A read of another file, or a page of more rows, does not.
#[test]
fn a_clipped_value_needs_a_continuation_that_reaches_it() {
    let read = |path: &str| json!({"tool":"localFetch","query":{"queries":[{"path":path,"ranges":["1-2"]}]}});
    let page = |next: Value| {
        json!({"files":[
            {"path":"a.rs","matches":[{"line":1,"value":"short"}]},
            {"path":"b.rs","matches":[{"line":4,"value":"long…","clipped":true,"originalChars":900}]}
        ],"next":next})
    };
    let codes = |data: Value| {
        result_row(
            ToolId::from_name("localSearch").expect("known tool"),
            0,
            &json!({"extras":["diagnostics"]}),
            data,
            None,
        )
        .pointer("/meta/diagnostics/codes")
        .cloned()
    };
    let missing = Some(json!(["continuationMissing"]));
    assert_eq!(codes(page(json!({"read":read("a.rs")}))), missing);
    assert_eq!(
        codes(page(
            json!({"read":read("a.rs"),"nextPage":{"tool":"localSearch","query":{"queries":[{"path":".","matchString":"x","page":2}]}}})
        )),
        missing
    );
    assert_eq!(codes(page(json!({"read":read("b.rs")}))), None);
    // An absolute row path and a relative continuation name one file.
    let absolute = |next: Value| {
        let mut data = page(next);
        data["files"][1]["path"] = json!("/repo/src/b.rs");
        data
    };
    assert_eq!(codes(absolute(json!({"read":read("src/b.rs")}))), None);
    assert_eq!(codes(absolute(json!({"read":read("c/b.rs")}))), missing);
    assert_eq!(codes(absolute(json!({"read":read("rc/b.rs")}))), missing);
    assert_eq!(
        codes(page(
            json!({"expandValues":{"tool":"localSearch","query":{"queries":[{"path":".","matchString":"x","matchContentLength":900}]}}})
        )),
        None
    );
}

/// Every tool's stale snapshot is the same failure: one text and a
/// `next.restart` that reads page 1 of the same query on the current
/// source. A restart the tool built is kept.
#[test]
fn a_stale_snapshot_always_restarts_with_one_text() {
    let query =
        json!({"path":"src","operation":"match","pattern":"f($A)","page":3,"snapshot":"s1"});
    let row = result_row(
        ToolId::AstSearch,
        0,
        &query,
        json!({"errorCode":"staleSnapshot","error":"tool wording","snapshot":"s2","isPartial":true}),
        Some("error"),
    );
    let data = &row["data"];
    assert_eq!(
        data["error"],
        super::super::pages::STALE_SNAPSHOT_ERROR,
        "{data}"
    );
    assert!(
        data.get("isPartial").is_none(),
        "a restart is not a remaining page: {data}"
    );
    let restart = &data["next"]["restart"];
    assert_eq!(restart["tool"], "astSearch", "{data}");
    assert_eq!(
        restart["query"]["queries"][0],
        json!({"path":"src","operation":"match","pattern":"f($A)"}),
        "page 1 of the same query: {data}"
    );
    let own = json!({"tool":"localFetch","query":{"queries":[{"path":"a","offset":0}]}});
    let row = result_row(
        ToolId::LocalFetch,
        0,
        &json!({"path":"a","offset":90,"snapshot":"s"}),
        json!({"errorCode":"staleSnapshot","error":"x","next":{"restart":own.clone()}}),
        Some("error"),
    );
    assert_eq!(row["data"]["next"]["restart"], own);
    assert_eq!(
        row["data"]["error"],
        super::super::pages::STALE_SNAPSHOT_ERROR
    );
}

/// A stale row ends the page: its pagination says `hasMore:false` and no
/// page continuation survives beside the restart.
#[test]
fn a_stale_snapshot_row_ends_the_page() {
    let query = json!({"path":"src","matchString":"f","page":2,"snapshot":"s1"});
    let row = result_row(
        ToolId::LocalSearch,
        0,
        &query,
        json!({
            "errorCode":"staleSnapshot","error":"x",
            "pagination":{"currentPage":2,"totalPages":5,"hasMore":true},
            "next":{"nextPage":{"tool":"localSearch","query":{"queries":[{"page":3}]}}}
        }),
        Some("error"),
    );
    let data = &row["data"];
    assert_eq!(data["pagination"]["hasMore"], false, "{data}");
    assert!(data["next"].get("nextPage").is_none(), "{data}");
    assert_eq!(data["next"]["restart"]["tool"], "localSearch", "{data}");
}

#[test]
fn incomplete_evidence_requires_executable_continuation_or_terminal_diagnostic() {
    let missing = result_row(
        ToolId::from_name("astSearch").expect("known tool"),
        0,
        &json!({"operation":"symbols","extras":["diagnostics"]}),
        json!({"hasMore":true,"nextPage":2}),
        None,
    );
    assert_eq!(
        missing.pointer("/meta/diagnostics/codes"),
        Some(&json!(["continuationMissing"]))
    );
    let terminal = result_row(
        ToolId::from_name("localFetch").expect("known tool"),
        0,
        &json!({"extras":["diagnostics"]}),
        json!({"isPartial":true,"terminalLimit":true}),
        None,
    );
    assert_eq!(
        terminal.pointer("/meta/diagnostics/codes"),
        Some(&json!(["terminalLimitReached"]))
    );
}

/// A row that offers more of its own result (`next.next*`, `continue*`,
/// `expand*`, `retry*`) is not complete, whatever the tool reported.
#[test]
fn a_row_with_a_next_page_is_never_complete() {
    let page = json!({"tool":"astSearch","query":{"queries":[{"path":".","pattern":"f($A)","scanOffset":100}]}});
    let row = result_row(
        ToolId::AstSearch,
        0,
        &json!({"extras":["diagnostics"]}),
        json!({"files":[],"next":{"expandScan":page}}),
        None,
    );
    assert_eq!(row.pointer("/data/isPartial"), Some(&json!(true)), "{row}");
    assert_eq!(
        row.pointer("/meta/diagnostics/partial"),
        Some(&json!(true)),
        "{row}"
    );
    let flagless = result_row(
        ToolId::LocalFetch,
        0,
        &json!({}),
        json!({"path":"a","isPartial":false,"next":{"continueWindow":page}}),
        None,
    );
    assert_eq!(
        flagless.pointer("/data/isPartial"),
        Some(&json!(true)),
        "{flagless}"
    );
    // A file's own match pagination says more beside the `next` page that
    // answers it (SH-4: nested blocks count): the row need not restate it.
    let nested = result_row(
        ToolId::AstSearch,
        0,
        &json!({}),
        json!({"files":[{"path":"a","matches":[],"pagination":{"hasMore":true}}],
            "next":{"nextMatchPage":page}}),
        None,
    );
    assert!(nested.pointer("/data/isPartial").is_none(), "{nested}");
    assert_eq!(
        nested.pointer("/data/files/0/pagination/hasMore"),
        Some(&json!(true)),
        "{nested}"
    );
    // A restart is a follow-up, not unread rest.
    let preview = result_row(
        ToolId::AstSearch,
        0,
        &json!({}),
        json!({"coverage":{"state":"complete"},"next":{"restart":page}}),
        None,
    );
    assert_eq!(
        preview.pointer("/data/coverage/state"),
        Some(&json!("complete")),
        "{preview}"
    );
    assert!(preview.pointer("/data/isPartial").is_none(), "{preview}");
}

/// `isPartial` is the umbrella flag for a row that does not cover its
/// scope. A row whose own `pagination.hasMore` and `next` page already say
/// so, with no other reason, states it once; any other cause keeps it.
#[test]
fn is_partial_is_dropped_only_where_pagination_and_next_already_say_it() {
    let page =
        json!({"tool":"localSearch","query":{"queries":[{"path":".","matchString":"x","page":2}]}});
    let paged = |extra: Value| {
        let mut data = json!({"files":[{"path":"a"}],"isPartial":true,
            "pagination":{"currentPage":1,"totalPages":2,"hasMore":true},
            "next":{"nextPage":page}});
        for (key, value) in extra.as_object().expect("object") {
            data[key] = value.clone();
        }
        result_row(ToolId::LocalSearch, 0, &json!({}), data, None)
    };
    let plain = paged(json!({}));
    assert!(plain["data"].get("isPartial").is_none(), "{plain}");
    assert_eq!(plain["data"]["pagination"]["hasMore"], true, "{plain}");
    assert!(plain["data"]["next"]["nextPage"].is_object(), "{plain}");
    let injected = result_row(
        ToolId::LocalSearch,
        0,
        &json!({}),
        json!({"files":[{"path":"a"}],"pagination":{"currentPage":1,"hasMore":true},
            "next":{"nextPage":page}}),
        None,
    );
    assert!(injected["data"].get("isPartial").is_none(), "{injected}");
    for (name, extra) in [
        (
            "partialReasons",
            json!({"partialReasons":["providerResultCap"]}),
        ),
        ("terminalLimit", json!({"terminalLimit":true})),
    ] {
        let row = paged(extra);
        assert_eq!(row["data"]["isPartial"], true, "{name}: {row}");
    }
    // A file's own match pagination with its `next` page says more too
    // (SH-4: nested blocks count as the row's own).
    let nested = result_row(
        ToolId::LocalSearch,
        0,
        &json!({}),
        json!({"files":[{"path":"a","pagination":{"hasMore":true}}],
            "next":{"nextMatchPage":page}}),
        None,
    );
    assert!(nested["data"].get("isPartial").is_none(), "{nested}");
    // A partial row with no page to read keeps the flag.
    let stopped = result_row(
        ToolId::LocalSearch,
        0,
        &json!({}),
        json!({"files":[{"path":"a"}],"isPartial":true}),
        None,
    );
    assert_eq!(stopped["data"]["isPartial"], true, "{stopped}");
}

#[test]
fn result_rows_normalize_null_is_partial_to_false() {
    let row = result_row(
        ToolId::from_name("ghGetFileContent").expect("known tool"),
        0,
        &json!({}),
        json!({"path":"a","content":"x","isPartial":null}),
        None,
    );
    assert_eq!(row.pointer("/data/isPartial"), Some(&json!(false)));
}

/// `empty` is decided per tool from the rows a page holds; a row that
/// can still be read on (an executable page, or a pagination block with
/// `hasMore`) is never `empty`, for every tool. A row with nothing more
/// keeps the tool's `empty`.
#[test]
fn a_row_with_more_to_read_is_never_empty() {
    let page = |tool: ToolId| json!({"tool": tool.as_str(), "query": {"queries": [{"page": 2}]}});
    for tool in ToolId::ALL
        .into_iter()
        .filter(|tool| !tool.output().resource_major())
    {
        for data in [
            json!({"files":[],"next":{"nextPage":page(tool)}}),
            json!({"files":[],"pagination":{"currentPage":1,"hasMore":true}}),
        ] {
            let row = result_row(tool, 0, &json!({}), data.clone(), Some("empty"));
            assert!(row.get("status").is_none(), "{tool:?} {data}: {row}");
        }
        let finished = result_row(
            tool,
            0,
            &json!({}),
            json!({"files":[],"pagination":{"currentPage":1,"hasMore":false}}),
            Some("empty"),
        );
        assert_eq!(finished["status"], "empty", "{tool:?}: {finished}");
    }
}

#[test]
fn result_rows_preserve_compare_status_as_contract_data() {
    let row = result_row(
        ToolId::from_name("ghGetHistoryItem").expect("known tool"),
        0,
        &json!({"operation":"compare"}),
        json!({"aheadBy":2,"commits":[]}),
        None,
    );
    assert_eq!(row.pointer("/data/aheadBy"), Some(&json!(2)));
}

#[test]
fn result_metadata_requires_diagnostics_extras() {
    let mut normal = result_row(
        ToolId::from_name("localSearch").expect("known tool"),
        0,
        &json!({}),
        json!({"files":[]}),
        None,
    );
    super::super::extras::prune_row(&mut normal, ToolId::LocalSearch, &json!({}));
    assert!(normal.get("meta").is_none());

    let query = json!({"extras":["diagnostics"]});
    let mut diagnostics = result_row(
        ToolId::from_name("localSearch").expect("known tool"),
        0,
        &query,
        json!({"files":[]}),
        None,
    );
    super::super::extras::prune_row(&mut diagnostics, ToolId::LocalSearch, &query);
    assert_eq!(diagnostics["meta"]["evidence"]["kind"], "lexical");
}

/// Debug diagnostics (`meta`) never take the row's one recovery hint: an
/// empty code search keeps its cause-specific tip.
#[test]
fn diagnostic_hints_never_displace_the_rows_recovery_tip() {
    let mut row = json!({"index":0,"status":"empty",
        "meta":{"diagnostics":{"hints":["GitHub reported an incomplete search index result; retry."]}},
        "data":{"hints":["The repository is missing, private, or hidden from this token; check owner/repo spelling."]}});
    apply_hint_policy(&mut row, ToolId::GhSearchCode, &json!({}));
    assert_eq!(
        row["data"]["hints"],
        json!([
            "The repository is missing, private, or hidden from this token; check owner/repo spelling."
        ])
    );
    assert_eq!(
        row["meta"]["diagnostics"]["hints"].as_array().map(Vec::len),
        Some(1)
    );
}

/// `hints.text` is an ordered list: the recovery first, then each further
/// finding in the order the tool wrote it, a repeat once, none lost.
#[test]
fn hints_are_an_ordered_list_that_keeps_every_finding() {
    let mut row = json!({"index":0,"status":"empty",
    "data":{"hints":[
        "No matches under src.",
        "2 entries sit behind skipped paths (build/); the lead lists them.",
        "No matches under src.",
        "Another finding."
    ]}});
    apply_hint_policy(&mut row, ToolId::LocalSearch, &json!({}));
    assert_eq!(
        row["data"]["hints"],
        json!([
            "No matches under src.",
            "2 entries sit behind skipped paths (build/); the lead lists them.",
            "Another finding."
        ])
    );
}

#[test]
fn error_fallbacks_are_failure_class_aware_and_preserve_domain_recovery() {
    let query = json!({"owner":"o","keywords":["octocode"]});
    let mut timeout = json!({
        "index": 0,
        "status": "error",
        "data": {"error":"timed out","errorCode":"timeout"}
    });
    apply_hint_policy(
        &mut timeout,
        ToolId::from_name("ghSearchCode").expect("known tool"),
        &query,
    );
    let hint = timeout["data"]["hints"][0].as_str().expect("hint");
    assert!(hint.contains("Retry once"), "{timeout}");
    assert!(!hint.contains("Broaden keywords"), "{timeout}");

    let mut permission = json!({
        "index": 0,
        "status": "error",
        "data": {"error":"forbidden","errorCode":"permission"}
    });
    apply_hint_policy(
        &mut permission,
        ToolId::from_name("ghSearchCode").expect("known tool"),
        &query,
    );
    assert!(
        permission["data"]["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("token scopes")),
        "{permission}"
    );

    let mut owned = json!({
        "index": 0,
        "status": "error",
        "data": {
            "error":"missing",
            "errorCode":"notFound",
            "hints":["Inspect the repository tree."]
        }
    });
    apply_hint_policy(
        &mut owned,
        ToolId::from_name("ghSearchCode").expect("known tool"),
        &query,
    );
    assert_eq!(
        owned["data"]["hints"],
        json!(["Inspect the repository tree."])
    );

    let mut missing_path = json!({
        "index": 0,
        "status": "error",
        "data": {"error":"missing","errorCode":"fileAccessFailed"}
    });
    apply_hint_policy(
        &mut missing_path,
        ToolId::from_name("localFetch").expect("known tool"),
        &json!({"path":"/repo/missing.rs"}),
    );
    assert!(
        missing_path["data"]["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("structureSearch operation:\"files\"")),
        "{missing_path}"
    );
}

/// One code, one recovery hint. A missing path gets the same hint
/// on every tool that reports `pathNotFound`.
#[test]
fn path_errors_have_one_hint_on_every_tool() {
    for (code, expected) in [
        (
            crate::policy::PATH_NOT_FOUND,
            crate::response::output::VERIFY_PATH_HINT,
        ),
        (
            crate::policy::PATH_POLICY_DENIED,
            crate::policy::discovery::WITHHELD_HINT,
        ),
    ] {
        let hints: std::collections::BTreeSet<String> = [
            "localFetch",
            "localSearch",
            "structureSearch",
            "astSearch",
            "lspSearch",
            "ghCloneRepo",
        ]
        .into_iter()
        .map(|tool| {
            let mut row = json!({
                "index": 0,
                "status": "error",
                "data": {"error":"withheld","errorCode":code}
            });
            apply_hint_policy(
                &mut row,
                ToolId::from_name(tool).expect("known tool"),
                &json!({"path":"/repo/missing.rs"}),
            );
            row["data"]["hints"][0].as_str().expect("hint").to_owned()
        })
        .collect();
        assert_eq!(hints.into_iter().collect::<Vec<_>>(), [expected], "{code}");
    }
}

#[test]
fn invalid_input_and_missing_path_errors_do_not_suggest_broadening() {
    for (tool, code, expected) in [
        (
            "structureSearch",
            "invalidInput",
            "Correct the rejected field",
        ),
        ("ghSearchRepo", "invalidInput", "Correct the rejected field"),
        ("structureSearch", "pathNotFound", "Verify the path exists"),
        ("localSearch", "pathNotFound", "Verify the path exists"),
        ("structureSearch", "outsideAllowedRoots", "allowed roots"),
    ] {
        let mut row = json!({
            "index": 0,
            "status": "error",
            "data": {"error":"failed","errorCode":code}
        });
        apply_hint_policy(
            &mut row,
            ToolId::from_name(tool).expect("known tool"),
            &json!({"path":"/repo/nope"}),
        );
        let hint = row["data"]["hints"][0].as_str().expect("hint");
        assert!(hint.contains(expected), "{code}: {hint}");
        assert!(!hint.contains("Broaden"), "{code}: {hint}");
    }
    assert!(is_invalid_input_code("invalidPattern"));
    assert!(!is_invalid_input_code("fileAccessFailed"));
    // astSearch rejects a missing, unknown, or mismatched grammar and an
    // unparseable pattern: the caller must fix the request (exit 2).
    for code in [
        "languageRequired",
        "languageUnsupported",
        "languageMismatch",
        "languageFileRequired",
        "languageDirectoryRequired",
        "invalidPattern",
        "invalidGlob",
        "invalidInput",
    ] {
        assert!(is_invalid_input_code(code), "{code}");
    }
    for code in [
        "timeout",
        "executionFailed",
        "fileTooLarge",
        "pathNotFound",
        "fileAccessFailed",
        // Undeclared spellings are no class at all, never invalid input.
        "structural.language.unsupported",
    ] {
        assert!(!is_invalid_input_code(code), "{code}");
    }
}

#[test]
fn ast_search_error_hints_are_code_specific() {
    let query = json!({"operation":"match","path":"/repo","pattern":"foo($A)"});
    for (code, expected) in [
        ("outsideAllowedRoots", "allowed root"),
        ("structural.query.compileFailed", "syntaxTree"),
        ("languageRequired", "language"),
        ("languageUnsupported", "language"),
        ("languageFileRequired", "languageGlobs"),
        ("languageDirectoryRequired", "language"),
    ] {
        let mut row = json!({
            "index": 0,
            "status": "error",
            "data": {"error":"failed","errorCode":code}
        });
        apply_hint_policy(
            &mut row,
            ToolId::from_name("astSearch").expect("known tool"),
            &query,
        );
        let hint = row["data"]["hints"][0].as_str().expect("hint");
        assert!(hint.contains(expected), "{code}: {hint}");
        assert!(!hint.contains("Broaden the syntax"), "{code}: {hint}");
    }
    // A parse limit is its typed error alone: no fallback text.
    let mut row = json!({"index":0,"status":"error",
        "data":{"error":"failed","errorCode":"fileTooLarge","terminalLimit":true}});
    apply_hint_policy(&mut row, ToolId::AstSearch, &query);
    assert!(row["data"].get("hints").is_none(), "{row}");
}

/// A page past the end is one shared outcome on every tool: status
/// `empty`, a `reason` naming the page and the page count, and a
/// `next.restart` at page 1 without the page cursors.
/// The empty fallback names the filter that most likely hid the
/// answer, in each tool's own vocabulary.
#[test]
fn empty_fallbacks_name_the_tools_own_filters() {
    let hint = |tool: ToolId, query: Value| tool.output().fallback_hint(&query);
    for (tool, query, needle) in [
        (
            ToolId::GhSearchCode,
            json!({"extensions":["rs"]}),
            "extensions",
        ),
        (ToolId::GhSearchCode, json!({"matchIn":"path"}), "path"),
        (
            ToolId::GhSearchCode,
            json!({"keywords":["x"]}),
            "unproven absence",
        ),
        (
            ToolId::GhSearchCode,
            json!({"keywords":["x","y"]}),
            "keywords",
        ),
        // A repository search's filters drop through its `broadenSearch`
        // lead, so its fallback text only names keywords.
        (ToolId::GhSearchRepo, json!({"keywords":["x"]}), "keywords"),
        (ToolId::GhStructure, json!({"include":["*.rs"]}), "include"),
        (ToolId::GhStructure, json!({"path":"src"}), "path"),
        (
            ToolId::ArtifactSearch,
            json!({"packageName":"x"}),
            "packageName",
        ),
        (
            ToolId::ArtifactSearch,
            json!({"keywords":["x"]}),
            "keywords",
        ),
        (
            ToolId::StructureSearch,
            json!({"operation":"files","extensions":["rs"]}),
            "extensions",
        ),
        (
            ToolId::StructureSearch,
            json!({"operation":"tree","path":"."}),
            "path",
        ),
    ] {
        let text = hint(tool, query.clone());
        assert!(text.contains(needle), "{tool:?} {query}: {text}");
        assert!(
            text.chars().count() <= MAX_GUIDANCE_CHARS,
            "{tool:?}: {text}"
        );
    }
    // Filters differ, so the advice differs.
    assert_ne!(
        hint(ToolId::GhSearchCode, json!({"extensions":["rs"]})),
        hint(ToolId::GhSearchCode, json!({"keywords":["x"]}))
    );
}

#[test]
fn an_out_of_range_page_is_one_shared_outcome() {
    let lsp = json!({"operation":"references","path":"a.rs","symbolName":"x","line":1});
    for (tool, base) in [
        (ToolId::LocalSearch, json!({"path":".","matchString":"x"})),
        (ToolId::LspSearch, lsp),
        (ToolId::StructureSearch, json!({"path":"."})),
        (
            ToolId::AstSearch,
            json!({"path":".","operation":"match","pattern":"x"}),
        ),
        (ToolId::LocalFetch, json!({"path":"a.rs"})),
    ] {
        let mut query = base.clone();
        query["page"] = json!(99);
        query["snapshot"] = json!("s");
        // A tool's own partial signals on a page past the end restate
        // nothing: no page is left to read, `next.restart` is the lead.
        let data = json!({"files":[],"isPartial":true,"terminalLimit":true,
            "partialReasons":["pageOutOfRange"],"pagination":{"currentPage":99,
            "totalPages":4,"totalItems":10,"hasMore":false,"outOfRange":true}});
        let row = result_row(tool, 0, &query, data, None);
        assert_eq!(row["status"], "outOfRange", "{tool}: {row}");
        for signal in ["isPartial", "terminalLimit", "partialReasons"] {
            assert!(row["data"].get(signal).is_none(), "{tool} {signal}: {row}");
        }
        assert_eq!(row["data"]["reason"], "page 99 of 4", "{tool}: {row}");
        assert_eq!(
            row["data"]["next"]["restart"]["query"]["queries"][0], base,
            "{tool}: {row}"
        );
        assert!(row["data"].get("error").is_none(), "{tool}: {row}");
    }
    // A tool that already led with its own restart keeps it.
    let own = json!({"files":[],"next":{"restart":{"tool":"localFetch","query":{"queries":[{"path":"x"}]}}},
        "pagination":{"currentPage":3,"totalPages":2,"hasMore":false,"outOfRange":true}});
    let row = result_row(ToolId::LocalFetch, 0, &json!({"path":"a.rs"}), own, None);
    assert_eq!(
        row["data"]["next"]["restart"]["query"]["queries"][0]["path"], "x",
        "{row}"
    );
    // An in-range last page is a plain result, never this outcome.
    let last = json!({"files":[{"path":"a"}],"pagination":{"currentPage":4,"totalPages":4,"hasMore":false}});
    let row = result_row(
        ToolId::LocalSearch,
        0,
        &json!({"path":".","matchString":"x"}),
        last,
        None,
    );
    assert!(row.get("status").is_none(), "{row}");
    assert!(row["data"].get("reason").is_none(), "{row}");
}

/// A section page past the end (`contentPagination.files`) settles like the row's
/// own `pagination`: an empty row, the page named, and a restart.
#[test]
fn a_file_page_past_the_end_is_the_same_shared_out_of_range_outcome() {
    let query = json!({"operation":"commit","owner":"o","repo":"r","ref":"abc","page":99});
    let data = json!({"commitSha":"abc","files":[],"isPartial":true,
        "contentPagination":{"files":{"currentPage":99,"totalPages":1,"pageSize":30,"totalItems":2,
            "hasMore":false,"outOfRange":true}}});
    let row = result_row(ToolId::GhGetHistoryItem, 0, &query, data, None);
    assert_eq!(row["status"], "outOfRange", "{row}");
    assert_eq!(row["data"]["reason"], "page 99 of 1", "{row}");
    assert!(row["data"].get("isPartial").is_none(), "{row}");
    let restart = &row["data"]["next"]["restart"]["query"]["queries"][0];
    assert_eq!(restart["ref"], "abc", "{row}");
    assert!(restart.get("page").is_none(), "{row}");
}

/// A bare empty row (no hint, no lead) gets its tool's own fallback
/// tip, shaped by the query; a recovery the tool already stated stays the
/// row's one hint.
#[test]
fn a_bare_empty_row_gets_its_tools_fallback_hint() {
    for tool in ToolId::ALL {
        let query = json!({});
        let mut row = json!({"index":0,"status":"empty","data":{"files":[]}});
        apply_hint_policy(&mut row, tool, &query);
        let hint = row["data"]["hints"][0].as_str().unwrap_or_default();
        assert_eq!(
            hint,
            concise(tool.output().fallback_hint(&query)),
            "{tool}: {row}"
        );
    }
    let mut stated = json!({"index":0,"status":"empty",
        "data":{"hints":["Own recovery."],"files":[]}});
    apply_hint_policy(&mut stated, ToolId::LocalSearch, &json!({}));
    assert_eq!(stated["data"]["hints"], json!(["Own recovery."]));
    let mut led = json!({"index":0,"status":"empty",
        "data":{"next":{"restart":{"tool":"localSearch","query":{"queries":[{"path":"."}]}}}}});
    apply_hint_policy(&mut led, ToolId::LocalSearch, &json!({}));
    assert!(led["data"].get("hints").is_none(), "{led}");
    // An empty row whose category the tool answers for keeps only its facts.
    let mut answered = json!({"index":0,"status":"empty",
        "data":{"category":"noDiagnostics","reason":"No diagnostics."}});
    apply_hint_policy(
        &mut answered,
        ToolId::LspSearch,
        &json!({"operation":"diagnostic"}),
    );
    assert!(answered["data"].get("hints").is_none(), "{answered}");
    // A successful row is never given one.
    let mut found = json!({"index":0,"data":{"files":[{"path":"a"}]}});
    apply_hint_policy(&mut found, ToolId::LocalSearch, &json!({}));
    assert!(found["data"].get("hints").is_none(), "{found}");
}

fn sanitize_context() -> crate::execution::ExecutionContext {
    crate::execution::ExecutionContext {
        cancellation: tokio_util::sync::CancellationToken::new(),
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(1),
        walk_threads: None,
        response_window: None,
        github_credential: None,
    }
}

#[test]
fn sanitizer_preserves_non_secret_next_query_and_location() {
    // A benign, non-secret-shaped string passes through unchanged — the
    // continuation stays executable and the location is untouched.
    let mut value = json!({
        "content": "token ghp_secretvalue12",
        "next": {
            "continueWindow": {
                "tool": "localFetch",
                "query": {"queries":[{"path": "/repo/ghp_secretvalue12.rs", "offset": 2}]},
                "confidence": "exact"
            }
        },
        "location": {"localPath": "/tmp/ghp_secretvalue12/repo"}
    });
    let context = sanitize_context();
    let security = crate::security::ContentSecurity::new();
    sanitize_fields(&mut value, &security, &context).expect("sanitize");
    assert_eq!(
        value.pointer("/next/continueWindow/query/queries/0/path"),
        Some(&json!("/repo/ghp_secretvalue12.rs"))
    );
    assert_eq!(
        value.pointer("/location/localPath"),
        Some(&json!("/tmp/ghp_secretvalue12/repo"))
    );
    // The executable tool identifier survives verbatim.
    assert_eq!(
        value.pointer("/next/continueWindow/query/queries/0/offset"),
        Some(&json!(2))
    );
}

#[test]
fn email_redaction_is_opt_in_and_masks_commit_authors() {
    let commit_row = || {
        json!({
            "commits": [{
                "sha": "abc123",
                "author": {"name": "Dev One", "email": "dev.one+git@example.co.uk"},
                "message": "fix: reported by user@example.com"
            }]
        })
    };
    let context = sanitize_context();
    let security = crate::security::ContentSecurity::new();
    // Default path (no opt-in): emails pass through unchanged.
    let mut untouched = commit_row();
    sanitize_fields(&mut untouched, &security, &context).expect("sanitize");
    assert_eq!(untouched, commit_row(), "default output must be unchanged");
    // Opt-in path: every email leaf is masked, structure preserved.
    let mut redacted = commit_row();
    redact_email_fields(&mut redacted, &security, &context).expect("redact");
    assert_eq!(
        redacted.pointer("/commits/0/author/email"),
        Some(&json!("[REDACTED-EMAIL]"))
    );
    assert_eq!(
        redacted.pointer("/commits/0/message"),
        Some(&json!("fix: reported by [REDACTED-EMAIL]"))
    );
    assert_eq!(
        redacted.pointer("/commits/0/author/name"),
        Some(&json!("Dev One"))
    );
}

/// File text a tool returns beside its source-code `path`
/// is not re-redacted as YAML by the output net, while every credential in
/// it, and `password:` in config files, stay redacted.
#[test]
fn output_net_applies_source_code_context_from_the_result_path() {
    let context = sanitize_context();
    let security = crate::security::ContentSecurity::new();
    let member = "interface Cfg {\n  password: hunter2hunter2hunter2;\n}\n";
    let token = format!("ghp_{}", "a".repeat(37));
    let pem = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEAxyz\n-----END RSA PRIVATE KEY-----";
    let run = |path: &str, content: &str| {
        let mut value = json!({"files":[{"path": path, "matches":[{"value": content}]}]});
        sanitize_fields(&mut value, &security, &context).expect("sanitize");
        value["files"][0]["matches"][0]["value"]
            .as_str()
            .expect("value")
            .to_owned()
    };
    for path in ["src/a.ts", "crates/b.rs"] {
        assert_eq!(run(path, member), member, "{path}");
        let credential = format!("const t = \"{token}\";\n{pem}\n");
        let out = run(path, &credential);
        assert!(!out.contains(&token) && !out.contains("MIIEow"), "{out}");
    }
    for path in ["config/app.yaml", ".env", "Procfile", ""] {
        let out = run(path, member);
        assert!(!out.contains("hunter2"), "{path}: {out}");
    }
    // No `path` anywhere: the pathless net applies as before.
    let mut bare = json!({"content": member});
    sanitize_fields(&mut bare, &security, &context).expect("sanitize");
    assert!(
        !bare["content"]
            .as_str()
            .expect("content")
            .contains("hunter2")
    );
}

#[test]
fn sanitizer_redacts_real_secret_in_next_query_and_location() {
    // A real GitHub token embedded in a continuation query or a location
    // leaf is now redacted — better to break the continuation than to leak.
    let token = format!("ghp_{}", "a".repeat(37));
    let mut value = json!({
        "next": {
            "continueWindow": {
                "tool": "localFetch",
                "query": {"queries":[{"path": format!("/repo/{token}.rs")}]}
            }
        },
        "location": {"localPath": format!("/tmp/{token}/repo")}
    });
    let context = sanitize_context();
    let security = crate::security::ContentSecurity::new();
    sanitize_fields(&mut value, &security, &context).expect("sanitize");
    // Tool preserved so the call is still routable.
    assert_eq!(
        value.pointer("/next/continueWindow/tool"),
        Some(&json!("localFetch"))
    );
    let query_path = value
        .pointer("/next/continueWindow/query/queries/0/path")
        .and_then(Value::as_str)
        .expect("query path");
    assert!(!query_path.contains("ghp_"), "token leaked in query path");
    let local_path = value
        .pointer("/location/localPath")
        .and_then(Value::as_str)
        .expect("localPath");
    assert!(!local_path.contains("ghp_"), "token leaked in location");
}

/// A later row of a batch gets the recovery its own query earns: the
/// fallback reads the query it is given, whatever the row's `index`.
#[test]
fn a_later_row_recovers_with_its_own_query() {
    let query = json!({"owner":"o","keywords":["a"],"matchIn":"path"});
    let mut row = json!({"index": 2, "status": "empty", "data": {}});
    ensure_recovery(&mut row, ToolId::GhSearchCode, &query);
    assert_eq!(
        row["data"]["hints"],
        json!([ToolId::GhSearchCode.output().fallback_hint(&query)]),
        "{row}"
    );
}

/// A page past the end flagged on a resource item's section (a pull
/// request's files) is the row's outcome: the page cursor is the row's.
#[test]
fn an_item_section_past_its_end_settles_the_row() {
    let query = json!({"operation":"pullRequest","owner":"o","repo":"r","number":4,
        "sections":["files"],"page":5});
    let data = json!({"pullRequests":[{"number":4,"files":[],
        "contentPagination":{"files":{"currentPage":5,"totalPages":1,"hasMore":false,"outOfRange":true}}}]});
    let row = result_row(ToolId::GhGetHistoryItem, 0, &query, data, None);
    assert_eq!(row["status"], "outOfRange", "{row}");
    assert_eq!(row["data"]["reason"], "page 5 of 1", "{row}");
    assert_eq!(
        row["data"]["next"]["restart"]["tool"], "ghGetHistoryItem",
        "{row}"
    );
    assert!(
        row["data"]["next"]["restart"]["query"]["queries"][0]
            .get("page")
            .is_none(),
        "{row}"
    );
}

/// A resource item's section blocks (a pull request's `contentPagination`)
/// carry no cursor, as the row's own do: `next` holds every page. The row's
/// `isPartial` does not restate the `hasMore` that its `next` page answers.
#[test]
fn nested_page_blocks_drop_cursors_and_the_restated_partial_flag() {
    let query = json!({"operation":"pullRequest","owner":"o","repo":"r","number":4,
        "sections":["files"],"extras":["stats"]});
    let next = crate::response::result::Continuation::new(
        ToolId::GhGetHistoryItem,
        json!({"operation":"pullRequest","owner":"o","repo":"r","number":4,
            "sections":["files"],"page":2}),
    )
    .build();
    let data = json!({"pullRequests":[{"number":4,"files":["M +1 -1 a.rs"],
        "contentPagination":{"files":{"currentPage":1,"totalPages":2,"pageSize":1,
            "totalItems":2,"hasMore":true,"nextPage":2,"snapshot":"s"}}}],
        "isPartial":true,"next":{"nextFilePage":next}});
    let mut row = result_row(ToolId::GhGetHistoryItem, 0, &query, data, None);
    minimize_row(&mut row, ToolId::GhGetHistoryItem, &query);
    let block = &row["data"]["pullRequests"][0]["contentPagination"]["files"];
    assert_eq!(block["hasMore"], true, "{row}");
    assert!(
        block.get("nextPage").is_none() && block.get("snapshot").is_none(),
        "{row}"
    );
    assert!(row["data"].get("isPartial").is_none(), "{row}");
    assert!(row.get("status").is_none(), "{row}");
    assert_eq!(row["data"]["next"]["nextFilePage"], next, "{row}");
}

/// An empty row whose only call is a retry (GitHub's incomplete results)
/// stays empty, so the call exits 1; a next page still makes it a success.
#[test]
fn an_empty_row_offering_only_a_retry_stays_empty() {
    let call = json!({"tool":"ghSearchRepo","query":{"queries":[{"keywords":["mcp"]}]}});
    let query = json!({"keywords":["mcp"]});
    let retry = json!({"repositories":[],"isPartial":true,
        "partialReasons":["providerIncompleteResults"],"next":{"retry":call}});
    let row = result_row(ToolId::GhSearchRepo, 0, &query, retry, Some("empty"));
    assert_eq!(row["status"], "empty", "{row}");
    let paged = json!({"repositories":[],"next":{"nextPage":call}});
    let row = result_row(ToolId::GhSearchRepo, 0, &query, paged, Some("empty"));
    assert_ne!(row["status"], "empty", "{row}");
}
