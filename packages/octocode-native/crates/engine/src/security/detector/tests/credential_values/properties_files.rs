//! Credential assignments in Java `.properties` files (B125): a config file
//! whose credential-named key holds a credential at any length, as in a
//! dotenv file. Every value is fake.

use super::*;

/// Paths of the properties class: any `*.properties` name, any case, with an
/// optional backup suffix.
const PROPERTIES_PATHS: &[&str] = &[
    "app.properties",
    "conf/db.properties",
    "src/main/resources/prod.properties",
    "deploy\\APP.Properties",
    "app.properties.bak",
];

/// Short and plain-word values under credential keys, each with the text it
/// must keep before the value and the value it must hide.
const SHORT_CREDENTIALS: &[(&str, &str, &str)] = &[
    ("db.password=fakeword\n", "db.password=", "fakeword"),
    (
        "spring.datasource.password=Fake0Moo\n",
        "spring.datasource.password=",
        "Fake0Moo",
    ),
    ("app.api_key=abc123\n", "app.api_key=", "abc123"),
    ("db.password = pw\n", "db.password = ", "pw"),
    ("db.password: CowSaysMoo\n", "db.password: ", "CowSaysMoo"),
    ("db.password:fakeword\n", "db.password:", "fakeword"),
    ("jwt.secret=moo\n", "jwt.secret=", "moo"),
    (
        "client.token=cow says moo\n",
        "client.token=",
        "cow says moo",
    ),
    ("db.password=fakeword # dev\n", "db.password=", "fakeword"),
    ("db.password=fakeword\r\n", "db.password=", "fakeword"),
    ("  db.password=fakeword", "  db.password=", "fakeword"),
];

/// In a properties file a credential name says the value is one, whatever
/// its length: the value goes, the key, the separator and the lines stay.
#[test]
fn properties_files_redact_short_values_under_credential_keys() {
    for path in PROPERTIES_PATHS {
        for (content, kept, secret) in SHORT_CREDENTIALS {
            let result = detect(content, Some(path));
            assert!(
                result.sanitized.starts_with(kept) && !result.sanitized.contains(secret),
                "{path}: {content:?} -> {:?}",
                result.sanitized
            );
            assert_eq!(result.sanitized.lines().count(), content.lines().count());
            assert_no_pattern_matches(&result);
            let again = detect(&result.sanitized, Some(path));
            assert_eq!(again.sanitized, result.sanitized, "redaction is stable");
        }
    }
    let file = "db.url=jdbc:postgresql://h/db\ndb.password=fakeword\nspring.datasource.password=Fake0Moo\napi.secret=correcthorsebattery\n";
    let result = detect(file, Some(".octocode/tmp/b125/app.properties"));
    assert!(
        result
            .sanitized
            .starts_with("db.url=jdbc:postgresql://h/db\ndb.password=[REDACTED-")
    );
    for secret in ["fakeword", "Fake0Moo", "correcthorsebattery"] {
        assert!(!result.sanitized.contains(secret), "{secret}");
    }
}

/// A properties file keeps what is no credential: empty values, references,
/// the tool's own marker, a locator, comments, and the settings that qualify
/// a credential (`password.min.length=8`: the credential word is not the
/// key's last dotted segment).
#[test]
fn properties_files_keep_settings_references_and_empty_values() {
    for content in [
        "db.password=\n",
        "db.password =\n",
        "db.password:\n",
        "db.password=${DB_PASSWORD}\n",
        "db.password=$DB_PASSWORD\n",
        "db.password=[REDACTED-PROPERTIESFILECREDENTIAL]\n",
        "db.password.file=/run/secrets/db\n",
        "db.password.url=https://vault.example.test/db\n",
        "password.min.length=8\n",
        "password.reset.token.ttl=3600\n",
        "security.password.enabled=true\n",
        "db.url=jdbc:postgresql://h/db\n",
    ] {
        for path in PROPERTIES_PATHS {
            let result = detect(content, Some(path));
            assert_eq!(result.sanitized, content, "{path}");
            assert!(result.secrets_detected.is_empty(), "{path}: {content:?}");
        }
    }
}

/// Outside the properties and dotenv classes a short value is too like any
/// setting to call a credential: the length floor stays, whatever the path.
#[test]
fn outside_properties_files_short_values_keep_the_length_floor() {
    for path in [
        Some("src/Config.java"),
        Some("src/lib.rs"),
        Some("conf/app.conf"),
        Some("conf/settings.yml"),
        Some("app.properties.example"),
        Some("properties"),
        None,
    ] {
        for (content, _, _) in SHORT_CREDENTIALS {
            let result = detect(content, path);
            assert_eq!(result.sanitized, *content, "{path:?}");
        }
    }
}

/// A properties file is no credential file by name: reads and listings still
/// show it, only its credential values go.
#[test]
fn properties_names_are_not_dotenv_names() {
    for path in PROPERTIES_PATHS {
        assert!(!crate::security::is_dotenv_name(path), "{path}");
    }
}

/// A value that continues on the next line (`\` at a line's end) is one
/// value: no piece of it stays.
#[test]
fn properties_continuation_lines_are_one_value() {
    let content = "db.password=fake\\\n    word9\\\n    tail\napp.name=demo\n";
    let result = detect(content, Some("app.properties"));
    for piece in ["fake", "word9", "tail"] {
        assert!(!result.sanitized.contains(piece), "{:?}", result.sanitized);
    }
    assert!(result.sanitized.ends_with("\napp.name=demo\n"));
    assert_eq!(
        result.sanitized.matches('\n').count(),
        content.matches('\n').count()
    );
}

/// Redacts each of `secrets` in `content` read as `scope`, keeps its lines,
/// and redacts nothing more on a second pass.
fn hides_value(content: &str, scope: Scope<'_>, secrets: &[&str]) {
    let result = detect_in(content, scope);
    for secret in secrets {
        assert!(
            !result.sanitized.contains(secret),
            "{scope:?}: {secret:?} leaked: {content:?} -> {:?}",
            result.sanitized
        );
    }
    assert_eq!(
        result.sanitized.matches('\n').count(),
        content.matches('\n').count(),
        "{scope:?}: {content:?} -> {:?}",
        result.sanitized
    );
    let again = detect_in(&result.sanitized, scope);
    assert_eq!(again.sanitized, result.sanitized, "redaction is stable");
}

/// Reads `content` as `scope` and keeps it byte for byte.
fn keeps_value(content: &str, scope: Scope<'_>) {
    let result = detect_in(content, scope);
    assert_eq!(result.sanitized, content, "{scope:?}");
    assert!(result.secrets_detected.is_empty(), "{scope:?}: {content:?}");
}

/// Keys that put a credential word before their last dotted segment
/// (rotation, environment and qualifier suffixes, list positions), each
/// with a short value that is no setting: hidden in a properties file, in a
/// dotted dotenv key and in stored text.
const QUALIFIED_KEYS: &[(&str, &str)] = &[
    ("db.password.value=fakeword2\n", "fakeword2"),
    (
        "spring.datasource.password.encrypted=fakeword3\n",
        "fakeword3",
    ),
    ("jdbc.password.prod=fakeword4\n", "fakeword4"),
    ("api.token.value=fakeword7\n", "fakeword7"),
    ("PASSWORD.0=fakeword8\n", "fakeword8"),
    ("password.1=fakeword9\n", "fakeword9"),
    ("db.password.old=fakewordC\n", "fakewordC"),
    ("oauth.client.secret.value=fakewordD\n", "fakewordD"),
    ("password.hash.salt=fakewordE\n", "fakewordE"),
    ("DB.PASSWORD.VALUE=fakewordF\n", "fakewordF"),
];

/// A credential word anywhere in a dotted key marks a credential unless the
/// value has a setting's shape.
#[test]
fn qualified_credential_keys_hide_values_that_are_no_settings() {
    for (content, secret) in QUALIFIED_KEYS {
        for scope in [
            Scope::File("app.properties"),
            Scope::File("conf/.env"),
            Scope::Stored(None),
        ] {
            hides_value(content, scope, &[secret]);
        }
    }
}

/// A setting that qualifies a credential (a length, a lifetime, a switch)
/// keeps its empty, numeric, boolean or referenced value.
#[test]
fn qualified_credential_keys_keep_setting_values() {
    for content in [
        "password.min.length=8\n",
        "password.reset.token.ttl=3600\n",
        "security.password.enabled=true\n",
        "security.password.enabled = FALSE\n",
        "password.expiry.days=0.5\n",
        "password.history.size=\n",
        "password.encoder.strength=${app.encoder.strength}\n",
        "password.min.length=$MIN_LENGTH\n",
    ] {
        for scope in [Scope::File("app.properties"), Scope::File("conf/.env")] {
            keeps_value(content, scope);
        }
    }
}

/// Indexed keys (`users[0].password`, `keys[1]`) are credential keys too.
#[test]
fn indexed_keys_are_credential_keys() {
    for (content, secret) in [
        ("spring.security.users[0].password=fakeword1\n", "fakeword1"),
        ("app.keys[0]=fakeword2\n", "fakeword2"),
        ("app.users[12].token=fakeword3\n", "fakeword3"),
        ("app.passwords[1].value=fakeword4\n", "fakeword4"),
    ] {
        hides_value(content, Scope::File("app.properties"), &[secret]);
        keeps_value(content, Scope::File("src/Config.java"));
    }
}

/// `.properties` has no inline comments: a ` #` tail, on the first line or
/// a continuation line, is part of the value and goes with it. A dotenv file
/// keeps its comment.
#[test]
fn properties_inline_hash_is_part_of_the_value() {
    hides_value(
        "db.password=fakeword C #D\nnext=1\n",
        Scope::File("app.properties"),
        &["fakeword", "#D"],
    );
    hides_value(
        "db.password=fake\\\n  # not a comment more\nnext=1\n",
        Scope::File("app.properties"),
        &["fake", "not a comment", "more"],
    );
    hides_value(
        "db.password= # all value\n",
        Scope::File("app.properties"),
        &["all value"],
    );
    let result = detect("DB_PASS=fakeword # dev\n", Some(".env"));
    assert!(
        result.sanitized.ends_with(" # dev\n"),
        "{:?}",
        result.sanitized
    );
}

/// A reference without a default only names another key; a default may be
/// the secret, so a reference with one is redacted.
#[test]
fn properties_references_keep_only_defaultless_names() {
    for content in [
        "db.password=${app.db.password}\n",
        "db.password=${DB_PASSWORD}\n",
    ] {
        keeps_value(content, Scope::File("app.properties"));
    }
    for (content, secret) in [
        ("db.password=${DB_PASSWORD:}\n", "DB_PASSWORD"),
        ("db.password=${DB_PASSWORD:fakeword}\n", "fakeword"),
        ("db.password=${app.db.password}fakeword\n", "fakeword"),
    ] {
        hides_value(content, Scope::File("app.properties"), &[secret]);
    }
}

/// Accepted over-redaction: any credential-word leaf holds a credential in a
/// properties file, whatever its value reads like.
#[test]
fn credential_word_leaves_are_credentials_in_properties_files() {
    for (content, secret) in [
        ("login.password=Password\n", "Password"),
        ("db.password=true\n", "true"),
        (
            "spring.kafka.consumer.key-serializer=StringSerializer\n",
            "StringSerializer",
        ),
    ] {
        hides_value(content, Scope::File("app.properties"), &[secret]);
    }
}

/// The two file classes are told apart by their own rule, so a marker names
/// the class that redacted.
#[test]
fn each_file_class_names_its_own_rule() {
    let properties = detect("db.password=fakeword\n", Some("app.properties"));
    assert_eq!(properties.secrets_detected, ["propertiesFileCredential"]);
    let dotenv = detect("DB_PASS=fakeword\n", Some(".env"));
    assert_eq!(dotenv.secrets_detected, ["dotenvFileCredential"]);
}

/// A default in a `${NAME<operator>default}` reference may be the secret:
/// shell and compose spell it with `-`, `:-`, `:`, `+`, `=` or `?`, so only a
/// bare name stays readable, in a properties file and in a dotenv file.
#[test]
fn reference_defaults_are_redacted_whatever_their_operator() {
    for operator in ["-", ":-", ":", "+", ":+", "=", ":=", "?", ":?"] {
        let value = format!("${{DB_PASS{operator}fakeword}}");
        for (content, scope) in [
            (
                format!("db.password={value}\n"),
                Scope::File("app.properties"),
            ),
            (format!("DB_PASS={value}\n"), Scope::File("conf/.env")),
            (format!("DB_PASS={value}\n"), Scope::Stored(None)),
        ] {
            hides_value(&content, scope, &["fakeword"]);
        }
    }
    for content in ["DB_PASS=${OTHER_PASS}\n", "DB_PASS=$OTHER_PASS\n"] {
        keeps_value(content, Scope::File("conf/.env"));
    }
    keeps_value("DB_PASS=${app.db.password}\n", Scope::File("conf/.env"));
}

/// First lines of a continued value that a generic rule redacts on its own
/// (12+ characters, a vendor token, a Spring Boot key), with the path that
/// puts the rule in play.
const CONTINUED_FIRST_LINES: &[(&str, &str, &str)] = &[
    (
        "app.properties",
        "db.password=fakelongword123456789",
        "fakelongword123456789",
    ),
    (
        "app.properties",
        "api.secret=fakelongwordfakelongword",
        "fakelongwordfakelongword",
    ),
    (
        "app.properties",
        "git.token=ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "ghp_aaaaaaaa",
    ),
    ("app.properties", "db.password=fakeword9", "fakeword9"),
    (
        "application.properties",
        "spring.datasource.password=fake",
        "fake",
    ),
    (
        "application-prod.properties",
        "spring.security.oauth2.client.registration.x.client-secret=fakelongword123",
        "fakelongword123",
    ),
];

/// Whatever rule redacts the first line of a continued value, the lines it
/// continues on are hidden with it: the value is one value, the lines are
/// kept, and a second pass changes nothing.
#[test]
fn continued_values_are_hidden_whichever_rule_takes_the_first_line() {
    for (path, first, secret) in CONTINUED_FIRST_LINES {
        for lines in [
            ["   part2tailA", "next=1"].as_slice(),
            ["   part2tailA\\", "   part3tailB", "next=1"].as_slice(),
        ] {
            let content = format!("{first}\\\n{}\n", lines.join("\n"));
            hides_value(
                &content,
                Scope::File(path),
                &[secret, "part2tailA", "part3tailB"],
            );
            let result = detect(&content, Some(path));
            assert!(
                result.sanitized.ends_with("\nnext=1\n"),
                "{path}: {content:?} -> {:?}",
                result.sanitized
            );
        }
    }
}

/// The properties rule reads the whole of a continued value, so it runs
/// before every generic rule: one that redacts the first line would take the
/// line's trailing `\` with it and leave the tail readable.
#[test]
fn properties_rule_leads_the_registry() {
    let names: Vec<_> = crate::security::patterns::PATTERNS
        .iter()
        .map(|pattern| pattern.name)
        .collect();
    assert_eq!(names.first(), Some(&"propertiesFileCredential"));
    assert_eq!(
        names
            .iter()
            .filter(|name| **name == "propertiesFileCredential")
            .count(),
        1
    );
}

/// A `key: value` line with a credential key is a config entry whatever path
/// the text came from: a stored copy must hold nothing a read of the same file
/// would hide, and no path says a line is a declaration, so none is judged
/// one. Code and prose paths hide it like the rest.
#[test]
fn colon_entries_are_hidden_under_every_path() {
    for (content, secret) in [
        ("password: hunter2xyz\n", "hunter2xyz"),
        ("    password: hunter2xyz\n", "hunter2xyz"),
        ("secret: abc123\n", "abc123"),
        ("DB_PASSWORD: hunter2xyz\n", "hunter2xyz"),
        ("password : hunter2xyz\n", "hunter2xyz"),
        ("password:=hunter2xyz\n", "hunter2xyz"),
        ("password: correct horse battery\n", "correct"),
        ("password: hunter2xyz\\\n  tailpart\n", "tailpart"),
        ("db.password: fakeword\n", "fakeword"),
        ("app.users[0].token: fakeword\n", "fakeword"),
        (
            "spring:\n  datasource:\n    password: hunter2xyz\n",
            "hunter2xyz",
        ),
    ] {
        for scope in [
            Scope::Stored(None),
            Scope::Stored(Some("x.rs")),
            Scope::Stored(Some("src/model.ts")),
            Scope::Stored(Some("notes/answer.md")),
            Scope::Stored(Some("notes/answer.MD")),
            Scope::Stored(Some("app.yaml")),
            Scope::Stored(Some(".env")),
            Scope::Stored(Some("conf/app.properties")),
            Scope::File("app.properties"),
        ] {
            hides_value(content, scope, &[secret]);
        }
    }
}

/// A text past the in-memory limit is judged in one pass over its spans, and
/// hides, keeps and ends its lines as the in-memory pass does, whatever its
/// line ends: LF, CRLF or a lone CR.
#[test]
fn long_texts_hide_continued_values_as_short_ones_do() {
    for (path, first, secret) in CONTINUED_FIRST_LINES {
        for eol in ["\n", "\r\n", "\r"] {
            let content =
                format!("{first}\\{eol}   part2tailA\\{eol}   part3tailB{eol}next=1{eol}");
            let scope = Scope::File(path);
            let in_memory = detect_in(&content, scope);
            let long = redact_within(&content, scope, 1).expect("a secret is hidden");
            let tails: &[&str] = if eol == "\r" {
                &[]
            } else {
                &["part2tailA", "part3tailB"]
            };
            for piece in std::iter::once(secret).chain(tails) {
                assert!(
                    !long.sanitized.contains(piece),
                    "{path} {eol:?}: {:?}",
                    long.sanitized
                );
            }
            assert_eq!(long.sanitized, in_memory.sanitized, "{path} {eol:?}");
            let again = redact_within(&long.sanitized, scope, 1);
            assert!(
                again
                    .as_ref()
                    .is_none_or(|again| again.sanitized == long.sanitized),
                "{path} {eol:?}: {:?} -> {:?}",
                long.sanitized,
                again.map(|again| again.sanitized)
            );
        }
    }
}

/// Spring Boot's own keys in `application*` files, in every form the pattern
/// names (bare, quoted, YAML, `:` or `=`): the redaction marker it writes is
/// no value it would redact again (the shared placeholder skip, whatever the
/// pattern's regex spells), so a second pass changes nothing and finds
/// nothing in the first pass's output (B142).
#[test]
fn spring_boot_redaction_is_a_fixed_point_in_application_files() {
    let lines = [
        "spring.datasource.password=Fake0Moo\n",
        "spring.datasource.password = fakeword\n",
        "spring.datasource.password: \"Fake0Moo\"\n",
        "spring.datasource.password='Fake0Moo'\n",
        "spring.security.oauth2.client.registration.x.client-secret=fakelongword123\n",
        "spring.security.oauth2.client.registration.x.client-secret: Fake0Value9Not4Real\n",
    ];
    for path in [
        "application.properties",
        "application-prod.properties",
        "bootstrap.yml",
        "config/application-dev.yaml",
    ] {
        for line in lines {
            let first = detect(line, Some(path));
            assert!(first.sanitized != line, "{path}: {line:?} stays readable");
            for scope in [
                Scope::File(path),
                Scope::Leaf(path),
                Scope::Stored(Some(path)),
                Scope::Stored(None),
                Scope::Pathless,
            ] {
                let second = detect_in(&first.sanitized, scope);
                assert_eq!(
                    second.sanitized, first.sanitized,
                    "{path} {scope:?}: {line:?}"
                );
                assert!(
                    second.secrets_detected.is_empty(),
                    "{path} {scope:?}: {line:?} found {:?} in its own output",
                    second.secrets_detected
                );
            }
        }
    }
}
