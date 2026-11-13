//! The classification egress applies one name rule whatever the source: a
//! credential-named GitHub file is refused before any fetch, on every route
//! to the provider, with `security.credentialFiles: allow` or without it. A
//! read without classification sends nothing to a provider and is unchanged.

use super::{clasify, noul, sent};
use crate::support::{self, Run as _, Workspace, clasify_runtime, github_api};
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};
use std::path::Path;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

mod direct;
mod held_pages;
mod saved_plan;
mod search;

/// A fake value in no format the redaction scanner knows.
pub(crate) const MARKER: &str = "plain-value-in-no-known-format-7741";
const NORMAL: &str = "the needle in an ordinary guide";
const SHA: &str = "1111111111111111111111111111111111111111";
/// GitHub's raw media type: the file's own bytes, as the reads ask for them, so
/// a kept copy holds the marker as text and a scan of the disk can see it.
pub(crate) const RAW_FILE: &str = "application/vnd.github.raw+json";

/// Every spelling here names a withheld file. `./.env` and `a/../.env` read the
/// same file as `.env`; `.config/gh/` is a credential store named by a fragment.
const WITHHELD: &[&str] = &[
    ".env",
    ".Env.Prod",
    "KEY.PEM",
    "server.pem",
    ".ssh/config",
    "a/.ssh/config",
    "./.env",
    "a/../.env",
    ".config/gh/hosts.yml",
    "id_rsa.bak",
];

/// A repository that serves every path, so a read the rule should have stopped
/// succeeds and is seen: a withheld name holds [`MARKER`], any other [`NORMAL`].
pub(crate) async fn github() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/octo/demo/commits/.+"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": SHA})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/octo/demo/contents/.+"))
        .respond_with(|request: &Request| {
            let body = if request.url.path().contains("guide") || request.url.path().contains("app")
            {
                NORMAL.to_owned()
            } else {
                format!("needle {MARKER}\n")
            };
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_body_raw(body, RAW_FILE)
        })
        .mount(&server)
        .await;
    server
}

/// Answers each question by its type: a predicate's score selects the
/// candidate, anything else is Noul 0.9.
async fn provider() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &Request| {
            let body: Value = request.body_json().expect("provider request");
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .iter()
                .map(|(id, question)| {
                    let answer = question["criteria"].as_array().map_or_else(
                        || json!({"type":"noul","noul":0.9}),
                        |criteria| {
                            let legend = criteria
                                .iter()
                                .enumerate()
                                .map(|(index, level)| (index.to_string(), level.clone()))
                                .collect::<serde_json::Map<_, _>>();
                            json!({"type":"score","score":2,"confidence":1,"legend":legend,
                                "probabilities":{"0":0,"1":0,"2":1}})
                        },
                    );
                    (id.clone(), answer)
                })
                .collect::<serde_json::Map<_, _>>();
            ResponseTemplate::new(200).set_body_json(json!({"model":"fixture","answers":answers,
                "usage":{"input_tokens":10,"output_tokens":2}}))
        })
        .mount(&server)
        .await;
    server
}

/// The paths read through the contents API.
pub(crate) async fn contents_reads(github: &MockServer) -> Vec<String> {
    github
        .received_requests()
        .await
        .expect("GitHub requests")
        .iter()
        .filter(|request| request.url.path().contains("/contents/"))
        .map(|request| request.url.path().to_owned())
        .collect()
}

/// Every file under `dir` holding `marker`: `*.entry` caches, saved plans, all.
pub(crate) fn files_holding(dir: &Path, marker: &str) -> Vec<String> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_holding(&path, marker));
        } else if std::fs::read(&path).is_ok_and(|bytes| {
            bytes
                .windows(marker.len())
                .any(|window| window == marker.as_bytes())
        }) {
            found.push(path.display().to_string());
        }
    }
    found
}

fn runtime_for(
    workspace: &Workspace,
    provider: &MockServer,
    github: &MockServer,
    allow: bool,
) -> ToolRuntime {
    let env = if allow {
        opted_in(github)
    } else {
        github_api(github).to_vec()
    };
    clasify_runtime(workspace, provider, &env)
}

/// The GitHub API with the opt-in that opens credential names to a plain read
/// (kept in memory only), which the no-copy-on-disk checks need: by default
/// the path policy refuses that read before any request.
fn opted_in(github: &MockServer) -> Vec<(&'static str, String)> {
    let mut env = github_api(github).to_vec();
    env.push(("OCTOCODE_CREDENTIAL_FILES", "allow".to_owned()));
    env
}

fn source(file: &str) -> Value {
    json!({"tool":"ghGetFileContent","query":{"owner":"octo","repo":"demo","path":file}})
}

fn evaluate(file: &str) -> Value {
    json!({"operation":"evaluate","source":source(file),"questions":{"a":noul("Is this set?")}})
}

fn assert_nothing_left(provider_sent: &[Value], workspace: &Workspace, what: &str) {
    assert!(
        provider_sent
            .iter()
            .all(|request| !request.to_string().contains(MARKER)),
        "{what}: the provider received the file: {provider_sent:?}"
    );
    assert_eq!(
        files_holding(&workspace.home, MARKER),
        Vec::<String>::new(),
        "{what}: a copy stayed on disk"
    );
}

/// A refused classified source names only a route the user's policy opens: a
/// file in a sensitive directory opens to no read; a credential name to a plain
/// ghGetFileContent read (redacted) only under the home opt-in, the one case
/// with that read as a lead. No empty lead container is left.
fn assert_refusal_routes(data: &Value, file: &str, allow: bool) {
    let text = data.to_string();
    let sensitive_directory = file.contains(".ssh/") || file.contains(".config/gh/");
    let hints = data["hints"].to_string();
    let read_lead = text.contains("\"tool\":\"ghGetFileContent\"");
    if sensitive_directory {
        assert!(hints.contains("No read returns it"), "{file}: {text}");
        assert!(!read_lead, "{file}: no read opens it: {text}");
    } else if allow {
        assert!(hints.contains("redacted"), "{file}: {text}");
        assert!(read_lead, "{file}: the opt-in read is offered: {text}");
    } else {
        assert!(
            hints.contains("security.credentialFiles"),
            "{file}: the refusal names the setting: {text}"
        );
        assert!(!read_lead, "{file}: the plain read is refused too: {text}");
    }
    for empty in ["{}", "[]"] {
        for key in ["next", "calls", "related"] {
            assert_ne!(
                data.get(key).map(Value::to_string).as_deref(),
                Some(empty),
                "{file}: an empty {key}: {text}"
            );
        }
    }
}

#[tokio::test]
async fn an_evaluated_github_source_is_withheld_by_name_before_any_fetch() {
    for allow in [false, true] {
        let (provider, github) = (provider().await, github().await);
        let workspace = Workspace::new();
        let runtime = runtime_for(&workspace, &provider, &github, allow);
        for &file in WITHHELD {
            let out = clasify(&runtime, vec![evaluate(file), evaluate("docs/guide.md")]).await;
            let refused = &out["results"][0];
            assert_eq!(refused["status"], "error", "{file}: {out}");
            assert_eq!(refused["data"]["errorCode"], "pathPolicyDenied", "{out}");
            assert!(
                refused["data"]["error"]
                    .as_str()
                    .is_some_and(|error| error.contains("withheld")),
                "the refusal says why: {out}"
            );
            assert_refusal_routes(&refused["data"], file, allow);
            assert!(
                out["results"][1]["data"].get("answers").is_some(),
                "{file}: the normal file in the same call is judged: {out}"
            );
        }
        let reads = contents_reads(&github).await;
        assert!(
            reads.iter().all(|read| read.contains("guide")),
            "allow={allow}: a withheld file was fetched: {reads:?}"
        );
        let sent = sent(&provider).await;
        assert!(sent.iter().any(|request| {
            request["state"]["text"]
                .as_str()
                .is_some_and(|text| text.contains(NORMAL))
        }));
        assert_nothing_left(&sent, &workspace, "evaluate");
        runtime.close().await;
    }
}

#[tokio::test]
async fn ranges_of_a_withheld_github_source_are_withheld_too() {
    let (provider, github) = (provider().await, github().await);
    let workspace = Workspace::new();
    let runtime = runtime_for(&workspace, &provider, &github, false);
    let out = clasify(
        &runtime,
        vec![json!({"operation":"evaluate",
            "source":{"tool":"ghGetFileContent","query":{"owner":"octo","repo":"demo",
                "path":".Env.Prod","ranges":[{"startLine":1,"endLine":1}]}},
            "questions":{"a":noul("Is this set?")}})],
    )
    .await;
    assert_eq!(
        out["results"][0]["data"]["errorCode"], "pathPolicyDenied",
        "{out}"
    );
    assert_eq!(contents_reads(&github).await, Vec::<String>::new());
    assert!(sent(&provider).await.is_empty());
    runtime.close().await;
}

#[tokio::test]
async fn a_withheld_github_resource_is_its_own_row_and_a_sibling_keeps_its_sections() {
    for allow in [false, true] {
        let (provider, github) = (provider().await, github().await);
        let workspace = Workspace::new();
        let runtime = runtime_for(&workspace, &provider, &github, allow);
        let out = clasify(
            &runtime,
            vec![json!({"operation":"sections",
                "resources":{
                    "plain":source(".env"),"cased":source(".Env.Prod"),"key":source("KEY.PEM"),
                    "nested":source("a/.ssh/config"),"guide":source("docs/guide.md"),
                },
                "questions":{"q":noul("Does this section say needle?")}})],
        )
        .await;
        let data = &out["results"][0]["data"];
        for key in ["plain", "cased", "key", "nested"] {
            let resource = &data["resources"][key];
            assert_eq!(resource["status"], "error", "{key}: {out:#}");
            assert_eq!(
                resource["error"]["errorCode"], "pathPolicyDenied",
                "{key}: {out:#}"
            );
        }
        assert_eq!(data["resources"]["guide"]["status"], "ready", "{out:#}");
        let reads = contents_reads(&github).await;
        assert!(reads.iter().all(|read| read.contains("guide")), "{reads:?}");
        let sent = sent(&provider).await;
        assert!(
            sent.iter()
                .any(|request| request.to_string().contains(NORMAL)),
            "the guide is judged: {sent:?}"
        );
        assert_nothing_left(&sent, &workspace, "sections");
        runtime.close().await;
    }
}

/// A read with `classification` answers as the local one does: the typed
/// refusal, while a normal row of the same call is judged. Without
/// `classification` the same read sends nothing to the provider: the path
/// policy refuses it by default, and the home opt-in returns the file.
#[tokio::test]
async fn a_classified_github_read_is_refused_by_name_and_a_plain_read_follows_the_policy() {
    for allow in [false, true] {
        let (provider, github) = (provider().await, github().await);
        let workspace = Workspace::new();
        let runtime = runtime_for(&workspace, &provider, &github, allow);
        let row = |file: &str| {
            json!({"owner":"octo","repo":"demo","path":file,
                "classification":{"predicates":["Does it name the needle?"]}})
        };
        for &file in WITHHELD {
            let out = runtime
                .run(
                    "classified-read".into(),
                    "ghGetFileContent".into(),
                    json!({"queries":[row(file), row("docs/guide.md")]}),
                )
                .await
                .expect("classified read")
                .structured_content;
            support::checks::assert_output_contract("ghGetFileContent", &out);
            let refused = &out["results"][0];
            assert_eq!(refused["status"], "error", "{file}: {out}");
            assert_eq!(refused["data"]["errorCode"], "pathPolicyDenied", "{out}");
            assert_refusal_routes(&refused["data"], file, allow);
            assert_ne!(out["results"][1]["status"], "error", "{file}: {out}");
            assert!(
                out["results"][1]["data"].get("selection").is_some(),
                "{file}: the normal row is judged: {out}"
            );
        }
        let reads = contents_reads(&github).await;
        assert!(reads.iter().all(|read| read.contains("guide")), "{reads:?}");
        let sent_before = sent(&provider).await;
        assert!(
            sent_before
                .iter()
                .any(|request| request.to_string().contains(NORMAL)),
            "{sent_before:?}"
        );
        assert_nothing_left(&sent_before, &workspace, "classified read");

        let plain = support::run(
            &runtime,
            "ghGetFileContent",
            json!({"owner":"octo","repo":"demo","path":".env"}),
        )
        .await;
        if allow {
            assert_eq!(plain.status(), "success", "{}", plain.data());
            assert!(
                plain.data()["content"]
                    .as_str()
                    .is_some_and(|content| content.contains(MARKER)),
                "the opt-in returns the file to its caller: {}",
                plain.data()
            );
        } else {
            assert_eq!(plain.status(), "error", "{}", plain.data());
            assert_eq!(
                plain.data()["errorCode"],
                "pathPolicyDenied",
                "the path policy refuses a plain read by default: {}",
                plain.data()
            );
        }
        assert_eq!(
            sent(&provider).await.len(),
            sent_before.len(),
            "a direct read sends nothing to the provider"
        );
        runtime.close().await;
    }
}

/// No other GitHub reader is a text resource or an image source, and no
/// tool that writes is a source at all: the contract refuses it. A search or
/// history tool may be an `evaluate` source (its rows are the units), and
/// then its own contract checks the query, which refuses this one. None
/// reaches the provider or GitHub.
#[tokio::test]
async fn no_other_github_tool_is_a_clasify_source() {
    let (provider, github) = (provider().await, github().await);
    let workspace = Workspace::new();
    let runtime = runtime_for(&workspace, &provider, &github, false);
    for tool in [
        "ghGetHistoryItem",
        "ghStructure",
        "ghSearchCode",
        "ghSearchHistory",
        "ghSearchRepo",
        "ghCloneRepo",
    ] {
        let source = json!({"tool":tool,"query":{"owner":"octo","repo":"demo","path":".env"}});
        let rows_tool = !matches!(tool, "ghStructure" | "ghCloneRepo");
        for (at, row) in [
            json!({"operation":"evaluate","source":source,"questions":{"a":noul("Is this set?")}}),
            json!({"operation":"sections","resources":{"r":source},"questions":{"a":noul("Is this set?")}}),
            json!({"operation":"evaluate","content":"image_a","assets":{"image_a":{"source":source}},
                "questions":{"a":noul("Does image_a show it?")}}),
        ]
        .into_iter()
        .enumerate()
        {
            let refused = runtime
                .run("source".into(), "clasify".into(), json!({"queries":[row]}))
                .await;
            if at == 0 && rows_tool {
                // Its own contract refuses the query at admission, or its
                // own search finds nothing: no row is judged either way.
                let judged = refused.as_ref().ok().map(|outcome| {
                    outcome.structured_content.clone().into_value()["results"][0]["data"]
                        ["errorCode"]
                        .is_null()
                });
                assert_ne!(judged, Some(true), "{tool}: {refused:?}");
            } else {
                assert!(refused.is_err(), "{tool}: {refused:?}");
            }
        }
    }
    assert!(sent(&provider).await.is_empty());
    // A search may run as its tool runs it; no file is read for any of them.
    let reads = github.received_requests().await.unwrap();
    assert!(
        reads
            .iter()
            .all(|request| !request.url.path().contains("/contents/")),
        "{reads:?}"
    );
    runtime.close().await;
}
