//! A search matches the text a read serves: bytes a read withholds make no
//! hit, count or listing in any localSearch view, so a regex walk over a
//! hidden value learns nothing of it; ordinary text and the placeholders a
//! read shows count as a read's `matchString` counts them. All key material
//! and secrets are fabricated.

use crate::support::{Workspace, call, query_path};
use serde_json::{Value, json};

/// A private-key body (its first line opens with `aMD5auQ`), and a value a
/// credential name makes secret, beside ordinary text.
const SOURCE: &str = "# config fixture\n\
-----BEGIN RSA PRIVATE KEY-----\n\
aMD5auQx7Tq2Lw9Pz4Vb8Nc3Kd6Hf1Jg5Rs0Ym2Xe7Wu4Ai9Oo3Pl8Ki1Uj6Yh5Tg\n\
uHe92vPWitM19kMjQOL2WFs24a8umkmlSlNvgKRi4WazQJcJLI5YfTSg5xmk7Gtt\n\
12AHJEbySvC2M3f94yoLngVEvpjoWponyy42EUQrVXc7OHK7r+LHkQ0CQtn6f9R5\n\
GID8oALScFwSq8QUTrTMNjt1vFm/OggvV/WWqzn/14Za0zHRVUAXgZdzJPJLb8jk\n\
sXO9T06LUONoub+PfCmycfYV3VG5c35JxAQPKGZo5LBJpWr3g79KfusAfRdEUaRa\n\
-----END RSA PRIVATE KEY-----\n\
ordinary line hello world\n\
db_password = \"Hx8vQ2mLp9Zr4Tn7Wk3Bd\"  # hello world\n";

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
    let ahead = json!({"matchString": ["db_password = \"(?=Hx8vQ)"], "regexEngine": "pcre2", "view": "countMatches"});
    assert_eq!(hits(&run("localSearch", ahead).await), 0);
    let term = json!({"matchString": ["hello"], "fileHas": ["Hx8vQ2m"], "view": "files"});
    assert_eq!(hits(&run("localSearch", term).await), 0);

    // Ordinary text on the same lines, and the placeholders a read shows,
    // count in every view as a read's matchString counts them.
    for pattern in ["hello", "\\[REDACTED-", "db_password"] {
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
}
