//! Credentials no name holds: an argument of a call that signs in, and the
//! last field of a password file's line.

use super::*;

const PASSWORD: &str = "oaySG41!Q5K7Hve9";

/// A call whose callee signs in or connects takes its credentials by
/// position: an opaque string argument is one.
#[test]
fn an_opaque_argument_of_a_sign_in_call_is_hidden() {
    for (content, path) in [
        (
            format!("client.login(\"admin\", \"{PASSWORD}\");\n"),
            "src/app.ts",
        ),
        (
            format!("await auth.signIn(\"admin@example.com\", \"{PASSWORD}\");\n"),
            "src/app.ts",
        ),
        (
            format!("session.sign_in(\"admin\", \"{PASSWORD}\")\n"),
            "src/app.py",
        ),
        (
            format!("conn = psycopg2.connect(\"db.example.com\", \"svc\", \"{PASSWORD}\")\n"),
            "src/db.py",
        ),
        (
            format!("Connection c = DriverManager.getConnection(url, \"svc\", \"{PASSWORD}\");\n"),
            "src/Db.java",
        ),
        (format!("api.authenticate(\"{PASSWORD}\")\n"), "src/app.rb"),
    ] {
        let result = detect(&content, Some(path));
        assert!(!result.sanitized.contains(PASSWORD), "readable: {content}");
        assert!(
            result.sanitized.contains("admin") || !content.contains("admin"),
            "{}",
            result.sanitized
        );
    }
}

/// The verb names no credential: a word, a read or a name after it stays, a
/// call of another callee keeps its arguments, and a value assigned to a name
/// that only spells the verb is not judged by it.
#[test]
fn a_sign_in_verb_names_no_credential() {
    for (content, path) in [
        (
            "client.login(\"admin\", \"password\");\n".to_owned(),
            "src/app.ts",
        ),
        (
            "client.login(\"admin\", process.env.PASSWORD);\n".to_owned(),
            "src/app.ts",
        ),
        (
            "client.login(userName, resourceClaimName);\n".to_owned(),
            "src/app.ts",
        ),
        (
            format!("logger.info(\"login\", \"{PASSWORD}\");\n"),
            "src/app.ts",
        ),
        (format!("const login = \"{PASSWORD}\";\n"), "src/app.ts"),
        (format!("{{\"login\": \"{PASSWORD}\"}}\n"), "conf/a.json"),
        (format!("connectionLabel: {PASSWORD}\n"), "conf/a.yaml"),
    ] {
        let result = detect(&content, Some(path));
        assert_eq!(result.sanitized, content, "{:?}", result.secrets_detected);
    }
    for name in [
        "login",
        "signIn",
        "connect",
        "getConnection",
        "authenticate",
    ] {
        let mut row = serde_json::json!({ name: PASSWORD });
        let removed = crate::security::sanitizer::sanitize_structure(&mut row).expect("scan");
        assert!(removed.is_empty() && row[name] == PASSWORD, "{name}: {row}");
    }
}

/// `host:port:database:user:password` in a file of any name: an opaque last
/// field is the password; a time, an address or words in that shape are not.
#[test]
fn a_password_file_line_hides_its_opaque_last_field() {
    let password = "vyzXP12xNV4AckE4Y9";
    for line in [
        format!("db.example.com:5432:app:app_writer:{password}\n"),
        format!("# hostname:port:database:username:password\n*:*:*:postgres:{password}\n"),
        format!("  10.0.0.7:6432:orders:svc-orders:{password}\r\n"),
    ] {
        let result = detect(&line, Some("notes/pgpass.txt"));
        assert!(!result.sanitized.contains(password), "readable: {line}");
        assert!(result.sanitized.contains(":app_writer:") || !line.contains("app_writer"));
    }
    for line in [
        "localhost:5432:mydb:myuser:mypassword\n",
        "12:30:45:error:connection refused\n",
        "a:b:c:d:e\n",
        "# hostname:port:database:username:password\n",
        "fe80:0:0:0:202b3ff1e8329\n",
        "src/app.ts:14:7:error:unusedVariableName\n",
    ] {
        let result = detect(line, Some("notes/pgpass.txt"));
        assert_eq!(result.sanitized, line, "{:?}", result.secrets_detected);
    }
}
