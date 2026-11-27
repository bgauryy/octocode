use super::*;
use octocode_test_support::json_fixture;

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

/// Whether nothing in `answer` would be redacted, judged afresh: what the
/// memoized gate must agree with.
fn gate_verdict(security: &ContentSecurity, answer: &ProviderAnswer) -> bool {
    body_verdict(security, answer) && envelope_verdict(security, answer)
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
    let binary = json_fixture(r#"{"encoding": "base64", "content": "/w=="}"#).to_string();
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
/// but the request and body the memo judged reaches that verdict.
#[test]
fn a_repeat_is_answered_from_the_memo_by_exactly_its_request_and_body() {
    let security = ContentSecurity::new();
    let answer = exposing("GET /memo-served", "plain", None, "a repeat body\n");
    let key = verdict_key(&answer);
    // A verdict the gate would never give shows who answered.
    VERDICTS.put_confirmed(key.clone(), false, 1, UNUSED_RETENTION);
    let repeated = gate_keeps(&security, &answer);
    VERDICTS.remove(&key);
    assert!(!repeated);
    assert!(gate_verdict(&security, &answer));
    for other in [
        exposing("GET /memo-served2", "plain", None, "a repeat body\n"),
        exposing("GET /memo-served", "plain", None, "a repeat body\n\n"),
        // A byte moved across the request's end is another answer.
        exposing("GET /memo-serve", "plain", None, "da repeat body\n"),
    ] {
        assert_ne!(verdict_key(&other), key, "{other:?}");
    }
}

/// A fresh fetch of a request that answers the same body (a search asked
/// again, a saved answer GitHub confirmed) differs only in its headers and
/// next link: its body's verdict comes from the memo, judged once in this
/// process.
#[test]
fn a_refetched_body_is_judged_from_the_memo_whatever_its_headers() {
    let security = ContentSecurity::new();
    let first = exposing(
        "GET /memo-refetched",
        "date one",
        None,
        "a refetched body\n",
    );
    let again = exposing(
        "GET /memo-refetched",
        "date two",
        Some("https://x/memo-refetched?page=2"),
        "a refetched body\n",
    );
    let key = verdict_key(&first);
    // A verdict the gate would never give shows who answered.
    VERDICTS.put_confirmed(key.clone(), false, 1, UNUSED_RETENTION);
    let refetched = gate_keeps(&security, &again);
    VERDICTS.remove(&key);
    assert!(!refetched, "the refetch is judged from the memo");
    assert!(gate_verdict(&security, &again));
}

/// The headers and next link of each fetch are judged even when its body's
/// verdict is known: a credential there keeps the answer off disk.
#[test]
fn a_credential_beside_a_memoized_body_is_judged_on_each_fetch() {
    let security = ContentSecurity::new();
    let secret = format!("token = \"ghp_{}\"\n", "z9Y8x7W6v5U4".repeat(3));
    let body = "a memoized body\n";
    assert!(gate_keeps(
        &security,
        &exposing("GET /memo-header", "plain", None, body)
    ));
    assert!(!gate_keeps(
        &security,
        &exposing("GET /memo-header", &secret, None, body)
    ));
    assert!(!gate_keeps(
        &security,
        &exposing("GET /memo-header", "plain", Some(&secret), body)
    ));
}

/// A public API answer is no config file: each leaf is judged by the path it
/// is about, so an advisory's prose with a bare word after a label leaves the
/// answer on disk, while a quoted value, which a read of any file hides,
/// keeps it in memory. An object of the body that names a config file's path
/// is judged as that file.
#[test]
fn a_public_api_answer_is_judged_by_the_path_each_leaf_is_about() {
    let security = ContentSecurity::new();
    let advisories = |description: &str| {
        exposing(
            "GET https://api.github.com/repos/o/r/security-advisories?per_page=100",
            "plain",
            None,
            &serde_json::json!([{"ghsa_id": "GHSA-abcd-efgh-ijkl", "description": description}])
                .to_string(),
        )
    };
    assert!(gate_keeps(
        &security,
        &advisories("Set password: letmeinNow in the service config.")
    ));
    assert!(!gate_keeps(
        &security,
        &advisories("asset=AdminAsset{name='foo', secret='LEAKED'}")
    ));
    let search = |path: &str| {
        exposing(
            "GET https://api.github.com/search/code?q=letmein",
            "plain",
            None,
            &serde_json::json!({"items": [{"path": path,
                "text_matches": [{"fragment": "password: letmeinNow"}]}]})
            .to_string(),
        )
    };
    assert!(!gate_keeps(&security, &search("deploy/app.yml")));
    assert!(gate_keeps(&security, &search("src/app.rs")));
}

/// A raw file that happens to be JSON is one text to a read: a known secret
/// in it keeps the answer off disk. An API payload's values are judged as
/// values, so `keys_url` beside a URL keeps it on disk.
#[test]
fn a_raw_json_file_is_judged_as_the_text_a_read_shows() {
    let security = ContentSecurity::new();
    let raw = |path: &str, body: &str| {
        exposing(
            &format!("GET https://x/repos/o/r/contents/{path}?ref=main"),
            "plain",
            None,
            body,
        )
    };
    for answer in [
        raw(
            "keys%2Fsigning.json",
            r#"{"kty":"EC","token":"ghp_Public0Coordinate9Stays8Readable0123"}"#,
        ),
        raw(
            "wg.json",
            r#"{"aws_secret_access_key":"wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"}"#,
        ),
    ] {
        assert!(!gate_verdict(&security, &answer), "{answer:?}");
    }
    assert!(gate_verdict(
        &security,
        &raw("package.json", r#"{"name":"x","version":"1.0.0"}"#)
    ));
    assert!(gate_verdict(
        &security,
        &exposing(
            "GET https://x/repos/o/r",
            "plain",
            None,
            r#"{"keys_url":"https://api.github.com/repos/o/r/keys{/key_id}"}"#,
        )
    ));
}

/// A credential in the raw bytes GitHub answers a contents request with, or in
/// a payload naming its file: the read redacts it, so it must not be stored.
/// The memoized verdict agrees.
#[test]
fn a_path_gated_credential_is_not_stored() {
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
        raw("config/app.env", &format!("TOKEN={token}\n")),
        raw("notes.txt", &format!("TOKEN={token}\n")),
        raw("db.properties", jdbc),
        // A blob names no file: its request says which one it was read for.
        exposing(
            "GET https://x/git/blobs/abc\nraw\n\ndeploy/.env",
            "plain",
            None,
            "QUJDREVGR0hJSktM\n",
        ),
        // A key named only by its file, with no marker in it, spelled as a
        // request encodes a path.
        raw(
            "deploy%2Fid_rsa",
            "c3BsaXRLZXlCb2R5VHdvQkFCQUJBQkFCQUJBQkFC\n",
        ),
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

/// Content-addressed bodies can belong to several requests and credentials:
/// retiring one answer must not make the others request their body again.
#[test]
fn forgetting_an_answer_keeps_other_answers_with_the_same_body() {
    for persistent in [false, true] {
        let dir = tempfile::tempdir().expect("dir");
        let store = KeptAnswers::new(persistent.then(|| dir.path().to_owned()));
        let answer = saved("fn shared() {}\n");
        store.save("one", "first", answer.clone());
        store.save("one", "second", answer.clone());
        store.save("two", "first", answer.clone());

        store.forget("one", "first");

        assert!(store.load("one", "first").is_none());
        assert_eq!(store.load("one", "second"), Some(answer.clone()));
        assert_eq!(store.load("two", "first"), Some(answer.clone()));
        if persistent {
            let reopened = KeptAnswers::new(Some(dir.path().to_owned()));
            assert_eq!(reopened.load("one", "second"), Some(answer.clone()));
            assert_eq!(reopened.load("two", "first"), Some(answer));
        }
    }
}

/// A shared body stays while any answer names it, for this process and the
/// next, and leaves the directory with the last one, by forget or replacement.
#[test]
fn a_shared_body_leaves_with_the_last_answer_naming_it() {
    let dir = tempfile::tempdir().expect("dir");
    let body = "fn last_owner() {}\n";
    let holding = || {
        std::fs::read_dir(dir.path())
            .expect("entries")
            .flatten()
            .filter(|entry| {
                std::fs::read(entry.path())
                    .is_ok_and(|bytes| String::from_utf8_lossy(&bytes).contains(body))
            })
            .count()
    };
    let store = KeptAnswers::new(Some(dir.path().to_owned()));
    let owners = [("one", "first"), ("one", "second"), ("two", "first")];
    for (scope, request) in owners {
        store.save(scope, request, saved(body));
    }
    assert_eq!(holding(), 1);
    let (last, rest) = owners.split_last().expect("owners");
    for (scope, request) in rest {
        KeptAnswers::new(Some(dir.path().to_owned())).forget(scope, request);
        assert_eq!(holding(), 1, "{scope} {request}");
    }
    assert_eq!(store.load(last.0, last.1), Some(saved(body)));
    store.save(last.0, last.1, saved("fn replacement() {}\n"));
    assert_eq!(holding(), 0);
}

/// Replacing a response retires its reference, not a body another response owns.
#[test]
fn replacing_an_answer_keeps_other_answers_with_the_previous_body() {
    for persistent in [false, true] {
        let dir = tempfile::tempdir().expect("dir");
        let store = KeptAnswers::new(persistent.then(|| dir.path().to_owned()));
        let previous = saved("fn previous() {}\n");
        let replacement = saved("fn replacement() {}\n");
        store.save("one", "first", previous.clone());
        store.save("two", "second", previous.clone());

        store.save("one", "first", replacement.clone());

        assert_eq!(store.load("one", "first"), Some(replacement));
        assert_eq!(store.load("two", "second"), Some(previous.clone()));
        if persistent {
            let reopened = KeptAnswers::new(Some(dir.path().to_owned()));
            assert_eq!(reopened.load("two", "second"), Some(previous));
        }
    }
}

/// Moving one response into memory does not revoke another response's clean
/// body, and a new process still cannot reach the sensitive replacement.
#[test]
fn a_sensitive_replacement_keeps_other_answers_with_the_previous_body() {
    let dir = tempfile::tempdir().expect("dir");
    let store = KeptAnswers::new(Some(dir.path().to_owned()));
    let previous = saved("fn shared_clean() {}\n");
    let sensitive = saved(&format!("GITHUB_TOKEN=ghp_{}\n", "a".repeat(36)));
    assert!(!gate_keeps(&store.security, &sensitive.answer));
    store.save("one", "first", previous.clone());
    store.save("two", "second", previous.clone());

    store.save("one", "first", sensitive.clone());

    assert_eq!(store.load("one", "first"), Some(sensitive));
    assert_eq!(store.load("two", "second"), Some(previous.clone()));
    let reopened = KeptAnswers::new(Some(dir.path().to_owned()));
    assert!(reopened.load("one", "first").is_none());
    assert_eq!(reopened.load("two", "second"), Some(previous));
}

/// Give each publication step its own directory and budget so a refusal can
/// be exercised without another store's eviction hiding a stale reference.
fn budgeted_disk_store(
    root: &std::path::Path,
    bodies: CacheConfig,
    sealed: CacheConfig,
) -> KeptAnswers {
    KeptAnswers {
        references: Store::new(bodies.clone(), Some(root.join("bodies"))),
        bodies: Store::new(bodies, Some(root.join("bodies"))),
        sealed: Store::new(sealed, Some(root.join("sealed"))),
        ..KeptAnswers::new(None)
    }
}

#[test]
fn a_refused_replacement_body_retires_only_its_previous_answer() {
    let dir = tempfile::tempdir().expect("dir");
    let body_budget = CacheConfig { disk_bytes: 1024 };
    let store = budgeted_disk_store(dir.path(), body_budget.clone(), CacheConfig::derived());
    let previous = saved("fn shared_clean() {}\n");
    store.save("one", "first", previous.clone());
    store.save("two", "second", previous.clone());
    assert_eq!(store.load("one", "first"), Some(previous.clone()));
    assert_eq!(store.load("two", "second"), Some(previous.clone()));

    let replacement = saved(&"new evidence\n".repeat(body_budget.disk_bytes as usize));
    assert!(replacement.answer.body.len() as u64 > body_budget.disk_bytes);
    assert!(gate_keeps(&store.security, &replacement.answer));
    store.save("one", "first", replacement);

    assert!(store.load("one", "first").is_none());
    assert_eq!(store.load("two", "second"), Some(previous.clone()));
    let reopened = budgeted_disk_store(dir.path(), body_budget, CacheConfig::derived());
    assert!(reopened.load("one", "first").is_none());
    assert_eq!(reopened.load("two", "second"), Some(previous));
}

#[test]
fn a_refused_replacement_metadata_retires_only_its_previous_answer() {
    let dir = tempfile::tempdir().expect("dir");
    let metadata_budget = CacheConfig { disk_bytes: 1024 };
    let store = budgeted_disk_store(dir.path(), CacheConfig::derived(), metadata_budget.clone());
    let previous = saved("fn shared_clean() {}\n");
    store.save("one", "first", previous.clone());
    store.save("two", "second", previous.clone());
    assert_eq!(store.load("one", "first"), Some(previous.clone()));
    assert_eq!(store.load("two", "second"), Some(previous.clone()));

    let mut replacement = saved("fn updated() {}\n");
    replacement.answer.headers.push((
        "x-note".into(),
        "updated metadata ".repeat(metadata_budget.disk_bytes as usize),
    ));
    assert!(gate_keeps(&store.security, &replacement.answer));
    store.save("one", "first", replacement);

    assert!(store.load("one", "first").is_none());
    assert_eq!(store.load("two", "second"), Some(previous.clone()));
    let reopened = budgeted_disk_store(dir.path(), CacheConfig::derived(), metadata_budget);
    assert!(reopened.load("one", "first").is_none());
    assert_eq!(reopened.load("two", "second"), Some(previous));
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
    let mut row = json_fixture(r#"{"artifacts": []}"#);
    disclose_reuse(&mut row, 86_400_000);
    assert_eq!(
        row,
        json_fixture(r#"{"artifacts": [], "observedAt": "1970-01-02T00:00:00Z"}"#),
        "the moment is a typed field, not hint text"
    );
}

mod credential_files;

/// A symlink answer names its target: a link to a credential file is held in
/// memory under the target's name, whatever the link is called, while a link
/// to an ordinary file is kept.
#[test]
fn a_symlink_to_a_credential_file_is_not_stored() {
    let link = |name: &str, target: &str| {
        exposing(
            &format!("GET https://x/contents/{name}?ref=main"),
            "plain",
            None,
            &serde_json::json!({"type":"symlink","target":target,"path":name}).to_string(),
        )
    };
    let security = ContentSecurity::new();
    for target in [".env", "../.ssh/id_rsa", "deploy/.pgpass"] {
        let answer = link("env", target);
        assert!(!gate_verdict(&security, &answer), "{target}");
        assert!(!gate_keeps(&security, &answer), "{target}");
    }
    assert!(gate_verdict(
        &security,
        &link("RelNotes", "Documentation/RelNotes.txt")
    ));
}

/// A blob's URL names only its SHA: its text is judged as the file it was
/// read for. A bare value after a known key is a value in a config file, so a
/// blob of `conf.yml` holding one never reaches the disk; in code it is an
/// identifier and the blob is kept.
#[test]
fn a_blob_is_judged_by_the_file_it_was_read_for() {
    let blob = |file: &str| {
        exposing(
            &format!(
                "GET https://x/repos/o/r/git/blobs/{}\nraw\n\n{file}",
                "b".repeat(40)
            ),
            "plain",
            None,
            "password: bareConfigValue7\n",
        )
    };
    let security = ContentSecurity::new();
    assert!(!gate_verdict(&security, &blob("conf.yml")));
    assert!(!gate_keeps(&security, &blob("deploy/conf.yml")));
    assert!(gate_verdict(&security, &blob("src/main.rs")));
    let dir = tempfile::tempdir().expect("dir");
    let store = KeptAnswers::new(Some(dir.path().to_owned()));
    store.save(
        "scope",
        "conf",
        SavedAnswer {
            answer: blob("conf.yml"),
            observed_ms: 1,
            redirected: false,
        },
    );
    let restarted = KeptAnswers::new(Some(dir.path().to_owned()));
    assert!(restarted.load("scope", "conf").is_none());
    for entry in walkdir_files(dir.path()) {
        let bytes = std::fs::read(&entry).expect("entry");
        assert!(
            !String::from_utf8_lossy(&bytes).contains("bareConfigValue7"),
            "{}",
            entry.display()
        );
    }
}

/// Every file under `dir`.
fn walkdir_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}

/// A listing entry's `download_url` carries a `?token=` value that no code
/// pattern names: the gate judges every leaf as any read could, so the
/// listing is not stored.
#[test]
fn a_listing_url_with_a_token_query_is_not_stored() {
    let listing = |url: &str| {
        exposing(
            "GET https://x/contents/docs?ref=main",
            "plain",
            None,
            &serde_json::json!([{"name":"README.md","path":"docs/README.md","type":"file",
                "download_url": url}])
            .to_string(),
        )
    };
    let security = ContentSecurity::new();
    let held = listing(
        "https://raw.example.com/o/r/main/docs/README.md?token=ABTH2BQWERTYUIOPASDFGHJKLZX",
    );
    assert!(!gate_verdict(&security, &held));
    assert!(!gate_keeps(&security, &held));
    assert!(gate_verdict(
        &security,
        &listing("https://raw.example.com/o/r/main/docs/README.md")
    ));
}

/// A credential the origin refused (a 401) keeps no saved answer, pinned ones
/// included, in the memory store or after a restart from disk; another
/// credential's answers stay, and answers saved after the refusal are served.
#[test]
fn a_revoked_credential_keeps_no_saved_answer() {
    for disk in [false, true] {
        let dir = tempfile::tempdir().expect("dir");
        let open = || KeptAnswers::new(disk.then(|| dir.path().to_owned()));
        let store = open();
        store.save("revoked", "r1", saved("{\"a\":1}"));
        store.save("revoked", "r2", saved("{\"a\":2}"));
        store.save("other", "r1", saved("{\"a\":1}"));
        assert!(store.load("revoked", "r1").is_some());
        store.revoked("revoked");
        let restarted = disk.then(open);
        for store in std::iter::once(&store).chain(restarted.as_ref()) {
            assert!(store.load("revoked", "r1").is_none(), "disk {disk}");
            assert!(store.load("revoked", "r2").is_none(), "disk {disk}");
            assert!(store.load("other", "r1").is_some(), "disk {disk}");
        }
        store.save("revoked", "r1", saved("{\"a\":3}"));
        assert!(store.load("revoked", "r1").is_some(), "disk {disk}");
    }
}

/// A stored body that no longer holds its digest is refused when its answer
/// would serve, from memory or disk, whichever value the store hands out,
/// and the answer is forgotten; a probe never serves it.
#[test]
fn a_changed_stored_body_is_refused_when_its_answer_would_serve() {
    for persistent in [false, true] {
        let dir = tempfile::tempdir().expect("dir");
        let store = KeptAnswers::new(persistent.then(|| dir.path().to_owned()));
        let body = "fn kept() {}\n";
        store.save("scope", "read", saved(body));
        assert_eq!(
            store.load("scope", "read").expect("kept").answer.body,
            body,
            "persistent {persistent}"
        );
        let digest = body_digest(&AnswerBody::from(body));
        let changed = AnswerBody::from("fn other() {}\n");
        let length = changed.len();
        store
            .bodies
            .put_confirmed(body_key(&digest), changed, length, UNUSED_RETENTION);
        assert!(
            store.probe("scope", "read").is_some(),
            "a probe reads no body: persistent {persistent}"
        );
        assert_eq!(store.load("scope", "read"), None, "persistent {persistent}");
        assert_eq!(
            store.probe("scope", "read"),
            None,
            "persistent {persistent}"
        );
    }
}

/// The memory store hands out the one value it keeps: it is hashed once
/// however often its answer serves. A disk read is hashed every time.
#[test]
fn a_memory_body_is_hashed_once_and_a_disk_body_on_every_load() {
    for persistent in [false, true] {
        let dir = tempfile::tempdir().expect("dir");
        let store = KeptAnswers::new(persistent.then(|| dir.path().to_owned()));
        store.save("scope", "read", saved("fn kept() {}\n"));
        for _ in 0..3 {
            assert!(store.load("scope", "read").is_some());
        }
        let hashed = store.verified.lock().expect("verified").hashed;
        assert_eq!(
            hashed,
            if persistent { 3 } else { 1 },
            "persistent {persistent}"
        );
    }
}

/// A probe states what a load would reuse: the same headers and
/// observation, so the same reuse; a memory body's length, none of a disk
/// body unread; a held answer whole; nothing for an answer not kept. It
/// hashes no body.
#[test]
fn a_probe_states_what_a_load_would_reuse_without_reading_a_disk_body() {
    let secret = format!("token = \"ghp_{}\"\n", "a1B2c3D4e5F6".repeat(3));
    for persistent in [false, true] {
        let dir = tempfile::tempdir().expect("dir");
        let store = KeptAnswers::new(persistent.then(|| dir.path().to_owned()));
        let body = "fn kept() {}\n";
        store.save("scope", "read", saved(body));
        let (probed, length) = store.probe("scope", "read").expect("probed");
        assert_eq!(store.verified.lock().expect("verified").hashed, 0);
        let loaded = store.load("scope", "read").expect("loaded");
        assert_eq!(probed.answer.headers, loaded.answer.headers);
        assert_eq!(probed.observed_ms, loaded.observed_ms);
        assert_eq!(probed.redirected, loaded.redirected);
        for pinned in [false, true] {
            assert_eq!(
                probed.reuse(pinned, Some(Duration::from_secs(60)), 2),
                loaded.reuse(pinned, Some(Duration::from_secs(60)), 2)
            );
        }
        assert_eq!(
            length,
            (!persistent).then_some(body.len()),
            "persistent {persistent}"
        );
        store.save("scope", "secret", saved(&secret));
        assert_eq!(
            store.probe("scope", "secret"),
            Some((saved(&secret), Some(secret.len())))
        );
        assert_eq!(store.probe("scope", "missing"), None);
    }
}
