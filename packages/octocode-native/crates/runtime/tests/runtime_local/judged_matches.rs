//! A search matches the text a read serves: bytes a read withholds make no
//! hit, count or listing in any localSearch view, so a regex walk over a
//! hidden value learns nothing of it; ordinary text counts as a read's
//! `matchString` counts it, and the placeholders a read shows, text it
//! wrote, count in no view. All key material and secrets are fabricated.

use crate::support::{Workspace, call, query_path};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

/// A private-key body (its first line opens with `aMD5auQ`), and a fake
/// known-format GitHub token, beside ordinary text.
const SOURCE: &str = "# config fixture\n\
-----BEGIN RSA PRIVATE KEY-----\n\
aMD5auQx7Tq2Lw9Pz4Vb8Nc3Kd6Hf1Jg5Rs0Ym2Xe7Wu4Ai9Oo3Pl8Ki1Uj6Yh5Tg\n\
uHe92vPWitM19kMjQOL2WFs24a8umkmlSlNvgKRi4WazQJcJLI5YfTSg5xmk7Gtt\n\
12AHJEbySvC2M3f94yoLngVEvpjoWponyy42EUQrVXc7OHK7r+LHkQ0CQtn6f9R5\n\
GID8oALScFwSq8QUTrTMNjt1vFm/OggvV/WWqzn/14Za0zHRVUAXgZdzJPJLb8jk\n\
sXO9T06LUONoub+PfCmycfYV3VG5c35JxAQPKGZo5LBJpWr3g79KfusAfRdEUaRa\n\
-----END RSA PRIVATE KEY-----\n\
ordinary line hello world\n\
db_password = \"ghp_Hx8vQ2mLp9Zr4Tn7Wk3BdHx8vQ2mLp9Zr4Tn\"  # hello world\n";

const VIEWS: [&str; 5] = [
    "content",
    "matchOnly",
    "files",
    "countLines",
    "countMatches",
];

/// The hits a localSearch row reports: its match count, with no file
/// listed when it is none.
fn hits(data: &Value) -> u64 {
    let count = data["matchCount"].as_u64().unwrap_or(0);
    if count == 0 {
        assert!(data["files"].as_array().is_none_or(Vec::is_empty), "{data}");
    }
    count
}

#[tokio::test]
async fn a_search_never_matches_what_a_read_withholds() {
    let workspace = Workspace::new();
    let path = workspace.write("fx/pem.txt", SOURCE);
    let runtime = workspace.runtime(&[]);
    let run = |tool: &'static str, query: Value| {
        let runtime = &runtime;
        let query = query_path(&path, query);
        async move {
            let outcome = call(runtime, tool, query.clone())
                .await
                .unwrap_or_else(|error| panic!("{tool} {query}: {error:?}"));
            let output = outcome.structured_content.into_value();
            assert!(
                output.pointer("/results/0/data/errorCode").is_none(),
                "{tool} {query}: {output}"
            );
            output["results"][0]["data"].clone()
        }
    };

    // Each half of a regex walk over a hidden value, and the hidden text
    // itself, find nothing in any view: no hit, count or listing.
    for pattern in [
        "^aMD5au[A-M]",
        "^aMD5au[N-Z]",
        "aMD5auQx7",
        "Hx8v[A-M]",
        "Hx8v[N-Z]",
        "Hx8vQ2m",
    ] {
        for view in VIEWS {
            let query = json!({"matchString": [pattern], "regexEngine": "rust", "view": view});
            let data = run("localSearch", query.clone()).await;
            assert_eq!(hits(&data), 0, "{pattern} {view}: {data}");
        }
    }
    // What surrounds a hidden value does not see it either: a lookahead
    // into it, or a file-level term spelling it.
    let ahead = json_fixture(
        r#"{
            "matchString": ["\\w+ = \"(?=ghp_Hx8vQ)"],
            "regexEngine": "pcre2",
            "view": "countMatches"
        }"#,
    );
    assert_eq!(hits(&run("localSearch", ahead).await), 0);
    let term = json_fixture(
        r#"{
            "matchString": ["hello"],
            "fileHas": ["Hx8vQ2m"],
            "view": "files"
        }"#,
    );
    assert_eq!(hits(&run("localSearch", term).await), 0);

    // Ordinary text on the same lines counts in every view as a read's
    // matchString counts it.
    for pattern in ["hello", "db_password"] {
        let fetched = run(
            "localFetch",
            json!({"matchString": [pattern], "regexEngine": "rust"}),
        )
        .await;
        let read = fetched["selectedMatchCount"]
            .as_u64()
            .unwrap_or_else(|| panic!("{pattern}: {fetched}"));
        assert!(read > 0, "{pattern}: {fetched}");
        for view in VIEWS {
            let query = json!({"matchString": [pattern], "regexEngine": "rust", "view": view});
            let data = run("localSearch", query).await;
            let count = match view {
                "countLines" => data["matchedLineCount"].as_u64().unwrap_or(0),
                _ => hits(&data),
            };
            assert_eq!(count, read, "{pattern} {view}: {data}");
        }
    }
    // The placeholders a read shows are text it wrote, not the file's: the
    // read shows them, a search for them matches nothing in any view, and
    // its empty row says masked secrets are not searched.
    let placeholder = "\\[REDACTED-";
    let fetched = run(
        "localFetch",
        json!({"matchString": [placeholder], "regexEngine": "rust"}),
    )
    .await;
    assert!(
        fetched["selectedMatchCount"]
            .as_u64()
            .is_some_and(|read| read > 0),
        "{fetched}"
    );
    for view in VIEWS {
        let query = json!({"matchString": [placeholder], "regexEngine": "rust", "view": view});
        let outcome = call(&runtime, "localSearch", query_path(&path, query.clone()))
            .await
            .unwrap_or_else(|error| panic!("{query}: {error:?}"));
        let served = outcome.structured_content.into_value();
        let data = &served["results"][0]["data"];
        assert_eq!(hits(data), 0, "{view}: {served}");
        assert_eq!(
            data["matchedLineCount"].as_u64().unwrap_or(0),
            0,
            "{view}: {served}"
        );
        assert!(
            served
                .to_string()
                .contains("masked secrets are not searched"),
            "{view}: {served}"
        );
    }
}

/// A JavaScript source whose known-format token literal a read hides, between
/// an ordinary literal and a declaration that follows it.
const SCRIPT: &str = "const greeting = \"hello\";\nconst db_password = \"ghp_Hx8vQ2mLp9Zr4Tn7Wk3BdHx8vQ2mLp9Zr4Tn\";\nconst after = 1;\n";

/// astSearch matches the text a read serves: a `regex` walk over a hidden
/// value, or a `follows` rule that depends on it, finds nothing in the match
/// and count views, for the file alone and for its directory; an ordinary
/// match keeps its line and column.
#[tokio::test]
async fn an_ast_match_never_reads_what_a_read_withholds() {
    let workspace = Workspace::new();
    let path = workspace.write("ast/app.js", SCRIPT);
    let dir = path.parent().expect("dir").to_owned();
    let runtime = workspace.runtime(&[]);
    let run = |target: &std::path::Path, query: Value| {
        let runtime = &runtime;
        let query = query_path(target, query);
        async move {
            let outcome = call(runtime, "astSearch", query.clone())
                .await
                .unwrap_or_else(|error| panic!("astSearch {query}: {error:?}"));
            let output = outcome.structured_content.into_value();
            assert!(
                output.pointer("/results/0/data/errorCode").is_none(),
                "astSearch {query}: {output}"
            );
            output["results"][0]["data"].clone()
        }
    };
    let follows = |regex: &str| json!({"kind":"lexical_declaration","follows":{"kind":"lexical_declaration","has":{"kind":"string_fragment","regex":regex,"stopBy":"end"}}});
    for rule in [
        json_fixture(r#"{"kind": "string_fragment", "regex": "^ghp_Hx8v[A-M]"}"#),
        json_fixture(r#"{"kind": "string_fragment", "regex": "^ghp_Hx8v[N-Z]"}"#),
        follows("^ghp_Hx8v[A-M]"),
        follows("^ghp_Hx8v[N-Z]"),
    ] {
        for target in [path.as_path(), dir.as_path()] {
            for view in ["content", "countMatches"] {
                let query = json!({"operation":"match","rule":rule,"view":view});
                let data = run(target, query).await;
                assert_eq!(hits(&data), 0, "{rule} {view} {}: {data}", target.display());
            }
        }
    }
    // What a read shows still matches, where it is.
    for target in [path.as_path(), dir.as_path()] {
        let rule = json_fixture(r#"{"kind": "string_fragment", "regex": "^hel"}"#);
        let data = run(
            target,
            json!({"operation":"match","rule":rule,"includeCaptures":true}),
        )
        .await;
        assert_eq!(hits(&data), 1, "{data}");
        let row = &data["files"][0]["matches"][0];
        assert_eq!(
            (&row["line"], &row["column"]),
            (&json!(1), &json!(19)),
            "{data}"
        );
        let data = run(
            target,
            json!({"operation":"match","rule":follows("^hel"),"view":"countMatches"}),
        )
        .await;
        assert_eq!(hits(&data), 1, "{data}");
    }
}

/// D5 (bin16 retest): a context row whose first line is a key's END armor,
/// numbered past 999, keeps its number and its armor: the label a numbered
/// view writes is no key text. The key's body stays hidden.
#[tokio::test]
async fn a_numbered_armor_line_keeps_its_number() {
    let filler = "2026-11-12 ordinary log text here\n".repeat(1500);
    let body = "MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7Zq8Rm4Lx2Vt9Wp\n".repeat(3);
    let text = format!(
        "{filler}-----BEGIN PRIVATE KEY-----\n{body}-----END PRIVATE KEY-----\nafter one\nafter two\n"
    );
    let workspace = Workspace::new();
    let path = workspace.write("fx/mid.log", &text);
    let runtime = workspace.runtime(&[]);
    let query = query_path(
        &path,
        json_fixture(
            r#"{
                "matchString": ["BEGIN|END"],
                "regexEngine": "rust",
                "contextLines": {"before": 0, "after": 2}
            }"#,
        ),
    );
    let output = call(&runtime, "localSearch", query)
        .await
        .expect("search")
        .structured_content
        .into_value();
    let rows = &output["results"][0]["data"]["files"][0]["matches"];
    let begin = rows[0]["value"].as_str().expect("begin row");
    let end = rows[1]["value"].as_str().expect("end row");
    assert!(
        begin.starts_with("1501\t-----BEGIN PRIVATE KEY-----\n1502\t[REDACTED-"),
        "{begin}"
    );
    assert!(!begin.contains("MIIEvQ"), "{begin}");
    assert_eq!(
        end, "1505\t-----END PRIVATE KEY-----\n1506\tafter one\n1507\tafter two",
        "{output}"
    );
}

/// D6 (bin16 retest): an empty match follows rg's rule in matchOnly: a
/// line's matches end with the line, so no row sits past the end of the
/// file, none reads as unreadable, and no warning says so.
#[tokio::test]
async fn match_only_empty_matches_end_with_their_line() {
    let workspace = Workspace::new();
    let path = workspace.write("fx/clean.txt", "nothing secret here\njust words\n");
    let runtime = workspace.runtime(&[]);
    let run = |pattern: &'static str| {
        let runtime = &runtime;
        let query = query_path(
            &path,
            json!({"matchString": [pattern], "regexEngine": "rust", "view": "matchOnly"}),
        );
        async move {
            call(runtime, "localSearch", query)
                .await
                .expect("search")
                .structured_content
                .into_value()["results"][0]["data"]
                .clone()
        }
    };
    let lines = run("^.*$").await;
    let rows = lines["files"][0]["matches"].as_array().expect("rows");
    let shown: Vec<(u64, &str)> = rows
        .iter()
        .map(|row| {
            (
                row["line"].as_u64().expect("line"),
                row["value"].as_str().expect("value"),
            )
        })
        .collect();
    assert_eq!(
        shown,
        [(1, "nothing secret here"), (2, "just words")],
        "{lines}"
    );
    assert_eq!(lines["matchCount"], 2, "{lines}");
    assert!(lines.get("warnings").is_none(), "{lines}");
    let empty = run("x*").await;
    assert_eq!(empty["matchCount"], 31, "{empty}");
    assert!(!empty.to_string().contains("REDACTED"), "{empty}");
}
