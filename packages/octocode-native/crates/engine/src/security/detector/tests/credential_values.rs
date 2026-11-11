//! Credential assignments, encoded keys and rule boundaries.

use super::*;

mod code_forms;
mod config_entries;
mod data_shaped_phrases;
mod described_values;
mod dotenv_files;
mod dotenv_shapes;
mod locators;
mod phrases;
mod properties_files;
mod readable_code;
mod short_literals;
mod source_forms;

/// A hex value is no proof of a git object id: tokens of that shape (classic
/// GitHub PATs, CircleCI, Sentry) under a credential-named key stay redacted,
/// in file content and in a structured response alike. A data value such as a
/// branch name must therefore stay out of JSON key position instead.
#[test]
fn hex_shaped_tokens_under_credential_names_stay_redacted() {
    let hex40 = "df5e4353e6dbef3b8a6e70441e7322092dbe110b";
    let hex64 = format!("{hex40}0123456789abcdef01234567");
    for secret in [hex40, hex64.as_str()] {
        for key in ["GITHUB_TOKEN", "secret/data/prod/datadog_app_key"] {
            let content = format!("{{\"{key}\": \"{secret}\"}}\n");
            let result = detect(&content, None);
            assert!(!result.sanitized.contains(secret), "leaked: {content}");
            let mut value = serde_json::json!({ key: secret });
            crate::security::sanitizer::sanitize_structure(&mut value).expect("scan");
            assert!(!value.to_string().contains(secret), "leaked: {value}");
        }
    }
}

/// A base64-encoded PEM private key (Kubernetes `data:`, Helm values,
/// JSON) is redacted in any window, without `kind: Secret` context.
#[test]
fn base64_encoded_private_key_is_redacted_without_manifest_context() {
    for kind in ["", "RSA ", "EC ", "OPENSSH ", "ENCRYPTED ", "PGP "] {
        let suffix = if kind == "PGP " { " BLOCK" } else { "" };
        let pem = format!(
            "-----BEGIN {kind}PRIVATE KEY{suffix}-----\n{}\n-----END {kind}PRIVATE KEY{suffix}-----\n",
            fake_b64(64)
        );
        let encoded = b64(&pem);
        for content in [
            format!("  tls.key: {encoded}\n"),
            format!("{{\"key\":\"{encoded}\"}}"),
            format!("key: |\n  {}\n  {}\n", &encoded[..76], &encoded[76..]),
        ] {
            let result = detect(&content, None);
            assert!(
                !result.sanitized.contains(&encoded[20..60]),
                "{kind}: base64 key body leaked: {}",
                result.sanitized
            );
            assert!(!result.secrets_detected.is_empty());
            assert_eq!(result.sanitized.lines().count(), content.lines().count());
        }
    }
    let cert = b64(&format!(
        "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
        fake_b64(64)
    ));
    let line = format!("  tls.crt: {cert}\n");
    assert_eq!(
        detect(&line, None).sanitized,
        line,
        "public certificates stay readable"
    );
}

/// Credential-named assignments in camelCase, JSON keys, YAML and
/// unquoted dotenv lines redact only the value and keep the identifier.
#[test]
fn credential_assignments_redact_only_the_value() {
    let value = format!("c7Tvo+hBKqFY{}", fake_b64(28));
    let cases = [
        (
            format!("const awsSecret = \"{value}\";"),
            "const awsSecret = \"",
        ),
        (
            format!("const awsSecret: \"{value}\""),
            "const awsSecret: \"",
        ),
        (
            format!("{{\"clientSecret\": \"{value}\"}}"),
            "{\"clientSecret\": \"",
        ),
        (
            format!("AWS_SECRET_ACCESS_KEY = \"{value}\""),
            "AWS_SECRET_ACCESS_KEY = \"",
        ),
        (format!("API_SECRET={value}\n"), "API_SECRET="),
        (
            format!("export DB_PASSWORD={value} # prod\n"),
            "export DB_PASSWORD=",
        ),
        (format!("  client_secret: {value}\n"), "  client_secret: "),
        (format!("  apiKey: {value}\n"), "  apiKey: "),
        (format!("jwt_secret = \"{value}\";"), "jwt_secret = \""),
    ];
    for (content, kept) in cases {
        let result = detect(&content, None);
        assert!(
            !result.sanitized.contains(&value[..16]),
            "value leaked: {content:?} -> {:?}",
            result.sanitized
        );
        assert!(
            result.sanitized.starts_with(kept),
            "identifier lost: {content:?} -> {:?}",
            result.sanitized
        );
    }
}

/// The credential-assignment rules leave code and config references alone.
#[test]
fn credential_assignments_skip_references_and_plain_keys() {
    for content in [
        "SECRET_KEY = os.environ[\"DJANGO_SECRET_KEY\"]\n",
        "API_TOKEN=${API_TOKEN_FROM_VAULT}\n",
        "  token: ${{ secrets.GITHUB_TOKEN }}\n",
        "  - key: node-role.kubernetes.io/control-plane\n",
        "  secretName: my-ingress-tls-secret\n",
        "const tokenType = \"Bearer\";\n",
    ] {
        assert_eq!(detect(content, None).sanitized, content);
    }
}

/// An unquoted assignment that ends a code statement (`;`) with a name reads
/// another variable or member, not a literal: it stays verbatim in any file,
/// while the same keys in dotenv, INI, properties and shell syntax keep their
/// values redacted.
#[test]
fn credential_assignments_keep_code_statement_references() {
    for (content, path) in [
        ("        frame.expect_key = frame.object;\n", None),
        (
            "    held.key_lines = point.inside_lines;\n",
            Some("src/large_source.rs"),
        ),
        (
            "  this.secret_token = !options.secret_token;\n",
            Some("a.ts"),
        ),
        ("api_key = default_api_key;\n", None),
        // Minified code: more statements follow on the line.
        ("x.password=t.password;username=u\n", Some("dist/app.js")),
        ("x.password=t.password;username=u\n", Some("README.md")),
        ("x.password=t.password;username=u\n", None),
        ("cfg.apiToken=opts.apiToken;cfg.retries=3\n", Some("a.ts")),
    ] {
        let result = detect(content, path);
        assert_eq!(result.sanitized, content, "{path:?}");
        assert!(result.secrets_detected.is_empty(), "{content:?}");
    }
    let value = format!("c7Tvo+hBKqFY{}", fake_b64(28));
    for (content, path) in [
        (format!("API_SECRET={value}\n"), Some(".env")),
        (format!("export DB_PASSWORD={value}\n"), Some("deploy.sh")),
        (format!("db.password={value}\n"), Some("app.properties")),
        (
            format!("[db]\nsecret_key = {value}\n"),
            Some("settings.ini"),
        ),
        ("PASSWORD=correct.horse.battery.staple\n".to_owned(), None),
        ("API_TOKEN=abcdefghijklmnopqrst;\n".to_owned(), Some(".env")),
        ("DB_PASSWORD=Xk9vQ2;pQ2=zz7Lm4\n".to_owned(), Some(".env")),
        (
            "db.password=Xk9vQ2;pQ2=zz7Lm4\n".to_owned(),
            Some("app.properties"),
        ),
    ] {
        let result = detect(&content, path);
        // A settings file's own rule runs first and takes its credentials.
        let rule =
            if path.is_some_and(|path| path.ends_with(".properties") || path.ends_with(".ini")) {
                "propertiesFileCredential"
            } else {
                "dotenvSecretAssignment"
            };
        assert!(
            result.secrets_detected.iter().any(|name| name == rule),
            "not redacted: {content:?} -> {:?}",
            result.sanitized
        );
    }
}

/// A credential-named key whose value spells a code identifier names a
/// field, so it stays verbatim in every assignment form; a credential value
/// under the same keys is still redacted.
#[test]
fn credential_assignments_keep_identifier_values() {
    for content in [
        "\"x-kubernetes-patch-merge-key\": \"resourceClaimName\",\n",
        // Long enough for the base64 shape: still a field name.
        "\"x-kubernetes-patch-merge-key\": \"resourceClaimNameForExtendedResources\",\n",
        "const sortKey = \"createdAtTimestamp\";\n",
        "PRIMARY_KEY=ResourceClaimName\n",
        "  apiKey: resourceClaimName\n",
    ] {
        let result = detect(content, None);
        assert_eq!(result.sanitized, content);
        assert!(result.secrets_detected.is_empty(), "{content:?}");
    }
    let value = format!("c7Tvo+hBKqFY{}", fake_b64(28));
    for (content, secret) in [
        (
            format!("\"x-kubernetes-patch-merge-key\": \"{value}\""),
            value.as_str(),
        ),
        (
            "const sortKey = \"resourceClaimName7\";".to_owned(),
            "resourceClaimName7",
        ),
        (
            "const apiToken = \"resourceCLaimName\";".to_owned(),
            "resourceCLaimName",
        ),
        (
            "const apiToken = \"abcdefghijklmnopqrstuvwx\";".to_owned(),
            "abcdefghijklmnopqrstuvwx",
        ),
    ] {
        let result = detect(&content, None);
        assert!(!result.sanitized.contains(secret), "leaked: {content:?}");
        // Independent value-shape rules may redact before the assignment rule.
        // This case proves redaction, not precedence between overlapping rules.
        assert!(!result.secrets_detected.is_empty(), "{content:?}");
    }
}

/// A dotted path of names under a `key`, `token` or `credential` name names a
/// key, a member or a message (`matchKeys` rows, `secretKeyRef.key`, i18n
/// keys), in a file and in a structured response; a person's chosen secret
/// with dots, and token shapes, stay redacted.
#[test]
fn key_paths_under_an_opaque_credential_name_are_names() {
    let paths = [
        "environment.TAVILY_API_KEY",
        "environment.GH_TOKEN.description",
        "settings.api_token.label",
        "servers[0].auth.refreshToken",
    ];
    let mut wrong = Vec::new();
    for path in paths {
        for content in [
            format!("{{\"key\": \"{path}\"}}\n"),
            format!("const tokenKey = \"{path}\";\n"),
        ] {
            if detect(&content, Some("a/b.json")).sanitized != content {
                wrong.push(format!("redacted: {content}"));
            }
        }
        let mut row = serde_json::json!([{ "line": 47, "key": path }]);
        let before = row.clone();
        crate::security::sanitizer::sanitize_structure(&mut row).expect("scan");
        if row != before {
            wrong.push(format!("structured: {row}"));
        }
    }
    for (content, secret) in [
        (
            "{\"password\": \"correct.horse.battery\"}\n",
            "correct.horse.battery",
        ),
        (
            "{\"key\": \"Xq7Lm2.Pz9Kd4Wr8.Tn3\"}\n",
            "Xq7Lm2.Pz9Kd4Wr8.Tn3",
        ),
        (
            "{\"apiKey\": \"qzxvbnmtr.wkplmnbvc\"}\n",
            "qzxvbnmtr.wkplmnbvc",
        ),
    ] {
        if detect(content, Some("a/b.json")).sanitized.contains(secret) {
            wrong.push(format!("leaked: {content}"));
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
}

#[test]
fn identifier_names_are_whole_camel_case_words() {
    for name in [
        "resourceClaimName",
        "ContainerPort",
        "createdAtTimestamp",
        "userId",
    ] {
        assert!(is_identifier_name(name), "{name}");
    }
    for value in [
        "abcdefghijklmnopqrstuvwx",
        "resourceClaimName7",
        "resourceCLaimName",
        "aBcDeFgHiJkLmNoPq",
        "qXzvbnTrwkpLmnbvc",
        "resource_claim_name",
        "",
    ] {
        assert!(!is_identifier_name(value), "{value}");
    }
}

/// A Kubernetes Secret keeps its `kind:`/`data:` lines and line count;
/// every data value is redacted whichever key comes first.
#[test]
fn kubernetes_secret_redacts_every_data_value_in_either_key_order() {
    let one = fake_b64(24);
    let two = format!("{}xyz", fake_b64(20));
    let kind_first = format!(
        "apiVersion: v1\nkind: Secret\ndata:\n  username: {one}\n  password: {two}\nmetadata:\n  name: db\n"
    );
    let data_first = format!(
        "apiVersion: v1\ndata:\n  username: {one}\n  password: {two}\nkind: Secret\nmetadata:\n  name: db\n"
    );
    for content in [kind_first, data_first] {
        let result = detect(&content, Some("k8s/secret.yaml"));
        assert!(!result.sanitized.contains(&one), "{}", result.sanitized);
        assert!(!result.sanitized.contains(&two), "{}", result.sanitized);
        assert!(result.sanitized.contains("kind: Secret\n"));
        assert!(result.sanitized.contains("\ndata:\n"));
        assert!(result.sanitized.contains("  name: db\n"));
        assert_eq!(result.sanitized.lines().count(), content.lines().count());
    }
}

/// A Stripe-shaped `sk_live_` key carries the Stripe label.
#[test]
fn stripe_secret_key_is_labelled_stripe() {
    let key = format!("sk_{}_{}", "live", "a1B2c3D4e5".repeat(3));
    let result = detect(&format!("STRIPE = '{key}'"), None);
    assert!(!result.sanitized.contains(&key));
    assert_eq!(
        result.secrets_detected.first().map(String::as_str),
        Some("stripeSecretKey")
    );
}

/// Rules bounded by quotes fire when the quote is followed by punctuation
/// (`"…";`, `"…"}`): a `\b` beside a quote needs a word character there.
#[test]
fn quote_bounded_rules_fire_before_punctuation() {
    let id = "a1B2c3D4e5F6g7H8i9J0k1L2";
    let result = detect(&format!("{{\"client_secret\":\"{id}\"}}"), None);
    assert!(!result.sanitized.contains(id), "{}", result.sanitized);
    assert!(result.sanitized.starts_with("{\"client_secret\":\""));
}

/// A hex digest keyed by a file path is a checksum manifest entry, even when
/// the file's name holds a credential word; the same digest under a
/// credential key that names no file, or a non-digest value under a file key,
/// is still redacted.
#[test]
fn credential_assignments_keep_digests_keyed_by_file_paths() {
    let sha256 = "fdb33b8b86243923b54ae97ca58536e1fba694b759d1bf966fc5f5fb974484b6";
    for (content, path) in [
        (
            format!("   \"packages/bench/compare/bin/with_credentials.mjs\": \"{sha256}\",\n"),
            Some("runs/manifest.json"),
        ),
        (format!("{{\"with_credentials.mjs\": \"{sha256}\"}}"), None),
        (
            format!("  config\\credentials.yaml: \"{}\"\n", &sha256[..40]),
            None,
        ),
        // The same manifest entry as unquoted YAML.
        (
            format!("packages/bench/compare/bin/with_credentials.mjs: {sha256}\n"),
            None,
        ),
    ] {
        let result = detect(&content, path);
        assert_eq!(result.sanitized, content);
        assert!(result.secrets_detected.is_empty(), "{content:?}");
    }
    let value = format!("c7Tvo+hBKqFY{}", fake_b64(28));
    for (content, secret) in [
        (format!("\"credentials\": \"{sha256}\""), sha256),
        (format!("const apiToken = \"{sha256}\";"), sha256),
        (format!("API_TOKEN=\"{}\"", &sha256[..32]), &sha256[..32]),
        (
            format!("\"bin/with_credentials.mjs\": \"{value}\""),
            value.as_str(),
        ),
        (
            format!("\"bin/with_credentials.mjs\": \"{}\"", &sha256[..50]),
            &sha256[..50],
        ),
        // Vault/Consul-style path keys name a secret, not a file: a bare-hex
        // credential (Datadog, Twilio) under them stays redacted.
        (
            format!(
                "\"secret/data/prod/datadog_api_key\": \"{}\"",
                &sha256[..32]
            ),
            &sha256[..32],
        ),
        (
            format!("\"config/myapp/twilio_auth_token\": \"{}\"", &sha256[..40]),
            &sha256[..40],
        ),
        (
            format!("secret/data/prod/datadog_app_key: {}\n", &sha256[..40]),
            &sha256[..40],
        ),
        // An unquoted YAML path key that no provider pattern knows: the
        // credential-named key alone says "credential".
        (
            format!("config/app/api_token: {}\n", &sha256[..40]),
            &sha256[..40],
        ),
    ] {
        let result = detect(&content, None);
        assert!(!result.sanitized.contains(secret), "leaked: {content:?}");
    }
}

/// A password in URL userinfo (an empty or short user too) and the
/// `Password=`/`Pwd=` field of a connection string are redacted in every file
/// class, under any key; the rest of the URL and the other fields stay.
#[test]
fn userinfo_and_connection_string_passwords_are_redacted_in_every_file() {
    for (path, line) in [
        (
            ".env",
            "REDIS_URL=redis://:Moo16@h:6379\nAMQP_URL=amqp://u:Moo17@h/vh\nCONN_STRING=Server=db;User=sa;Password=Hunter2x\nCONN_STRING2=Server=db;Uid=sa;Pwd=ab12\nCONN3=Server=db;User=sa;Password=x\nCONN4=Pwd=ab12;Server=db\n",
        ),
        (
            "src/db.ts",
            "const a = \"redis://:Moo16@h:6379\";\nconst b = \"amqp://u:Moo17@h/vh\";\nconst c = \"Server=db;User=sa;Password=Hunter2x\";\nconst d = \"Server=db;Uid=sa;Pwd=ab12\";\nconst e = \"Server=db;User=sa;Password=x\";\n",
        ),
        (
            "config/db.json",
            "{\"a\": \"redis://:Moo16@h:6379\",\n\"b\": \"amqp://u:Moo17@h/vh\",\n\"c\": \"Server=db;User=sa;Password=Hunter2x\",\n\"d\": \"Server=db;Uid=sa;Pwd=ab12\",\n\"e\": \"Server=db;User=sa;Password=x\"}\n",
        ),
    ] {
        let result = detect(line, Some(path));
        for secret in ["Moo16", "Moo17", "Hunter2x", "ab12", "Password=x"] {
            assert!(
                !result.sanitized.contains(secret),
                "{path}: {secret} leaked: {}",
                result.sanitized
            );
        }
        for kept in [
            "redis://:",
            "@h:6379",
            "amqp://u:",
            "@h/vh",
            "Server=db;User=sa;Password=",
            "Server=db;Uid=sa;Pwd=",
        ] {
            assert!(
                result.sanitized.contains(kept),
                "{path}: {kept} lost: {}",
                result.sanitized
            );
        }
        assert_eq!(result.sanitized.lines().count(), line.lines().count());
    }
}

/// A connection-string field name opens a line or a value, or follows `;`, a
/// quote or whitespace. Minified code assigning a `pwd` or `password` variable after
/// `,`, `(` or `.` stays readable; real connection strings still lose the
/// password, at a line start, quoted, or after other fields.
#[test]
fn connection_string_fields_open_after_a_separator() {
    let readable = [
        ",pwd=formatCwdForFooter(x),branch=y;\n",
        "t=1,password=t.password;username=u\n",
        "{e.password=t.password;username=u}\n",
        "branch&&(pwd=`${pwd} x`);branch=z;\n",
    ];
    let redacted = [
        "Server=x;User Id=u;Password=Pw0rd9q;\n",
        "Data Source=db;Pwd=Pw0rd9q;\n",
        "Password=Pw0rd9q;Server=x\n",
        "const c = \"Server=x;User Id=u;Password=Pw0rd9q\";\n",
        "conn: 'Data Source=db; Pwd=Pw0rd9q'\n",
    ];
    let mut wrong = Vec::new();
    for path in [Some("src/app.js"), Some("Web.config"), None] {
        for content in readable {
            let result = detect(content, path);
            if result.sanitized != content {
                wrong.push(format!("{path:?} redacted: {}", result.sanitized));
            }
        }
        for content in redacted {
            let result = detect(content, path);
            if result.sanitized.contains("Pw0rd9q") {
                wrong.push(format!("{path:?} leaked: {}", result.sanitized));
            }
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
}

/// A URL with credentials loses only its password: scheme, user, host, port
/// and path stay readable, whichever URL pattern (or several) names it, and
/// the password is counted once.
#[test]
fn credentials_in_a_url_redact_only_the_password() {
    let password = "Fake0Pass9Word";
    let urls = [
        "https://admin:{p}@the-internet.herokuapp.com/basic_auth",
        "postgres://app:{p}@db.example.test:5432/orders",
        "postgresql://app:{p}@db.example.test/orders",
        "mysql://app:{p}@db.example.test/orders",
        "mongodb+srv://app:{p}@cluster0.example.test/orders",
        "rediss://app:{p}@cache.example.test:6380",
        "http://elastic:{p}@search.example.test:9200",
        "bolt://neo4j:{p}@graph.example.test:7687",
        "cassandra://app:{p}@ring.example.test:9042",
        "postgres://app:{p}@ep-cool.us-east-2.aws.neon.tech/neondb",
    ];
    for url in urls {
        let line = format!("url = {}\n", url.replace("{p}", password));
        let result = detect(&line, None);
        assert!(
            !result.sanitized.contains(password),
            "{url}: password leaked: {}",
            result.sanitized
        );
        let (head, tail) = url.split_once("{p}").expect("placeholder");
        let kept = result
            .sanitized
            .strip_prefix(&format!("url = {head}"))
            .and_then(|rest| rest.strip_suffix(&format!("{tail}\n")))
            .unwrap_or_else(|| panic!("{url}: context lost: {}", result.sanitized));
        assert!(
            kept.starts_with(crate::security::REDACTION_PREFIX) && !kept.contains(' '),
            "{url}: {kept}"
        );
        assert_eq!(
            result.secrets_detected.len(),
            1,
            "{url}: {:?}",
            result.secrets_detected
        );
    }
}
