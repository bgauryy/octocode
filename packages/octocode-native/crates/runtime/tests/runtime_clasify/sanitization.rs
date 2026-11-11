use super::*;

/// A JSON object's structure is evidence, just as its serialized form is.
/// Keep the public members, folders, types and question ids while hiding data.
#[tokio::test]
async fn sensitive_boundary_structured_content_and_questions_keep_their_context() {
    let secret = "SyntheticCredentialMaterial987654321abcdef";
    let folder = "src/secrets/credentials/private/source.pem";
    let documents = [
        json!({"password":secret,"folder":folder,"enabled":true,"count":3}),
        json!({"kind":"Secret","data":{"payload":secret},"folder":folder}),
        json!({"nested":[{"d":secret,"kty":"RSA","n":"publicModulus","e":"AQAB"}],"folder":folder}),
    ];
    for document in documents {
        let workspace = crate::support::Workspace::new();
        let server = provider(1).await;
        let runtime = crate::support::clasify_runtime(&workspace, &server, &[]);
        let result = clasify(
            &runtime,
            vec![json!({
                "operation":"evaluate", "content":document,
                "questions":{"inspect":{"type":"noul","instructions":document}}
            })],
        )
        .await;
        assert!(
            result["results"][0]["data"]["answers"]["inspect"].is_object(),
            "{result}"
        );
        let requests = sent(&server).await;
        let request = &requests[0];
        assert!(!request.to_string().contains(secret), "{request}");
        assert!(request.to_string().contains("REDACTED"), "{request}");
        assert_eq!(request["state"]["folder"], folder);
        assert_eq!(
            request["questions"]["answer"]["instructions"]["folder"],
            folder
        );
        if document.get("enabled").is_some() {
            assert_eq!(request["state"]["enabled"], true);
            assert_eq!(request["state"]["count"], 3);
        }
    }
}

/// D2: a source a clasify row captures reaches the provider as its reader
/// showed it. The reader judged the file by its own path's rules; the
/// provider's text is judged by the same rules, so a `.properties` rule
/// never re-reads a `.txt` line, the reader's placeholders stay, and the
/// text before and after each stays too.
#[tokio::test]
async fn a_captured_source_reaches_the_provider_as_its_reader_shows_it() {
    let key = "Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2Ua4";
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
        let sent_text = body["state"]["text"].as_str().unwrap_or_default();
        assert_eq!(
            sent_text.trim_end_matches('\n'),
            view.trim_end_matches('\n'),
            "{name}: the provider's text is not the reader's view"
        );
    }
    runtime.close().await;
}
