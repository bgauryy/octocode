//! localSearch through the runtime: literal hits, match pages, default-exclude disclosure, match limits and long values.

use crate::support::{Run as _, Workspace, call, row_data, row_status};
use serde_json::json;

#[tokio::test]
async fn local_search_finds_literal_matches() {
    let workspace = Workspace::new();
    workspace.write("src/main.ts", "export function needle() { return 1; }\n");
    workspace.write("src/other.ts", "const unused = 2;\n");
    let runtime = workspace.runtime(&[]);
    let found = call(
        &runtime,
        "localSearch",
        json!({
            "path": workspace.workspace,
            "matchString": "needle",
            "regex": "literal"
        }),
    )
    .await
    .expect("search");
    assert_eq!(row_status(&found), "success");
    let rendered = serde_json::to_string(row_data(&found)).expect("json");
    assert!(
        rendered.contains("main.ts"),
        "expected main.ts in {rendered}"
    );

    runtime.close().await;
}

/// `matchPageSize:1` is `rg -m1`: one row per file, and at most one
/// continuation for the whole response, never one per file.
#[tokio::test]
async fn local_search_match_page_size_one_is_first_hit_per_file() {
    let workspace = Workspace::new();
    for index in 0..18 {
        workspace.write(
            &format!("conv/c{index:02}.cs"),
            format!("#region License\nclass C{index} {{}}\n#region Other\n"),
        );
    }
    let runtime = workspace.runtime(&[]);
    let found = call(
        &runtime,
        "localSearch",
        json!({
            "path": workspace.workspace.join("conv"),
            "matchString": "#region",
            "matchPageSize": 1,
            "extras": []
        }),
    )
    .await
    .expect("search");
    assert_eq!(
        row_status(&found),
        "success",
        "{}",
        found.structured_content
    );
    let data = row_data(&found);
    let files = data["files"].as_array().expect("files");
    assert_eq!(files.len(), 18, "{data}");
    for file in files {
        assert_eq!(file["matches"].as_array().map(Vec::len), Some(1), "{file}");
        assert!(file.get("next").is_none(), "{file}");
        assert!(file.get("hints").is_none(), "{file}");
    }
    // The rest of every file is one match page, named once.
    let next = data["next"].as_object().expect("next");
    assert_eq!(next.keys().collect::<Vec<_>>(), ["nextMatchPage"], "{data}");
    runtime.close().await;
}

/// A localSearch that found matches still discloses what the default
/// excludes skipped (pruned build dirs and generated files such as
/// `*.min.js`), with the same search over them one call away; the escape
/// flag really searches them. Lockfiles pin dependency versions: evidence,
/// searched by default.
#[tokio::test]
async fn local_search_discloses_default_excluded_dirs_and_files_with_a_rerun() {
    let workspace = Workspace::new();
    workspace.write("drivers/scsi/core.c", "transport_free_cmd(cmd);\n");
    workspace.write(
        "drivers/target/transport.c",
        "void transport_free_cmd(void) {}\n",
    );
    workspace.write("dist/bundle.js", "transport_free_cmd();\n");
    workspace.write("Cargo.lock", "transport_free_cmd = 1\n");
    workspace.write("vendor/lib.min.js", "transport_free_cmd();\n");
    let runtime = workspace.runtime(&[]);
    let root = workspace.workspace.clone();
    let files_of = |outcome: &octocode_native::response::outcome::ToolOutcome| -> Vec<String> {
        let mut files = row_data(outcome)["files"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|file| file["path"].as_str().or_else(|| file.as_str()))
            .map(|path| path.split(' ').next().unwrap_or(path).to_owned())
            .collect::<Vec<_>>();
        files.sort();
        files
    };

    let found = call(
        &runtime,
        "localSearch",
        json!({"path":root,"matchString":"transport_free_cmd","resultView":"files"}),
    )
    .await
    .expect("search");
    let text = found.structured_content.to_string();
    assert_eq!(
        files_of(&found),
        ["Cargo.lock", "drivers/scsi/core.c"],
        "{text}"
    );
    assert_eq!(
        row_data(&found)["skippedPaths"],
        json!({"entries":[
            {"path":"dist","cause":"default-excluded"},
            {"path":"drivers/target","cause":"default-excluded"},
            {"path":"vendor/lib.min.js","cause":"default-excluded"}
        ],"sampled":false}),
        "{text}"
    );
    assert!(row_data(&found).get("warnings").is_none(), "{text}");
    let hint = crate::support::checks::hint(row_data(&found), "includeSkipped");
    let lead = (!hint.is_null())
        .then_some(&hint)
        .or_else(|| {
            found
                .structured_content
                .pointer("/results/0/data/next/includeSkipped")
        })
        .unwrap_or_else(|| panic!("includeSkipped lead: {text}"));
    assert_eq!(
        lead["query"]["queries"][0]["defaultExcludes"], false,
        "{lead}"
    );

    let all = call(&runtime, "localSearch", lead["query"].clone())
        .await
        .expect("rerun");
    assert_eq!(
        files_of(&all),
        [
            "Cargo.lock",
            "dist/bundle.js",
            "drivers/scsi/core.c",
            "drivers/target/transport.c",
            "vendor/lib.min.js",
        ],
        "{}",
        all.structured_content
    );
    runtime.close().await;
}

/// A regex read past the match limit says how to reach the unselected
/// matches without restating a call, on both surfaces, also on the first
/// page of a read that continues. The search lead that pages every match is
/// opt-in (`extras: ["hints"]`) and then rides `hints` (pages ride `next`).
#[tokio::test]
async fn regex_match_limit_warning_and_its_opt_in_search_lead() {
    let workspace = Workspace::new();
    let body: String = (1..=100_005).map(|n| format!("hit {n}\n")).collect();
    let path = workspace.write("hits.txt", body);
    let runtime = workspace.runtime(&[]);
    for hinted in [false, true] {
        let mut row = json!({"path":path.to_string_lossy(),"matchString":"hit","regex":"rust"});
        if hinted {
            row["extras"] = json!(["hints"]);
        }
        let query = json!({"queries":[row]});
        for mcp in [false, true] {
            let envelope = if mcp {
                runtime
                    .run_mcp("limit-mcp".into(), "localFetch".into(), query.clone())
                    .await
                    .expect("mcp call")["structuredContent"]
                    .clone()
            } else {
                runtime
                    .run("limit-cli".into(), "localFetch".into(), query.clone())
                    .await
                    .expect("cli call")
                    .structured_content
                    .into_value()
            };
            let data = &envelope["results"][0]["data"];
            let warning = data["warnings"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .find(|w| w.contains("match limit"))
                .unwrap_or_else(|| panic!("mcp={mcp}: no match-limit warning: {data}"));
            assert!(
                data["next"]["continueWindow"].is_object(),
                "mcp={mcp}: the read pages"
            );
            assert!(
                warning.ends_with("narrow the regex to select them."),
                "mcp={mcp}: {warning}"
            );
            if hinted {
                assert_eq!(
                    crate::support::checks::hint(data, "findText")["tool"],
                    "localSearch",
                    "mcp={mcp}: {data}"
                );
            } else {
                assert!(
                    data.get("hints")
                        .is_none_or(|hints| hints.get("findText").is_none()),
                    "mcp={mcp}: the search lead is opt-in: {data}"
                );
            }
        }
    }
}

/// A value clipped to the display bound is not a gap in the rows: the row
/// stays complete, the clip is reported by `clipped` and `originalChars`, and its
/// `expandValues` lead is optional, so the call is not partial.
#[tokio::test]
async fn a_clipped_match_value_is_not_a_partial_result() {
    use octocode_native::response::exit::ExitClass;
    let workspace = Workspace::new();
    workspace.write("min.js", format!("needle {}\n", "x ".repeat(5_000)));
    let runtime = workspace.runtime(&[]);
    let outcome = runtime
        .run(
            "clipped-value".into(),
            "localSearch".into(),
            json!({"queries":[{"path":workspace.workspace.to_string_lossy(),"matchString":"needle"}]}),
        )
        .await
        .expect("localSearch");
    let data = row_data(&outcome);
    let matched = &data["files"][0]["matches"][0];
    assert!(matched["originalChars"].as_u64().is_some(), "{data}");
    assert_eq!(matched["clipped"], true, "{data}");
    assert!(data.get("isPartial").is_none(), "{data}");
    assert!(data["next"]["expandValues"].is_object(), "{data}");
    assert_eq!(outcome.exit_class(), ExitClass::Success, "{data}");
    runtime.close().await;
}

/// On a line longer than a read page the match is far from the line's start:
/// the `expandValues` read windows the line by bytes at the match (a
/// multi-byte prefix included), not at the line's start.
#[tokio::test]
async fn expand_values_of_a_long_line_windows_around_the_match() {
    let workspace = Workspace::new();
    let line = format!(
        "{}needleHere{}",
        "é ".repeat(100_000),
        " tail".repeat(1_000)
    );
    workspace.write("min.js", format!("head\n{line}\ntail\n"));
    let runtime = workspace.runtime(&[]);
    let outcome = runtime
        .run(
            "long-line".into(),
            "localSearch".into(),
            json!({"queries":[{"path":workspace.workspace.to_string_lossy(),"matchString":"needleHere"}]}),
        )
        .await
        .expect("localSearch");
    let data = row_data(&outcome);
    let lead = &data["next"]["expandValues"];
    let read = &lead["query"]["queries"][0];
    assert_eq!(lead["tool"], "localFetch", "{data}");
    assert_eq!(read["unit"], "bytes", "{data}");
    assert_eq!(read["ranges"], json!(["2-2"]), "{data}");
    let offset = read["offset"].as_u64().expect("offset") as usize;
    assert!(offset > 100_000, "{read}");
    let page = runtime
        .run(
            "long-line-read".into(),
            "localFetch".into(),
            lead["query"].clone(),
        )
        .await
        .expect("localFetch");
    let page = row_data(&page);
    assert!(
        page["content"]
            .as_str()
            .is_some_and(|text| text.contains("needleHere")),
        "the window holds the match: {page}"
    );
    runtime.close().await;
}
