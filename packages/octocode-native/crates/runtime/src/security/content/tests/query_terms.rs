//! Which request strings the input gate refuses as a credential: whatever
//! a read of text of no known file hides, and nothing a read shows.

use super::*;

fn refused(field: &str, value: serde_json::Value) -> bool {
    !ContentSecurity::new()
        .validate_input_parameters(&serde_json::json!({ field: value }))
        .secret_fields
        .is_empty()
}

#[test]
fn a_search_for_a_credential_named_declaration_runs() {
    for term in [
        "password: str",
        "password: string",
        "token: string",
        "token: Token",
        "api_key: str = None",
        "password = None",
        "secret: Option<String>",
        "apiKey?: string;",
        "const token = \"\"",
        "API_KEY=",
        "token = os.environ[\"GITHUB_TOKEN\"]",
        "secret_key: bytes = b\"\"",
        "password: Annotated[str, Field(min_length=8)]",
    ] {
        for field in ["matchString", "keywords", "questions"] {
            assert!(
                !refused(field, serde_json::json!([term])),
                "{field} {term:?} was refused as a credential"
            );
        }
        assert!(!refused("matchString", serde_json::json!(term)), "{term:?}");
    }
}

/// A known secret format is refused whatever names it; a value in no known
/// format under a credential's name is the caller's own text and runs.
#[test]
fn a_known_secret_is_refused_whatever_names_it() {
    let github = format!("ghp_{}", "a1B2".repeat(9));
    let named = format!("password: {github}");
    for term in [
        github.as_str(),
        named.as_str(),
        "AKIAIOSFODNN7EXAMPLE",
        "aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
    ] {
        for field in ["matchString", "keywords"] {
            assert!(
                refused(field, serde_json::json!([term])),
                "{field} {term:?} was run"
            );
        }
        assert!(refused("matchString", serde_json::json!(term)), "{term:?}");
    }
    for term in [
        "password: Tr0ub4dor&3xK9!zQ",
        "token: 9f8Kq2LmZx7Wv3Tn5Rb1Yc6H",
    ] {
        assert!(
            !refused("matchString", serde_json::json!([term])),
            "{term:?}"
        );
    }
}

#[test]
fn a_refused_value_is_redacted_where_it_stands() {
    let github = format!("ghp_{}", "a1B2".repeat(9));
    let result = ContentSecurity::new().validate_input_parameters(&serde_json::json!({
        "matchString": [format!("token: {github}"), "password: str"],
    }));
    let kept = serde_json::to_string(&result.sanitized_params).expect("serializable");
    assert!(!kept.contains(&github), "{kept}");
    assert!(kept.contains("password: str"), "{kept}");
}

const TOKEN: &str = "ghp_Zq8Wv3Xr7Tn2Lp5Ab1Cd9Ef4Gh6Jk8Lm0Np2Q";
const OPENSSH_BODY: &str = "b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAABlwAAAAdzc2gtcn";
const BODY_LINE: &str = "WzHvuy3fEWJr5musirUr5JYe5CMpm8DI2mKvqGsxgG5oDTudHZGh0Z7tcHL5kVeqJy";

/// Every screened field judges a string alike: a search term, a keyword, the
/// request's goal, a classification predicate.
fn refused_everywhere(term: &str) -> bool {
    let fields = [
        serde_json::json!({ "matchString": [term] }),
        serde_json::json!({ "keywords": [term] }),
        serde_json::json!({ "mainGoal": term }),
        serde_json::json!({ "classification": { "predicates": [term], "context": "plain" } }),
        serde_json::json!({ "classification": { "predicates": ["plain"], "context": term } }),
    ];
    let verdicts: Vec<bool> = fields
        .iter()
        .map(|params| {
            !ContentSecurity::new()
                .validate_input_parameters(params)
                .secret_fields
                .is_empty()
        })
        .collect();
    assert!(
        verdicts.iter().all(|refused| *refused == verdicts[0]),
        "{term:?} is judged apart by field: {verdicts:?}"
    );
    verdicts[0]
}

#[test]
fn a_known_secret_inside_a_sentence_is_refused() {
    for term in [
        format!("locate password: {TOKEN}"),
        format!("find token={TOKEN} in config"),
        format!("Does the text contain {TOKEN}?"),
        format!("the token {TOKEN}"),
    ] {
        assert!(refused_everywhere(&term), "{term:?} was run");
    }
    for term in [
        "locate password: str",
        "find token: Token in the client",
        "where api_key: str = None is declared",
        "the dispatchToken: string member",
        "Does the function take a password: str parameter?",
    ] {
        assert!(!refused_everywhere(term), "{term:?} was refused");
    }
}

#[test]
fn a_secret_spelled_as_a_regex_is_refused() {
    let github = format!("ghp_{}", "a1B2".repeat(9));
    let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9\\.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ\\.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    let class = format!("{}[2]", github.trim_end_matches('2'));
    for term in [jwt, class.as_str()] {
        assert!(refused_everywhere(term), "{term:?} was run");
    }
    for term in [
        "ghp_[A-Za-z0-9]{36}",
        "eyJ[\\w-]+\\.eyJ[\\w-]+\\.",
        "token\\s*[:=]",
    ] {
        assert!(!refused_everywhere(term), "{term:?} was refused");
    }
}

/// The armor line is the same text in every key and no part of any: a search
/// for it runs. A key's body does not, under its armor line or alone when
/// its format's own header opens it.
#[test]
fn a_key_body_is_refused_and_its_armor_line_runs() {
    let partial = format!("-----BEGIN RSA PRIVATE KEY-----\n{BODY_LINE}");
    let escaped = format!("-----BEGIN RSA PRIVATE KEY-----\\n{BODY_LINE}");
    let one_line = format!("-----BEGIN RSA PRIVATE KEY-----{BODY_LINE}");
    for term in [
        OPENSSH_BODY,
        partial.as_str(),
        escaped.as_str(),
        one_line.as_str(),
    ] {
        assert!(refused_everywhere(term), "{term:?} was run");
    }
    for term in [
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----BEGIN OPENSSH PRIVATE KEY-----",
        "-----END OPENSSH PRIVATE KEY-----",
        "-----BEGIN CERTIFICATE-----",
        "-----BEGIN (RSA |OPENSSH )?PRIVATE KEY-----",
        "find files that hold -----BEGIN RSA PRIVATE KEY-----",
        BODY_LINE,
    ] {
        assert!(!refused_everywhere(term), "{term:?} was refused");
    }
    // A refused value never stays as written, whichever rule refused it.
    for term in [OPENSSH_BODY, escaped.as_str()] {
        let result = ContentSecurity::new()
            .validate_input_parameters(&serde_json::json!({ "matchString": [term] }));
        let kept = serde_json::to_string(&result.sanitized_params).expect("serializable");
        assert!(
            !kept.contains(&BODY_LINE[..24]) && !kept.contains(&OPENSSH_BODY[..24]),
            "{kept}"
        );
    }
}

/// A URL's userinfo password is a known format, the word `password` too; a
/// URL with no password, or one whose password is wholly an interpolation or
/// a template placeholder, runs.
#[test]
fn a_url_password_is_refused_and_a_url_without_one_runs() {
    for term in [
        "postgres://user:password@localhost:5432/db",
        "https://user:pass@example.com",
        "postgres://admin:Tr0ub4dor3xK9zQ@db.internal:5432/app",
        "https://deploy:s3cr3t!@example.com",
        "postgres://u:${PGPASSWORD}x@db",
    ] {
        assert!(refused_everywhere(term), "{term:?} was run");
    }
    for term in [
        "postgres://localhost:5432/db",
        "https://example.com/path",
        "postgres://u:${PGPASSWORD}@db",
        "postgresql://app:$DB_PASS@db.internal:5432/app",
        "redis://default:{{ .Values.redis.password }}@redis:6379",
        "mongodb://app:<password>@cluster0.mongodb.net/app",
        "mysql://root:%(password)s@db/app",
    ] {
        assert!(!refused_everywhere(term), "{term:?} was refused");
    }
}

/// Round 12 (D9, D4, D5, D6, D8): the input gate refuses exactly what a
/// read hides: each text a read of a source file, a text file or text of no
/// known file shows runs in every screened field, and each it hides is
/// refused there, before anything leaves the machine.
#[test]
fn the_gate_refuses_exactly_what_a_read_hides() {
    let security = ContentSecurity::new();
    for text in [
        "password = \"hunter2-Sup3rS3cret\"",
        "password = \"otherVisible9\"",
        "Bearer abcdefghijklmnopqrstuvwx",
        "headers[\"Authorization\"] = \"Bearer abcdefghijklmnopqrstuvwx\";",
        "const h = { Authorization: \"Bearer abcdefghijklmnopqrstuvwx\" };",
        "const h = { Authorization: `Bearer abcdefghijklmnopqrstuvwx` };",
        "my password was Pl4net-Orb1t9",
        "the password is: hunter2-Sup3rS3cret",
        "then password = hunter2-Sup3rS3cret",
        "the password is: s3cretPassw0rd",
        "the password is required",
        "my password was changed",
        "the token was expired",
        "token: T | null",
        "secret: T",
        "  token: this.token,",
        "  secret: opts.secret,",
        "  password: undefined,",
        "  apiKey: null,",
        "password: str",
    ] {
        let hidden = security.sanitize_text(text, None).content != text;
        for path in ["src/a.ts", "notes.txt", "src/a.py"] {
            let read = security.sanitize_text(text, Some(std::path::Path::new(path)));
            assert_eq!(read.content != text, hidden, "{path}: {text:?}");
        }
        assert_eq!(refused_everywhere(text), hidden, "{text:?}");
    }
}

/// D10: clasify evidence with no path is judged as a read judges text of no
/// known file: the provider reads what every view shows, quotes and shell
/// syntax kept, one placeholder per value.
#[test]
fn clasify_evidence_is_judged_as_text_of_no_known_file() {
    const KEY: &str = TOKEN;
    let security = ContentSecurity::new();
    for evidence in [
        format!("password = \"{KEY}\"\n"),
        format!("API_KEY=\"${{API_KEY:-{KEY}}}\"\n"),
        format!("x = 1 # a\\.b {KEY}\n"),
    ] {
        let checked = security
            .check_input(
                serde_json::json!({"content": evidence, "questions": {"q": {"type": "bool", "instructions": "is it set?"}}}),
                crate::security::InputPolicy::ClasifyEvidence {
                    content_limit: crate::id::clasify_policy::REQUEST_BYTES,
                },
            )
            .expect("evidence is redacted in place, not refused");
        let read = security.sanitize_text(&evidence, None).content.into_owned();
        assert_eq!(checked["content"], serde_json::json!(read), "{evidence:?}");
    }
    let read = security
        .sanitize_text(&format!("API_KEY=\"${{API_KEY:-{KEY}}}\"\n"), None)
        .content
        .into_owned();
    assert!(
        read.starts_with("API_KEY=\"${API_KEY:-[REDACTED-"),
        "{read}"
    );
    let quoted = security
        .sanitize_text(&format!("password = \"{KEY}\"\n"), None)
        .content
        .into_owned();
    assert!(
        quoted.starts_with("password = \"[REDACTED-") && quoted.ends_with("]\"\n"),
        "{quoted}"
    );
}
