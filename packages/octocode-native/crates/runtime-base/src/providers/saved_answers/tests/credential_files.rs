//! A provider answer that shows a credential file stays off disk, whichever read
//! fetched it: a code search page, a contents read or a patch.
use super::*;
use octocode_test_support::json_fixture;

/// A search page's index fragments are file text: one that lists a
/// credential-named file stays off disk whatever its text looks like.
#[test]
fn a_code_search_page_listing_a_credential_file_stays_off_disk() {
    let security = ContentSecurity::new();
    let page = |path: &str| {
        serde_json::json!({"items":[{"path":path,"text_matches":[{"fragment":"a plain value"}]}]})
            .to_string()
    };
    let answer = |request: &str, body: &str| exposing(request, "plain", None, body);
    let search = "GET https://x/search/code?q=needle";
    for withheld in [".env", ".Env.Prod", "KEY.PEM", "a/.ssh/config"] {
        assert!(
            !gate_keeps(&security, &answer(search, &page(withheld))),
            "{withheld}"
        );
    }
    assert!(gate_keeps(&security, &answer(search, &page("src/app.rs"))));
    // Another answer that merely has such a field is judged as before.
    assert!(gate_keeps(
        &security,
        &answer("GET https://x/repos/o/r/git/trees/s", &page(".env"))
    ));
}

/// A contents read names its file in the request: a credential-named one stays
/// off disk whatever its text looks like, raw or as a JSON payload.
#[test]
fn a_contents_read_of_a_credential_file_stays_off_disk() {
    let security = ContentSecurity::new();
    let answer = |file: &str| {
        exposing(
            &format!("GET https://x/api/v3/repos/o/r/contents/{file}?ref=main"),
            "plain",
            None,
            "a plain value\n",
        )
    };
    for withheld in [
        ".env",
        ".Env.Prod",
        "KEY.PEM",
        "a%2F.ssh%2Fconfig",
        "a%2Fid_rsa.bak",
    ] {
        assert!(!gate_keeps(&security, &answer(withheld)), "{withheld}");
    }
    for kept in [
        "src%2Fa.rs",
        ".github%2Fci.yml",
        "src%2Fcredentials%2Fmod.rs",
    ] {
        assert!(gate_keeps(&security, &answer(kept)), "{kept}");
    }
    // The commit-date lookup names the file only in its query, and holds no text.
    assert!(gate_keeps(
        &security,
        &exposing(
            "GET https://x/repos/o/r/commits?path=.env",
            "plain",
            None,
            "[]"
        )
    ));
}

/// A changed-file list's `patch` is the file's text: one for a credential-named
/// file, by its name or a rename's old name, stays off disk. A file with no
/// patch holds no text, and an ordinary file's patch is judged as before.
#[test]
fn a_patch_of_a_credential_file_stays_off_disk() {
    let security = ContentSecurity::new();
    let file = |fields: serde_json::Value| fields;
    let commit = "GET https://x/api/v3/repos/o/r/commits/abc";
    let compare = "GET https://x/api/v3/repos/o/r/compare/a...b";
    let files = "GET https://x/api/v3/repos/o/r/pulls/7/files?per_page=100&page=1";
    let answers = |entry: serde_json::Value| {
        [
            (
                commit,
                serde_json::json!({"sha": "abc", "files": [entry.clone()]}),
            ),
            (
                compare,
                serde_json::json!({"status": "ahead", "files": [entry.clone()]}),
            ),
            (files, serde_json::json!([entry])),
        ]
    };
    for withheld in [
        file(json_fixture(
            r#"{"filename": ".env", "patch": "+a plain value"}"#,
        )),
        file(json_fixture(
            r#"{"filename": "a/.Env.Prod", "patch": "+a plain value"}"#,
        )),
        file(json_fixture(
            r#"{"filename": "deploy/KEY.PEM", "patch": "+a plain value"}"#,
        )),
        file(json_fixture(
            r#"{
                "filename": "x.sample",
                "previous_filename": "a/.env",
                "patch": "+a plain value"
            }"#,
        )),
    ] {
        for (request, body) in answers(withheld.clone()) {
            assert!(
                !gate_keeps(
                    &security,
                    &exposing(request, "plain", None, &body.to_string())
                ),
                "{request}: {withheld}"
            );
        }
    }
    for kept in [
        file(json_fixture(
            r#"{"filename": "src/app.rs", "patch": "+a plain value"}"#,
        )),
        file(json_fixture(
            r#"{
                "filename": "src/credentials/mod.rs",
                "patch": "+a plain value"
            }"#,
        )),
        file(json_fixture(r#"{"filename": ".env", "additions": 1}"#)),
        file(json_fixture(r#"{"filename": ".env", "patch": null}"#)),
    ] {
        for (request, body) in answers(kept.clone()) {
            assert!(
                gate_keeps(
                    &security,
                    &exposing(request, "plain", None, &body.to_string())
                ),
                "{request}: {kept}"
            );
        }
    }
}
