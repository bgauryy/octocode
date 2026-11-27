//! `evaluate` with `resources` end to end: each resource
//! judged whole with every question, one provider request per resource, a
//! failure kept with its resource, and a glob expanded to one resource per
//! file whose keys never take another key.

use super::sources::window_provider;
use super::{clasify, noul, provider, sent};
use crate::support::{Workspace, clasify_runtime};
use serde_json::{Value, json};

fn local(path: &str) -> Value {
    json!({"tool":"localFetch","query":{"path":path}})
}

/// A resources row bounded past every glob here: these tests check what its
/// resources are, not what bounds them ([`super::budget`]).
fn evaluate(resources: Value) -> Value {
    json!({"operation":"evaluate","resources":resources,"maxRequests":super::budget::EVERY_PAGE,
        "questions":{"set":noul("Is a value set?")}})
}

/// The row's `resources` map, its keys in order.
fn resources(out: &Value) -> (&serde_json::Map<String, Value>, Vec<&str>) {
    let data = &out["results"][0]["data"];
    let map = data["resources"]
        .as_object()
        .unwrap_or_else(|| panic!("a resources row: {out:#}"));
    (map, map.keys().map(String::as_str).collect())
}

/// Three files, one missing: two answers, one `notFound`, and exactly
/// one provider request per file read; the row validates as served.
#[tokio::test]
async fn each_resource_is_judged_once_and_a_missing_one_keeps_its_error() {
    let server = provider(2).await;
    let workspace = Workspace::new();
    workspace.write("a.toml", "a = 1\n");
    workspace.write("b.toml", "b = 2\n");
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let out = clasify(
        &runtime,
        vec![evaluate(json!({
            "a":local("a.toml"),"gone":local("gone.toml"),"b":local("b.toml")
        }))],
    )
    .await;
    assert!(out["results"][0].get("status").is_none(), "{out:#}");
    let (map, keys) = resources(&out);
    assert_eq!(keys, ["a", "gone", "b"], "{out:#}");
    for key in ["a", "b"] {
        assert_eq!(map[key]["answers"]["set"]["noul"], 0.9, "{out:#}");
        assert_eq!(map[key]["usage"]["calls"], 1, "{out:#}");
    }
    assert_eq!(map["gone"]["errorCode"], "notFound", "{out:#}");
    assert!(map["gone"].get("answers").is_none(), "{out:#}");
    assert_eq!(out["results"][0]["data"]["model"], "jev-1.13.0", "{out:#}");
    assert_eq!(sent(&server).await.len(), 2, "one request per file read");
    runtime.close().await;
}

/// A glob is one resource per matching file, `<key>/<path>`, in walk
/// order, each judged by its own request; a caller's own key is never taken
/// by an expanded one (`d/a.md` beside the glob `d` keeps its file, and the
/// glob's `a.md` takes `d/a.md-2`).
#[tokio::test]
async fn a_glob_is_one_judged_resource_per_file_and_never_takes_a_callers_key() {
    let server = provider(3).await;
    let workspace = Workspace::new();
    workspace.write("docs/a.md", "short\n");
    workspace.write("docs/b.md", "also short\n");
    workspace.write("docs/c.txt", "not matched\n");
    workspace.write("other/z.rs", "fn longer_than_the_others() {}\n");
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let out = clasify(
        &runtime,
        vec![evaluate(json!({
            "d":local("docs/*.md"),"d/a.md":local("other/z.rs")
        }))],
    )
    .await;
    let (map, keys) = resources(&out);
    assert_eq!(keys, ["d/a.md-2", "d/b.md", "d/a.md"], "{out:#}");
    let chars = |key: &str| map[key]["readChars"].as_u64().unwrap_or_default();
    assert_eq!(
        chars("d/a.md"),
        "fn longer_than_the_others() {}\n".len() as u64,
        "{out:#}"
    );
    assert_eq!(chars("d/a.md-2"), "short\n".len() as u64, "{out:#}");
    assert_eq!(sent(&server).await.len(), 3, "one request per file");
    runtime.close().await;
}

/// Source-scope disclosures on the row's data.
fn warnings(out: &Value) -> Vec<&str> {
    out["results"][0]["data"]["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

/// A glob on `sections` keys each file `<key>/<path>` and the row validates
/// as served; on both operations the warning names what the walk left out.
#[tokio::test]
async fn a_glob_keys_each_file_on_sections_and_both_operations_disclose_its_skips() {
    let server = provider(2).await;
    let workspace = Workspace::new();
    workspace.write("docs/a.md", "visible\n");
    workspace.write("docs/.h.md", "hidden\n");
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let glob = json!({"d":local("docs/*.md")});
    let sections = clasify(
        &runtime,
        vec![json!({"operation":"sections","maxRequests":crate::runtime_clasify::budget::EVERY_PAGE,"resources":glob,
            "questions":{"set":noul("Is a value set?")}})],
    )
    .await;
    let (map, keys) = resources(&sections);
    assert_eq!(keys, ["d/a.md"], "{sections:#}");
    assert_eq!(map["d/a.md"]["status"], "ready", "{sections:#}");
    let evaluated = clasify(&runtime, vec![evaluate(glob.clone())]).await;
    assert_eq!(resources(&evaluated).1, ["d/a.md"], "{evaluated:#}");
    for out in [&sections, &evaluated] {
        assert!(
            warnings(out)
                .iter()
                .any(|hint| hint.contains("`docs/.h.md` (hidden)")),
            "{out:#}"
        );
    }
    runtime.close().await;
}

/// A row none of whose resources was read fails as they did: the row is an
/// error with their one code, and nothing reached the provider.
#[tokio::test]
async fn a_row_none_of_whose_resources_was_read_fails_like_them() {
    let server = provider(0).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let out = clasify(
        &runtime,
        vec![evaluate(
            json!({"a":local("gone-a.md"),"b":local("gone-b.md")}),
        )],
    )
    .await;
    assert_eq!(out["results"][0]["status"], "error", "{out:#}");
    assert_eq!(
        out["results"][0]["data"]["errorCode"], "notFound",
        "{out:#}"
    );
    assert_eq!(resources(&out).1, ["a", "b"], "{out:#}");
    runtime.close().await;
}

/// A glob over credential stores sends none of the
/// known stores (a Docker client config under `.docker/`, a kubeconfig under
/// `.kube/`) and, of a file outside the list (a GCP service-account key under
/// any name), only its known secrets are hidden: no private key is in any
/// request, and what says where and who still is.
#[tokio::test]
async fn credential_files_a_glob_sends_carry_no_secret_to_the_provider() {
    let server = provider(1).await;
    let workspace = Workspace::new();
    // Each store's secret, with what it is: a failure names that, never the value.
    let planted = [
        ("the Docker auth", "cm9ib3Q6czNjcjN0LXBhc3N3b3JkLTk4NzY1"),
        ("the kubeconfig token", "Zq7xK2mPlanted9OpaqueTok"),
        (
            "the service-account private key",
            "MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7Zq7xK2mPlanted9",
        ),
    ];
    workspace.write(
        "creds/.docker/config.json",
        format!(
            "{{\n  \"auths\": {{\n    \"registry.example.com\": {{ \"auth\": \"{}\" }}\n  }}\n}}\n",
            planted[0].1
        ),
    );
    workspace.write(
        "creds/.kube/config",
        format!(
            "apiVersion: v1\nkind: Config\nusers:\n- name: static\n  user:\n    token: {}\n",
            planted[1].1
        ),
    );
    workspace.write(
        "creds/gcp/my-proj-1a2b3c4d5e6f.json",
        format!(
            "{{\n  \"type\": \"service_account\",\n  \"project_id\": \"my-proj\",\n  \"private_key\": \"-----BEGIN PRIVATE KEY-----\\n{}\\n-----END PRIVATE KEY-----\\n\",\n  \"token_uri\": \"https://oauth2.googleapis.com/token\"\n}}\n",
            planted[2].1
        ),
    );
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let glob = workspace
        .workspace
        .join("creds/**/*")
        .to_string_lossy()
        .into_owned();
    let out = clasify(&runtime, vec![evaluate(json!({"c": local(&glob)}))]).await;
    let bodies = sent(&server).await;
    let wire = serde_json::to_string(&bodies).unwrap_or_default();
    assert_eq!(bodies.len(), 1, "only the file outside the list: {out:#}");
    for (what, secret) in planted {
        assert!(!wire.contains(secret), "{what} reached the provider");
    }
    for kept in ["my-proj", "https://oauth2.googleapis.com/token"] {
        assert!(wire.contains(kept), "{kept} did not reach the provider");
    }
    runtime.close().await;
}

/// A resource past the provider's windows is answered from its sections, as
/// a `source` is; a sibling keeps its own answer and captured source read.
#[tokio::test]
async fn a_resource_too_large_for_the_window_is_answered_from_its_sections() {
    let server = window_provider(100_000, 100_000).await;
    let workspace = Workspace::new();
    workspace.write(
        "big.txt",
        "x".repeat(80) + &"\nfiller line of text".repeat(14_000),
    );
    workspace.write("ok.txt", "a = 1\n");
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let out = clasify(
        &runtime,
        vec![evaluate(
            json!({"big":local("big.txt"),"ok":local("ok.txt")}),
        )],
    )
    .await;
    let (map, _) = resources(&out);
    assert!(map["big"].get("errorCode").is_none(), "{out:#}");
    assert_eq!(map["big"]["answers"]["set"]["noul"], 0.9, "{out:#}");
    let sections = map["big"]["coverage"]["totalSections"].as_u64().unwrap();
    assert!(sections > 1, "{out:#}");
    assert_eq!(map["big"]["usage"]["calls"], json!(sections + 1), "{out:#}");
    assert_eq!(map["ok"]["answers"]["set"]["noul"], 0.9, "{out:#}");
    for key in ["big", "ok"] {
        assert_eq!(map[key]["next"]["read"]["tool"], "localFetch", "{out:#}");
        crate::support::assert_cursor_only(&map[key]["next"]["read"]["input"]);
    }
    assert!(
        out["results"][0]["data"].get("related").is_none(),
        "{out:#}"
    );
    runtime.close().await;
}
