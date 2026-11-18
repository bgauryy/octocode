//! What a read serves of lines the known-format list reads by their file
//! kind: URL query values, config values, interpolation defaults, dotenv
//! backups and multi-line values, and armor lines in a regex source.
use super::*;

/// The text a read of `name` holding `source` serves, under the real scan.
fn served(name: &str, source: &str) -> String {
    let fixture = Fixture::new();
    let path = fixture.write(name, source);
    let paths = Paths(fixture.0.clone());
    let result = fetch_with(
        &q(&path),
        &paths,
        &octocode_runtime_base::security::ContentSecurity::new(),
    );
    assert_eq!(result.error_code, None, "{result:?}");
    result.content.expect("content")
}

#[test]
fn a_url_query_value_is_hidden_in_a_code_file() {
    let out = served("a.ts", "fetch(\"/x?token=abc123xyz&page=2\");\n");
    assert!(!out.contains("abc123xyz"), "{out}");
    assert!(out.contains("?token=") && out.contains("&page=2"), "{out}");
}

#[test]
fn a_paging_token_in_a_query_reads_as_written_and_an_access_token_is_hidden() {
    let source = "const u = `/v1/items?page_token=Abc123Def456&sync_token=Qrs789Tuv012&access_token=Zyx987Wvu654`;\n";
    let out = served("a.ts", source);
    assert!(
        out.contains("page_token=Abc123Def456") && out.contains("sync_token=Qrs789Tuv012"),
        "{out}"
    );
    assert!(!out.contains("Zyx987Wvu654"), "{out}");
}

#[test]
fn a_quoted_label_in_a_ternary_branch_reads_as_written() {
    let source = "export function editor(secret, input) {\n  input.type = secret ? 'password' : 'text';\n}\n";
    assert_eq!(served("a.js", source), source);
}

#[test]
fn rust_paths_and_minified_values_read_as_written() {
    let rust = "if self.token == token::OpenParen {\n    brace_token: token::Brace,\n}\n";
    assert_eq!(served("a.rs", rust), rust);
    let minified = "var s={file:!0,password:!0};var t={type:\"x\",password:!0,token:void 0};\n";
    assert_eq!(served("a.js", minified), minified);
}

#[test]
fn config_values_are_hidden_and_code_shows_the_same_lines() {
    let lines = "password: letmein\npassword: hunter2\npassword: userPass12\n";
    let yaml = served("a.yaml", lines);
    for value in ["letmein", "hunter2", "userPass12"] {
        assert!(!yaml.contains(value), "{yaml}");
    }
    assert_eq!(yaml.lines().count(), 3, "{yaml}");
    assert!(
        yaml.lines()
            .all(|line| line.starts_with("password: [REDACTED-")),
        "{yaml}"
    );
    assert_eq!(served("a.rs", lines), lines);
}

#[test]
fn an_interpolation_default_is_hidden_and_its_variable_reads_as_written() {
    let out = served(
        "application.properties",
        "spring.datasource.password=${MYSQL_PASS:petclinic}\nother.password=${X:changeme}\n",
    );
    assert!(!out.contains("petclinic"), "{out}");
    assert!(out.contains("password=${MYSQL_PASS:[REDACTED-"), "{out}");
    assert!(out.contains("other.password=${X:changeme}"), "{out}");
}

#[test]
fn a_backup_of_a_dotenv_file_hides_its_values_like_the_file() {
    for name in ["config.env.bak", ".envrc.old", ".env.local.orig", ".env~"] {
        let out = served(name, "export DB_PASS=s3cretValue77\nMODE=dev\n");
        assert!(!out.contains("s3cretValue77"), "{name}: {out}");
        assert_eq!(out.lines().count(), 2, "{name}: {out}");
        assert!(
            out.contains("DB_PASS=[REDACTED-DOTENVVALUE]"),
            "{name}: {out}"
        );
    }
    let yaml = served("config.yaml.bak", "password: hunter2x\n");
    assert!(!yaml.contains("hunter2x"), "{yaml}");
}

#[test]
fn a_quoted_dotenv_value_that_spans_lines_is_hidden_whole() {
    let out = served(".env", "A=1\nMULTI=\"line1\nsecretline2tail\"\nB=2\n");
    assert!(
        !out.contains("line1") && !out.contains("secretline2tail"),
        "{out}"
    );
    assert_eq!(out.lines().count(), 4, "{out}");
    assert!(
        out.starts_with("A=[REDACTED-DOTENVVALUE]\nMULTI=[REDACTED-DOTENVVALUE]\n"),
        "{out}"
    );
    assert!(out.ends_with("\nB=[REDACTED-DOTENVVALUE]\n"), "{out}");
    let single = served(".env", "K='a\nsecret2\n'\n");
    assert!(!single.contains("secret2"), "{single}");
    assert_eq!(single.lines().count(), 3, "{single}");
    // An opening quote nothing closes hides the line it opens and no more.
    let open = served(".env", "K=\"never closed\nNEXT=ok\n");
    assert!(!open.contains("never closed"), "{open}");
}

#[test]
fn armor_lines_in_a_regex_source_enclose_no_key() {
    let source = "const re = /-----BEGIN (?:RSA )?PRIVATE KEY-----[\\s\\S]+?-----END (?:RSA )?PRIVATE KEY-----/;\nconst head = /-----BEGIN RSA PRIVATE KEY-----/;\nconst tail = /-----END RSA PRIVATE KEY-----/;\n";
    assert_eq!(served("keys.js", source), source);
}
