use super::*;

/// A JSON object's structure is evidence, just as its serialized form is.
/// Keep public members, folders, types and question ids while hiding a fake
/// known-format token in content. Questions containing that token are refused;
/// nonsecret questions keep the same structure and reach the provider intact.
#[tokio::test]
async fn sensitive_boundary_structured_content_and_questions_keep_their_context() {
    let secret = "ghp_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789";
    let folder = "src/secrets/credentials/private/source.pem";
    let documents = [
        (
            json!({"password":secret,"folder":folder,"enabled":true,"count":3}),
            "/password",
        ),
        (
            json!({"kind":"Secret","data":{"payload":secret},"folder":folder}),
            "/data/payload",
        ),
        (
            json!({"nested":[{"d":secret,"kty":"RSA","n":"publicModulus","e":"AQAB"}],"folder":folder}),
            "/nested/0/d",
        ),
    ];
    for (document, secret_path) in documents {
        let workspace = crate::support::Workspace::new();
        let server = provider(1).await;
        let runtime = crate::support::clasify_runtime(&workspace, &server, &[]);
        let refused = runtime
            .run(
                "credential-question".into(),
                "clasify".into(),
                json!({"queries":[{
                    "operation":"evaluate", "content":document,
                    "questions":{"inspect":{"type":"noul","instructions":document}}
                }]}),
            )
            .await
            .expect_err("credential-bearing questions are refused before execution");
        assert_eq!(
            refused.code,
            octocode_contract_types::ErrorCode::InvalidInput,
            "{refused:?}"
        );
        assert!(
            refused.message.contains("questions.inspect.instructions"),
            "{refused:?}"
        );
        assert!(
            sent(&server).await.is_empty(),
            "a refused question never reaches the provider"
        );

        let mut instructions = document.clone();
        *instructions
            .pointer_mut(secret_path)
            .expect("fixture value") = json!("Is this field configured?");
        let result = clasify(
            &runtime,
            vec![json!({
                "operation":"evaluate", "content":document,
                "questions":{"inspect":{"type":"noul","instructions":instructions}}
            })],
        )
        .await;
        assert!(
            result["results"][0]["data"]["answers"]["inspect"].is_object(),
            "{result}"
        );
        let requests = sent(&server).await;
        assert_eq!(requests.len(), 1, "only the nonsecret question is sent");
        let request = &requests[0];
        assert!(!request.to_string().contains(secret), "{request}");
        assert!(request.to_string().contains("REDACTED"), "{request}");
        assert_eq!(request["state"]["folder"], folder);
        assert_eq!(request["questions"]["answer"]["instructions"], instructions);
        if document.get("enabled").is_some() {
            assert_eq!(request["state"]["enabled"], true);
            assert_eq!(request["state"]["count"], 3);
        }
        runtime.close().await;
    }
}

/// A source a clasify row captures reaches the provider as its reader
/// showed it. A known-format token is hidden in each source representation;
/// the reader's placeholders and the surrounding text reach the provider
/// unchanged, including text whose credential-shaped names are not secrets.
#[tokio::test]
async fn a_captured_source_reaches_the_provider_as_its_reader_shows_it() {
    let key = "ghp_Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2";
    let nonce = "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==  tailH\n";
    let files = [
        (
            "notes.txt",
            format!("{nonce}api_key: {key}  tailT\npassword: str\nplain line\n"),
        ),
        (
            "conf.py",
            format!("api_key = \"{key}\" + suffix_visible5\nport = 8080\n"),
        ),
        (
            "conf.ts",
            format!("const apiKey = cond ? \"{key}\" : \"otherVisible3\";\nexport {{ apiKey }};\n"),
        ),
        (
            "app.properties",
            format!("db.password={key}\nname=visible\n"),
        ),
        ("README.md", format!("{nonce}Set `token: {key}` then go.\n")),
    ];
    let workspace = crate::support::Workspace::new();
    let server = provider(u64::try_from(files.len()).unwrap_or_default()).await;
    let runtime = crate::support::clasify_runtime(&workspace, &server, &[]);
    let mut views = std::collections::BTreeMap::new();
    for (name, source) in &files {
        let path = workspace.write(name, source);
        let read: Value = runtime
            .run(
                "view".into(),
                "localFetch".into(),
                json!({"queries":[{"path": path}]}),
            )
            .await
            .expect("read")
            .structured_content
            .into_value();
        let view = read["results"][0]["data"]["content"]
            .as_str()
            .unwrap_or_else(|| panic!("no content: {read}"))
            .lines()
            .map(|line| line.split_once('\t').map_or(line, |(_, text)| text))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!view.contains(key), "{name}: {view}");
        let result = clasify(
            &runtime,
            vec![json!({
                "operation":"evaluate",
                "source":{"tool":"localFetch","query":{"path": path}},
                "questions":{"a":noul("Is a credential set?")}
            })],
        )
        .await;
        assert!(
            result["results"][0]["data"]["answers"]["a"].is_object(),
            "{name}: {result}"
        );
        views.insert(path.to_string_lossy().into_owned(), (*name, view));
    }
    let bodies = sent(&server).await;
    assert_eq!(bodies.len(), files.len());
    for body in &bodies {
        let path = body["state"]["source"]["path"].as_str().unwrap_or_default();
        let (name, view) = views
            .iter()
            .find(|(read, _)| read.ends_with(path) || path.ends_with(read.as_str()))
            .map(|(_, view)| view)
            .unwrap_or_else(|| panic!("no read for {path}: {body}"));
        // The text judged whole carries each part between its marker lines
        // (`<lines 1-40>` … `</lines 1-40>`, `<lines 41-80 continues>`); the
        // lines between them are the reader's view.
        let sent_text = body["state"]["text"]
            .as_str()
            .unwrap_or_default()
            .split_inclusive('\n')
            .filter(|line| {
                let line = line.trim_end();
                !line
                    .strip_prefix("</lines ")
                    .or_else(|| line.strip_prefix("<lines "))
                    .and_then(|rest| rest.strip_suffix('>'))
                    .map(|rest| rest.strip_suffix(" continues").unwrap_or(rest))
                    .and_then(|rest| rest.split_once('-'))
                    .is_some_and(|(a, b)| a.parse::<usize>().is_ok() && b.parse::<usize>().is_ok())
            })
            .collect::<String>();
        assert_eq!(
            sent_text.trim_end_matches('\n'),
            view.trim_end_matches('\n'),
            "{name}: the provider's text is not the reader's view"
        );
    }
    runtime.close().await;
}

/// Item 9 (bin16 retest): a captured local source reaches the provider
/// named as a result row names it, by its path in the workspace: never the
/// absolute path, so no home directory or user name leaves the host.
#[tokio::test]
async fn a_captured_source_names_its_workspace_path_to_the_provider() {
    let workspace = crate::support::Workspace::new();
    let server = provider(2).await;
    let runtime = crate::support::clasify_runtime(&workspace, &server, &[]);
    let path = workspace.write("src/deep/notes.txt", "plain line\nsecond line\n");
    for named in [
        path.to_string_lossy().into_owned(),
        "src/deep/notes.txt".to_owned(),
    ] {
        let result = clasify(
            &runtime,
            vec![json!({
                "operation":"evaluate",
                "source":{"tool":"localFetch","query":{"path": named}},
                "questions":{"a":noul("Is a credential set?")}
            })],
        )
        .await;
        assert!(
            result["results"][0]["data"]["answers"]["a"].is_object(),
            "{result}"
        );
    }
    let bodies = sent(&server).await;
    assert!(!bodies.is_empty());
    let root = workspace.workspace.to_string_lossy().into_owned();
    for body in &bodies {
        assert_eq!(
            body["state"]["source"]["path"], "src/deep/notes.txt",
            "{body}"
        );
        assert!(!body.to_string().contains(&root), "{body}");
    }
    runtime.close().await;
}

/// One placeholder per hidden line, the same in every view of a
/// key file. A localFetch read, a localSearch row in content and in
/// matchOnly, and clasify's inline content each show a key's line as the
/// read does; the lines that hold no private key (a PuTTY file's headers and
/// public lines, a PGP armor's `Version:`) read as written in each. Every
/// value is fabricated.
#[tokio::test]
async fn a_key_file_shows_one_placeholder_form_in_every_view() {
    let body =
        |seed: &str| format!("{seed}{}", "Qk9EWUZBS0VMSU5FTk9UQUtFWQ".repeat(3))[..64].to_owned();
    let (public, private, pgp, rsa) = (
        body("AAAAB3Nz"),
        body("pRiVaTeL"),
        body("lQOYpGpL"),
        body("MIIEfAkE"),
    );
    let file = format!(
        "PuTTY-User-Key-File-2: ssh-rsa\nEncryption: none\nComment: rsa-key-20261112\nPublic-Lines: 1\n{public}\nPrivate-Lines: 2\n{private}\n{private}\nPrivate-MAC: 9f8e7d6c5b4a39281706aabbccdd\n\n-----BEGIN PGP PRIVATE KEY BLOCK-----\nVersion: GnuPG v2\n\n{pgp}\n{pgp}\n-----END PGP PRIVATE KEY BLOCK-----\n-----BEGIN RSA PRIVATE KEY-----\n{rsa}\n{rsa}\n-----END RSA PRIVATE KEY-----\n"
    );
    let workspace = crate::support::Workspace::new();
    let server = provider(1).await;
    let runtime = crate::support::clasify_runtime(&workspace, &server, &[]);
    let path = workspace.write("keys.txt", &file);
    let data = |output: Value| output["results"][0]["data"].clone();
    let read = data(
        runtime
            .run(
                "view".into(),
                "localFetch".into(),
                json!({"queries":[{"path": path}]}),
            )
            .await
            .expect("read")
            .structured_content
            .into_value(),
    );
    let view: Vec<String> = read["content"]
        .as_str()
        .unwrap_or_else(|| panic!("no content: {read}"))
        .lines()
        .map(|line| {
            line.split_once('\t')
                .map_or(line, |(_, text)| text)
                .to_owned()
        })
        .collect();
    let placeholder = "[REDACTED-PRIVATEKEYFRAGMENT]";
    for (at, line) in file.lines().enumerate() {
        let shown = &view[at];
        if [&private, &pgp, &rsa]
            .iter()
            .any(|key| line == key.as_str())
        {
            assert_eq!(shown, placeholder, "line {}: {view:?}", at + 1);
        } else if line.starts_with("Private-MAC:") {
            assert_eq!(shown, &format!("Private-MAC: {placeholder}"), "{view:?}");
        } else {
            assert_eq!(shown, line, "line {}: {view:?}", at + 1);
        }
    }
    // A search reads what a read shows: a content row's lines around each
    // block are the read's lines, and a search for the placeholder finds
    // nothing: it is the read's text, not the file's.
    let search = |query: Value| {
        let runtime = &runtime;
        async move {
            data(
                runtime
                    .run(
                        "search".into(),
                        "localSearch".into(),
                        json!({"queries":[query]}),
                    )
                    .await
                    .expect("search")
                    .structured_content
                    .into_value(),
            )
        }
    };
    for needle in ["Private-Lines:", "BEGIN PGP", "BEGIN RSA"] {
        let found = search(json!({"path": path, "matchString":[needle], "view":"content", "contextLines":{"before":4,"after":4}})).await;
        let mut checked = 0;
        for hit in found["files"][0]["matches"]
            .as_array()
            .unwrap_or_else(|| panic!("{needle}: {found}"))
        {
            for line in hit["value"].as_str().expect("value").lines() {
                let Some((number, text)) = line.split_once('\t') else {
                    continue;
                };
                let number: usize = number.trim().parse().expect("line number");
                assert_eq!(text, view[number - 1], "{needle} line {number}: {found}");
                checked += 1;
            }
        }
        assert!(checked >= 5, "{needle}: {found}");
    }
    // The placeholder is text the read wrote, never the file's: a search for
    // it matches nothing, and the empty row says masked secrets are not
    // searched, whatever was masked. No key text is served.
    let served = runtime
        .run(
            "search".into(),
            "localSearch".into(),
            json!({"queries":[{"path": path, "matchString":[placeholder], "regexEngine":"literal", "view":"matchOnly"}]}),
        )
        .await
        .expect("search")
        .structured_content
        .into_value();
    let row = &served["results"][0]["data"];
    assert!(
        row["files"].as_array().is_none_or(Vec::is_empty),
        "{served}"
    );
    assert!(
        row["hints"]
            .as_array()
            .is_some_and(|hints| hints.len() == 1 && hints[0].to_string().contains("maskedSpans")),
        "an empty answer over masked text says what was not searched, once: {served}"
    );
    let text = served.to_string();
    for key in [&private, &pgp, &rsa] {
        assert!(!text.contains(key.as_str()), "{served}");
    }
    let result = clasify(
        &runtime,
        vec![json!({
            "operation":"evaluate",
            "content":{"document":{"text": file}},
            "questions":{"a":noul("Does `document.text` hold a key?")}
        })],
    )
    .await;
    assert!(
        result["results"][0]["data"]["answers"]["a"].is_object(),
        "{result}"
    );
    let bodies = sent(&server).await;
    let sent_text = bodies[0]["state"]["document"]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("{}", bodies[0]));
    assert_eq!(
        sent_text.trim_end_matches('\n'),
        view.join("\n"),
        "clasify's inline content is not the read's view"
    );
    runtime.close().await;
}
