use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

/// `value` through the output gate, emails masked when `emails`.
fn seal(value: Value, emails: bool) -> Value {
    if !emails {
        return Sanitized::sealed(value).into_value();
    }
    ContentSecurity::new()
        .seal_output(value, emails, || Ok::<_, std::convert::Infallible>(()))
        .unwrap_or_else(|never| match never {})
        .into_value()
}

#[test]
fn sanitizer_preserves_non_secret_next_query_and_location() {
    // A benign, non-secret-shaped string passes through unchanged — the
    // continuation stays executable and the location is untouched.
    let mut value = json_fixture(
        r#"{
            "content": "token ghp_secretvalue12",
            "next": {
                "nextPage": {
                    "tool": "localFetch",
                    "query": {
                        "queries": [{"path": "/repo/ghp_secretvalue12.rs", "offset": 2}]
                    },
                    "confidence": "exact"
                }
            },
            "location": {"path": "/tmp/ghp_secretvalue12/repo"}
        }"#,
    );
    value = seal(value, false);
    assert_eq!(
        value.pointer("/next/nextPage/query/queries/0/path"),
        Some(&json!("/repo/ghp_secretvalue12.rs"))
    );
    assert_eq!(
        value.pointer("/location/path"),
        Some(&json!("/tmp/ghp_secretvalue12/repo"))
    );
    // The executable tool identifier survives verbatim.
    assert_eq!(
        value.pointer("/next/nextPage/query/queries/0/offset"),
        Some(&json!(2))
    );
}

/// UUIDs are masked only when asked, apart from emails: each switch
/// masks its own kind, wherever a leaf holds one.
#[test]
fn uuid_and_email_masking_are_opt_in_and_independent() {
    const ID: &str = "123e4567-e89b-12d3-a456-426614174000";
    let row = || json!({"id": ID, "author": "dev@example.com", "text": format!("run {ID} by dev@example.com")});
    let seal = |pii: crate::security::Pii| {
        ContentSecurity::new()
            .seal_judged(
                row(),
                &JudgedLeaves::default(),
                &Admitted::default(),
                pii,
                || Ok::<_, std::convert::Infallible>(()),
            )
            .unwrap_or_else(|never| match never {})
            .into_value()
    };
    assert_eq!(seal(crate::security::Pii::default()), row());
    let uuids = seal(crate::security::Pii {
        emails: false,
        uuids: true,
    });
    assert_eq!(uuids["id"], "[REDACTED-UUID]");
    assert_eq!(uuids["author"], "dev@example.com");
    assert_eq!(uuids["text"], "run [REDACTED-UUID] by dev@example.com");
    let both = seal(crate::security::Pii {
        emails: true,
        uuids: true,
    });
    assert_eq!(both["text"], "run [REDACTED-UUID] by [REDACTED-EMAIL]");
}

#[test]
fn email_redaction_is_opt_in_and_masks_commit_authors() {
    let commit_row = || {
        json_fixture(
            r#"{
                "commits": [
                    {
                        "sha": "abc123",
                        "author": {"name": "Dev One", "email": "dev.one+git@example.co.uk"},
                        "message": "fix: reported by user@example.com"
                    }
                ]
            }"#,
        )
    };
    // Default path (no opt-in): emails pass through unchanged.
    let mut untouched = commit_row();
    untouched = seal(untouched, false);
    assert_eq!(untouched, commit_row(), "default output must be unchanged");
    // Opt-in path: every email leaf is masked, structure preserved.
    let mut redacted = commit_row();
    redacted = seal(redacted, true);
    assert_eq!(
        redacted.pointer("/commits/0/author/email"),
        Some(&json!("[REDACTED-EMAIL]"))
    );
    assert_eq!(
        redacted.pointer("/commits/0/message"),
        Some(&json!("fix: reported by [REDACTED-EMAIL]"))
    );
    assert_eq!(
        redacted.pointer("/commits/0/author/name"),
        Some(&json!("Dev One"))
    );
}

/// File text a tool returns beside its source-code `path`
/// is not re-redacted as YAML by the output net, while every credential in
/// it, and `password:` in config files, stay redacted.
#[test]
fn sanitizer_redacts_real_secret_in_next_query_and_location() {
    // A real GitHub token embedded in a continuation query or a location
    // leaf is now redacted — better to break the continuation than to leak.
    let token = format!("ghp_{}", "a".repeat(37));
    let mut value = json!({
        "next": {
            "nextPage": {
                "tool": "localFetch",
                "query": {"queries":[{"path": format!("/repo/{token}.rs")}]}
            }
        },
        "location": {"path": format!("/tmp/{token}/repo")}
    });
    value = seal(value, false);
    // Tool preserved so the call is still routable.
    assert_eq!(
        value.pointer("/next/nextPage/tool"),
        Some(&json!("localFetch"))
    );
    let query_path = value
        .pointer("/next/nextPage/query/queries/0/path")
        .and_then(Value::as_str)
        .expect("query path");
    assert!(!query_path.contains("ghp_"), "token leaked in query path");
    let local_path = value
        .pointer("/location/path")
        .and_then(Value::as_str)
        .expect("path");
    assert!(!local_path.contains("ghp_"), "token leaked in location");
}

/// Round 13: a continuation's runtime state was judged as text, so the
/// stored call lost its `snapshot` and the walk ended in
/// `outputContractViolation`. State the detector would hide (here a
/// token-shaped value in every kind of state field and a fresh handle)
/// passes unchanged; the call's path, a related call's `matchString` taken
/// from a hit, and the row's own text are judged as before.
#[test]
fn continuation_state_is_never_judged() {
    use crate::response::cursor::issued;
    let token = format!("ghp_{}", "a1B2".repeat(9));
    let handle = issued::format(&issued::encode(&[7; 16]), &issued::encode(&[9; 16]));
    let walk = |input: Value| json!({"kind": "continue", "tool": "localFetch", "input": input});
    let response = json!({
        "results": [{"index": 0, "data": {
            "path": "vendor/lib.min.js",
            "content": format!("const t = \"{token}\";"),
            "next": {
                "nextPage": walk(json!({"queries": [{
                    "path": format!("/repo/{token}.min.js"),
                    "snapshot": token,
                    "readProgress": {"unit": "bytes", "offset": 3},
                }]})),
                "quiet": {"kind": "continue", "tool": "clasify", "input": {"queries": [{"operation": "sections", "assessment": token}]}},
            },
            "related": [{"kind": "related", "tool": "localSearch", "input": {"queries": [{"path": "src", "matchString": token}]}}],
        }}],
        "next": walk(json!({"cursor": handle})),
    });
    assert!(
        ContentSecurity::new().sanitize_leaf(&token).is_some(),
        "the detector hides the state value"
    );
    let sealed = seal(response.clone(), false);
    for pointer in [
        "/results/0/data/next/nextPage/input/queries/0/snapshot",
        "/results/0/data/next/nextPage/input/queries/0/readProgress",
        "/results/0/data/next/quiet/input/queries/0/assessment",
        "/next/input/cursor",
    ] {
        assert_eq!(
            sealed.pointer(pointer),
            response.pointer(pointer),
            "{pointer}: {sealed}"
        );
    }
    for judged in [
        "/results/0/data/content",
        "/results/0/data/next/nextPage/input/queries/0/path",
        "/results/0/data/related/0/input/queries/0/matchString",
    ] {
        let text = sealed
            .pointer(judged)
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(
            !text.contains(&token) && text.contains("[REDACTED"),
            "{judged}: {sealed}"
        );
    }
}

/// The tool's own messages are not text of the row's file: they stay
/// out of the structured pass and are judged by the pathless net alone,
/// so the tool's wording about secrets reads as written while a
/// credential a message quotes (a provider's error, quoted where it was
/// built, [`crate::security::quoted`], or not) is hidden, as is a value
/// the content's redaction hid. `redacted` is set exactly where
/// the row's content holds a placeholder: a placeholder in a message
/// alone says nothing about the content.
#[test]
fn a_tools_own_messages_are_never_judged_as_file_text() {
    let wordings = [
        "The parser could not parse lines 5-6 of this file, where secret-shaped text was redacted, so this outline may miss declarations there.",
        "compact view: 177 of 184 bytes the content view serves for the selected lines, all pages (96.19%).",
        "Pass the password the server expects through ~/.octocode/.env; the token is never written to a file.",
    ];
    let secret = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    let token = format!("ghp_{}", "a1B2".repeat(9));
    let row = |content: String, quoted: &str| {
        let mut warnings: Vec<Value> = wordings.iter().map(|wording| json!(wording)).collect();
        if !quoted.is_empty() {
            warnings.push(json!(quoted));
        }
        json!({"index": 0, "data": {
            "path": "config/app.properties",
            "content": content,
            "warnings": warnings,
            "hints": wordings,
            "notices": wordings,
            "timedNotices": [{"text": wordings[0], "seconds": 1.0}],
        }, "meta": {"diagnostics": {"hints": wordings}}})
    };
    let envelope = json!({
        "results": [
            row("name=demo\n".into(), ""),
            row(format!("aws_secret_access_key={secret}\n"), &format!("the line `aws_secret_access_key={secret}` was read")),
            row(
                "name=demo\n".into(),
                &format!(
                    "GitHub said: {}",
                    crate::security::quoted(&format!("bad credentials {token}"))
                ),
            ),
        ],
        "warnings": wordings,
        "hints": wordings,
    });
    let before = crate::response::rows::placeholders_per_row(&envelope);
    let mut sealed = seal(envelope.clone(), false);
    crate::response::rows::warn_redacted_rows(&mut sealed, &before);
    for (index, wording) in wordings.iter().enumerate() {
        for at in ["/warnings", "/hints"]
            .into_iter()
            .map(str::to_owned)
            .chain((0..3).flat_map(|row| {
                ["warnings", "hints", "notices"]
                    .map(|key| format!("/results/{row}/data/{key}"))
                    .into_iter()
                    .chain([format!("/results/{row}/meta/diagnostics/hints")])
            }))
        {
            let pointer = format!("{at}/{index}");
            assert_eq!(
                sealed.pointer(&pointer),
                Some(&json!(wording)),
                "{pointer}: {sealed}"
            );
        }
    }
    let redaction_stated = |row: usize| sealed["results"][row]["data"]["redacted"] == json!(true);
    for row in 0..3 {
        let content = sealed["results"][row]["data"]["content"]
            .as_str()
            .unwrap_or_default();
        assert_eq!(
            redaction_stated(row),
            content.contains("[REDACTED"),
            "row {row}: {sealed}"
        );
    }
    let text = sealed.to_string();
    assert!(!text.contains(secret) && !text.contains(&token), "{sealed}");
    // A message that quoted its outside text where it was built reads
    // the same after the gate: the backstop finds nothing more.
    let built = format!(
        "GitHub said: {}",
        crate::security::quoted(&format!("bad credentials {token}"))
    );
    let sealed = seal(json!({"hints": [built]}), false);
    assert_eq!(sealed["hints"][0], json!(built), "{sealed}");
}

const ARMOR: &str = "-----BEGIN RSA PRIVATE KEY-----";
/// A secret only its well-known key name makes one.
const AWS_SECRET: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";

/// `value` sealed as the response to `rows`, which the input gate admits.
fn seal_for(rows: &[Value], value: Value, marks: &JudgedLeaves) -> Value {
    let rows: Vec<_> = rows
        .iter()
        .map(|row| crate::security::CheckedQuery::checked(row.clone()))
        .collect();
    ContentSecurity::new()
        .seal_judged(
            value,
            marks,
            &Admitted::of(&rows),
            crate::security::Pii::default(),
            || Ok::<_, std::convert::Infallible>(()),
        )
        .unwrap_or_else(|never| match never {})
        .into_value()
}

/// A request value the input gate admitted comes back as sent wherever
/// the response repeats it: a lead's input, an error that quotes it, a
/// hint. The same response to a call that did not send it hides it.
#[test]
fn an_admitted_request_value_is_echoed_as_sent() {
    for own in [
        ARMOR,
        "password: str",
        "token: Token",
        "api_key: str = None",
        "dispatchToken: string",
        "password:\\s*hunter2[",
    ] {
        let response = json!({
            "error": format!("invalidPattern: regex parse error in `{own}`: unclosed class"),
            "hints": [format!("No match for {own}; widen the path.")],
            "related": [{"tool": "localSearch", "input": {"path": "config/app.yaml", "matchString": own}}],
            "next": {"retry": {"tool": "localSearch", "query": {"queries": [{"matchString": own}]}}},
        });
        let row = json!({"path": "config/app.yaml", "matchString": own});
        assert_eq!(
            seal_for(
                std::slice::from_ref(&row),
                response.clone(),
                &JudgedLeaves::default()
            ),
            response,
            "{own}"
        );
    }
    // No request sent this: the gate judges it as any text. An armor
    // line holds no key, so no view hides it (D8), sent or not; a body
    // beside the armor is hidden either way.
    let response = json!({"related": [{"input": {"matchString": ARMOR}}]});
    let sealed = seal_for(
        &[json_fixture(r#"{"matchString": "other"}"#)],
        response.clone(),
        &JudgedLeaves::default(),
    );
    assert_eq!(sealed, response);
    assert_eq!(sealed, seal(response, false));
    let body = "MIIEowIBAAKCAQEAxyzMIIEowIBAAKCAQEAxyzMIIEowIBAAKCAQEAxyzMIIEowIB";
    let keyed = json!({"hit": format!("{ARMOR}\n{body}")});
    for sent in [ARMOR, "other"] {
        let sealed = seal_for(
            &[json!({"matchString": sent})],
            keyed.clone(),
            &JudgedLeaves::default(),
        );
        let hit = sealed["hit"].as_str().expect("hit");
        assert!(
            hit.starts_with(&format!("{ARMOR}\n")) && !hit.contains(body),
            "{sent}: {hit}"
        );
    }
}

/// Only the bytes the caller sent are left: text of a file in the same
/// leaf, a value the request value is only a part of, and every other
/// leaf are judged as before.
#[test]
fn text_around_an_admitted_request_value_is_still_judged() {
    let token = format!("ghp_{}", "a".repeat(36));
    let body = "MIIEowIBAAKCAQEAxyzMIIEowIBAAKCAQEAxyzMIIEowIBAAKCAQEAxyzMIIEowIB";
    let rows = [
        json!({"matchString": ARMOR, "path": "src"}),
        json_fixture(r#"{"matchString": "ghp_aaaa"}"#),
    ];
    let response = json!({
        "lead": format!("{ARMOR} was found beside {token}"),
        "window": format!("{ARMOR}\n{body}\n{body}"),
        "prefix": format!("ghp_aaaa is the start of {token}"),
        "files": [{"path": "config/app.yaml", "matches": [{"value": format!("aws_secret_access_key = {AWS_SECRET}")}]}],
    });
    let sealed = seal_for(&rows, response.clone(), &JudgedLeaves::default());
    let text = sealed.to_string();
    assert!(
        !text.contains(&token) && !text.contains(body) && !text.contains(AWS_SECRET),
        "{text}"
    );
    assert_eq!(
        sealed["lead"],
        json!(format!("{ARMOR} was found beside [REDACTED-GITHUBTOKENS]"))
    );
    // A key written on one line keeps nothing after its armor.
    let one_line = json!({"line": format!("{ARMOR} {body}")});
    let sealed_line = seal_for(&rows, one_line, &JudgedLeaves::default());
    assert!(!sealed_line.to_string().contains(body), "{sealed_line}");
    assert!(
        sealed["window"]
            .as_str()
            .expect("window")
            .starts_with(&format!("{ARMOR}\n")),
        "{sealed}"
    );
    assert!(
        sealed["prefix"]
            .as_str()
            .expect("prefix")
            .starts_with("ghp_aaaa is the start of [REDACTED"),
        "{sealed}"
    );
    // Every leaf that repeats no request value is what the gate alone makes it.
    let alone = seal(response, false);
    assert_eq!(sealed["files"], alone["files"]);
}

/// A value redacted in one leaf is redacted where a leaf about the same
/// source repeats it (a capture of that file), but not inside the request
/// value a lead repeats, not in a leaf of another source, which its own
/// rules judge, and not again in the leaf its own rules judged: one
/// placeholder per value, as a read of each source shows it.
#[test]
fn a_value_hidden_elsewhere_stays_in_the_echoed_request_value() {
    let asked = format!("{AWS_SECRET} login");
    let line = format!("aws_secret_access_key = {AWS_SECRET}\nname: {AWS_SECRET}");
    let rows = [json!({"matchString": asked})];
    let response = json!({
        "files": [
            {
                "path": "config/app.yaml",
                "matches": [{"value": line}],
                "capture": AWS_SECRET,
                "related": [{"input": {"matchString": asked}}],
            },
            {"path": "notes.txt", "matches": [{"value": AWS_SECRET}]},
        ],
    });
    let sealed = seal_for(&rows, response, &JudgedLeaves::default());
    let file = &sealed["files"][0];
    assert_eq!(file["related"][0]["input"]["matchString"], json!(asked));
    assert_eq!(
        file["capture"],
        json!("[REDACTED-AWSSECRETACCESSKEY]"),
        "{sealed}"
    );
    let read = ContentSecurity::new()
        .sanitize_leaf(&line)
        .expect("the read hides the key");
    assert_eq!(file["matches"][0]["value"], json!(read), "{sealed}");
    assert_eq!(
        sealed["files"][1]["matches"][0]["value"],
        json!(AWS_SECRET),
        "{sealed}"
    );
}

/// The gate refuses a credential, so no admitted text holds one: a
/// response can never be asked to echo it.
#[test]
fn a_refused_request_value_is_never_admitted_text() {
    let token = format!("ghp_{}", "a".repeat(36));
    for refused in [
        token.clone(),
        "postgres://admin:Tr0ub4dor3xK9zQ@db.internal:5432/app".to_owned(),
        format!("aws_secret_access_key = {AWS_SECRET}"),
    ] {
        let checked = ContentSecurity::new().check_input(
            json!({"matchString": refused}),
            crate::security::InputPolicy::Query,
        );
        assert!(checked.is_err(), "{refused}");
    }
    // Clasify evidence is redacted, not admitted: it is no request text.
    let evidence = format!("deploy with {token}");
    let row = ContentSecurity::new()
        .check_input(
            json!({"content": evidence, "background": "review"}),
            crate::security::InputPolicy::ClasifyEvidence {
                content_limit: 32_000,
            },
        )
        .expect("redacted in place");
    assert_eq!(
        Admitted::of([&row]),
        Admitted::of([&crate::security::CheckedQuery::checked(json_fixture(
            r#"{"background": "review"}"#
        ))])
    );
}
