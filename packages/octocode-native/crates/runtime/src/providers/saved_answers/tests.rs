use super::*;

fn saved(body: &str) -> SavedAnswer {
    SavedAnswer {
        answer: ProviderAnswer {
            request: "GET x".into(),
            status: 200,
            headers: vec![("etag".into(), "\"e\"".into())],
            body: body.into(),
            next: None,
            not_found: None,
        },
        observed_ms: 1,
        redirected: false,
    }
}

fn exposing(request: &str, header: &str, next: Option<&str>, body: &str) -> ProviderAnswer {
    ProviderAnswer {
        request: request.into(),
        status: 200,
        headers: vec![("x-note".into(), header.into())],
        body: body.into(),
        next: next.map(str::to_owned),
        not_found: None,
    }
}

/// The memoized verdict is the fresh gate's, the first time and from the
/// memo, whichever field exposes the credential.
#[test]
fn a_memoized_verdict_equals_a_fresh_gate() {
    use base64::Engine as _;
    let security = ContentSecurity::new();
    let secret = format!("token = \"ghp_{}\"\n", "q7W8e9R0t1Y2".repeat(3));
    let encoded = base64::engine::general_purpose::STANDARD.encode(&secret);
    let contents = serde_json::json!({"encoding": "base64", "content": encoded}).to_string();
    let binary = serde_json::json!({"encoding": "base64", "content": "/w=="}).to_string();
    let cases = [
        (
            exposing("GET /clean", "plain", None, "fn main() {}\n"),
            true,
        ),
        (exposing("GET /body", "plain", None, &secret), false),
        (exposing("GET /blob", "plain", None, &contents), false),
        (exposing("GET /binary", "plain", None, &binary), false),
        (
            exposing("GET /header", &secret, None, "fn main() {}\n"),
            false,
        ),
        (
            exposing("GET /next", "plain", Some(&secret), "fn main() {}\n"),
            false,
        ),
        (exposing(&secret, "plain", None, "fn main() {}\n"), false),
    ];
    for (answer, expected) in &cases {
        assert_eq!(gate_verdict(&security, answer), *expected, "{answer:?}");
        assert_eq!(gate_keeps(&security, answer), *expected, "{answer:?}");
        assert_eq!(gate_keeps(&security, answer), *expected, "{answer:?}");
    }
}

/// A repeat answers from the memo instead of running the gate, and nothing
/// but the exact fields the gate judges reaches that verdict.
#[test]
fn a_repeat_is_answered_from_the_memo_by_exactly_its_fields() {
    let security = ContentSecurity::new();
    let answer = exposing("GET /memo-served", "plain", None, "a repeat body\n");
    let key = verdict_key(&answer).expect("key");
    // A verdict the gate would never give shows who answered.
    VERDICTS.put_confirmed(key.clone(), false, 1, UNUSED_RETENTION);
    assert!(!gate_keeps(&security, &answer));
    assert!(gate_verdict(&security, &answer));
    VERDICTS.remove(&key);
    for other in [
        exposing("GET /memo-served2", "plain", None, "a repeat body\n"),
        exposing("GET /memo-served", "plain2", None, "a repeat body\n"),
        exposing("GET /memo-served", "plain", Some(""), "a repeat body\n"),
        exposing("GET /memo-served", "plain", None, "a repeat body\n\n"),
        // A byte moved across a field boundary is another answer.
        exposing("GET /memo-serve", "dplain", None, "a repeat body\n"),
    ] {
        assert_ne!(verdict_key(&other), Some(key.clone()), "{other:?}");
    }
}

/// A credential only a path-gated pattern sees, in the raw bytes GitHub
/// answers a contents request with or in a payload naming its file: the
/// read redacts it, so it must not be stored. The memoized verdict agrees.
#[test]
fn a_path_gated_credential_is_not_stored() {
    let manifest =
        "apiVersion: v1\nkind: Secret\ndata:\n  tls.crt: QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo=\n";
    let token = format!("github_pat_{}", "A1b2C3d4E5".repeat(8) + "zz");
    let jdbc = "url = jdbc:mysql://admin:hunter2hunter2@db.internal/app\n";
    let raw = |path: &str, body: &str| {
        exposing(
            &format!("GET https://x/contents/{path}?ref=main"),
            "plain",
            None,
            body,
        )
    };
    let answers = [
        raw("k8s/secret.yaml", manifest),
        // The same bytes under any name: the name is not always in the request.
        raw("blob", manifest),
        exposing("GET /repos/o/r/git/blobs/abc", "plain", None, &serde_json::json!({
            "encoding": "base64",
            "content": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, manifest),
        }).to_string()),
        raw("config/app.env", &format!("TOKEN={token}\n")),
        raw("notes.txt", &format!("TOKEN={token}\n")),
        raw("db.properties", jdbc),
        // A key named only by its file, with no marker in it, spelled as a
        // request encodes a path.
        raw("deploy%2Fid_rsa", "c3BsaXRLZXlCb2R5VHdvQkFCQUJBQkFCQUJBQkFC\n"),
    ];
    let security = ContentSecurity::new();
    for answer in &answers {
        assert!(!gate_verdict(&security, answer), "{answer:?}");
        assert!(!gate_keeps(&security, answer), "{answer:?}");
        assert!(!gate_keeps(&security, answer), "{answer:?}");
    }
    let dir = tempfile::tempdir().expect("dir");
    let store = KeptAnswers::new(Some(dir.path().to_owned()));
    for (index, answer) in answers.iter().enumerate() {
        let kept = SavedAnswer {
            answer: answer.clone(),
            observed_ms: 1,
            redirected: false,
        };
        store.save("scope", &index.to_string(), kept);
        assert!(store.load("scope", &index.to_string()).is_some());
    }
    let restarted = KeptAnswers::new(Some(dir.path().to_owned()));
    for index in 0..answers.len() {
        assert!(
            restarted.load("scope", &index.to_string()).is_none(),
            "{index}"
        );
    }
}

/// What only looks like a credential once its context is ignored stays
/// stored: a commit SHA is not a `wandb` key, wherever it is mentioned, and
/// the path in the request still names a key for one that is.
#[test]
fn a_bare_shape_needs_its_context_to_keep_an_answer_off_disk() {
    let security = ContentSecurity::new();
    let sha = "9b5e0d8c2f7a41e6b3c58d9f0a1e2b7c4d6f8a03";
    let uuid = "3f2b8c1e-9a7d-4e6f-8b5a-1c2d3e4f5a6b";
    let metadata = serde_json::json!({"sha": sha, "id": uuid, "path": "src/lib.rs"}).to_string();
    assert!(gate_verdict(
        &security,
        &exposing(
            "GET https://x/repos/o/r/commits/main",
            "plain",
            None,
            &metadata
        )
    ));
    assert!(gate_verdict(
        &security,
        &exposing("GET https://x/contents/src/a.txt", "plain", None, sha)
    ));
    assert!(!gate_verdict(
        &security,
        &exposing("GET https://x/contents/wandb/settings", "plain", None, sha)
    ));
}

/// A clean answer reaches disk and a second process; one the gate would
/// rewrite stays in this process, and a later clean answer replaces it.
#[test]
fn only_an_answer_the_gate_leaves_unchanged_reaches_the_disk() {
    let dir = tempfile::tempdir().expect("dir");
    let secret = format!("token = \"ghp_{}\"\n", "a1B2c3D4e5F6".repeat(3));
    let store = KeptAnswers::new(Some(dir.path().to_owned()));
    store.save("scope", "clean", saved("fn main() {}\n"));
    store.save("scope", "secret", saved(&secret));
    assert_eq!(
        store.load("scope", "secret").expect("held").answer.body,
        secret
    );
    let restarted = KeptAnswers::new(Some(dir.path().to_owned()));
    assert!(restarted.load("scope", "clean").is_some());
    assert!(restarted.load("scope", "secret").is_none());
    for entry in std::fs::read_dir(dir.path()).expect("entries").flatten() {
        let bytes = std::fs::read(entry.path()).expect("entry");
        assert!(!String::from_utf8_lossy(&bytes).contains("ghp_"));
    }
    store.save("scope", "secret", saved("clean now\n"));
    assert_eq!(
        store.load("scope", "secret").expect("sealed").answer.body,
        "clean now\n",
        "the held answer does not outlive its replacement"
    );
}

#[test]
fn scopes_separate_answers_and_forget_reaches_both_stores() {
    let dir = tempfile::tempdir().expect("dir");
    let store = KeptAnswers::new(Some(dir.path().to_owned()));
    let secret = format!("token = \"ghp_{}\"\n", "a1B2c3D4e5F6".repeat(3));
    store.save("one", "clean", saved("a\n"));
    store.save("one", "secret", saved(&secret));
    assert!(store.load("two", "clean").is_none());
    store.forget("one", "clean");
    store.forget("one", "secret");
    assert!(store.load("one", "clean").is_none());
    assert!(store.load("one", "secret").is_none());
}

/// With `storage.mode` memory, answers end with the store.
#[test]
fn a_memory_stores_answers_end_with_it() {
    let store = KeptAnswers::new(None);
    store.save("scope", "clean", saved("a\n"));
    assert!(store.load("scope", "clean").is_some());
    assert!(KeptAnswers::new(None).load("scope", "clean").is_none());
}

#[test]
fn the_oldest_reuse_is_the_one_disclosed() {
    let reused = ReusedSince::default();
    assert_eq!(reused.get(), None);
    reused.record(30);
    reused.record(10);
    reused.record(20);
    assert_eq!(reused.get(), Some(10));
    let mut row = json!({"artifacts": []});
    disclose_reuse(&mut row, 86_400_000);
    assert_eq!(
        row["hints"][0],
        "Answer observed 1970-01-02T00:00:00Z: its origin was not asked again, so later changes are not shown."
    );
}
