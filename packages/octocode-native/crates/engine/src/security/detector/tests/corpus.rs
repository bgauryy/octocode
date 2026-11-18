//! Real code that must stay verbatim. Every line runs through the whole
//! pattern list in both value forms and through the request gate, so a new
//! or reshaped pattern that starts to hide ordinary text fails here. Each
//! pattern ships with one case of its own syntax (a positive, in the test of
//! its module) and one line of this corpus.

use super::*;

/// Lines no form hides and no request refuses: Rust paths, minified
/// JavaScript, regex sources that hold armor lines, placeholder keys, SQL
/// binds, YAML anchors and typed fields.
const VERBATIM: &[&str] = &[
    // Rust paths and type names after a label word.
    "self.token == token::OpenParen {",
    "use syn::token::Paren;",
    "let t = password::hash(input);",
    "pub token: Token<'a>,",
    "fn secret::<T>() {}",
    // Minified JavaScript: `!0` and `void 0` are values of no secret.
    "var s={file:!0,password:!0};",
    "var t={type:\"x\",password:!0,token:void 0};",
    "e.exports={secret:!1,apiKey:void 0}",
    "x={token:!!a,password:!1}",
    // Regex sources and string literals that name an armor.
    "const BEGIN = /-----BEGIN RSA PRIVATE KEY-----/;",
    "const re = /-----BEGIN (?:RSA )?PRIVATE KEY-----[\\s\\S]+?-----END (?:RSA )?PRIVATE KEY-----/;",
    "const A = \"-----BEGIN RSA PRIVATE KEY-----\";\nconst B = \"-----END RSA PRIVATE KEY-----\";\n",
    // A placeholder key.
    "-----BEGIN RSA PRIVATE KEY-----\n...\n-----END RSA PRIVATE KEY-----\n",
    "-----BEGIN PRIVATE KEY-----\n<your key here>\n-----END PRIVATE KEY-----\n",
    // SQL binds and one-character placeholders.
    "ALTER USER u PASSWORD :pw;",
    "CREATE USER u IDENTIFIED BY :password;",
    "ALTER ROLE r PASSWORD 'x';",
    "SELECT * FROM t WHERE token = :token AND secret = ?",
    "UPDATE users SET password = $1 WHERE id = $2",
    // YAML anchors, tags and references.
    "password: *default_password",
    "token: &tok",
    "secret: !!str",
    "password: !vault",
    "<<: *credentials",
    // Interpolations and typed fields.
    "password: ${DB_PASSWORD}",
    "password: ${DB_PASSWORD:-}",
    "spring.datasource.password=${MYSQL_PASS}",
    "password: str",
    "password?: string;",
    "password: Option<String>,",
    "password_hash: x",
    "max_tokens: 100",
    // Words that start like a registry token.
    "let cio = cioManagerConfigurationHandlerFactory;",
    "registry = \"ciovalidation\"",
    "see terraform.atlasv1.example for the token format",
    // Query strings that carry a value of no secret.
    "fetch(\"/x?page=2&limit=10\")",
    "fetch(\"/x?token=\" + token)",
    "fetch(`/x?token=${token}`)",
    "https://example.com/?token=$TOKEN&x=1",
    "fetch(\"/x?page_token=Abc123Def456&next_token=Qrs789Tuv012\")",
    "https://example.com/list?nextPageToken=Abc123Def456&sync_token=Qrs789Tuv012",
    // A label, then a mark that opens no literal: a code span, a reference
    // type, a placeholder (lines of this repository's own docs).
    "token: `auth`",
    "token: &str,",
    "TOKEN=<your-token-here>",
    // A quoted label word used as a value: a ternary branch, a comparison, a
    // case label, a list of input types.
    "input.type = secret ? 'password' : 'text';",
    "input.type = secret ? \"password\" : \"text\";",
    "const kind = open ? 'token' : 'secret';",
    "if (kind === 'password') {",
    "case 'password':",
    "const types = ['password', 'text'];",
    "GET /v1/items?api_key=%s HTTP/1.1",
    "url = base + \"?api_key={api_key}\"",
];

/// Lines whose bare value after a label is an identifier or a type in code,
/// and a value in a config file: the code form and the request gate leave
/// them as written.
const VERBATIM_IN_CODE: &[&str] = &[
    "password = userPass1",
    "token = req.headers.authorization",
    "const secret = generateSecret",
    "password: string;",
    "self.password = password_input",
    "brace_token: token::Brace,",
    "pub password: String,",
];

/// The lines of `lines` a form hides or the request gate refuses, each with
/// what took it.
fn offenders(lines: &[&str], forms: &[ValueForm]) -> Vec<String> {
    let mut found = Vec::new();
    for line in lines {
        for &form in forms {
            let spans = secret_spans_in(line, form);
            if !spans.is_empty() {
                found.push(format!("{form:?} hides {line:?}: {spans:?}"));
            }
        }
        if holds_credential_as_request(line) {
            found.push(format!("a request refuses {line:?}"));
        }
    }
    found
}

#[test]
fn ordinary_lines_stay_in_both_forms_and_pass_the_request_gate() {
    let found = offenders(VERBATIM, &[ValueForm::Code, ValueForm::Config]);
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn a_bare_value_after_a_label_stays_in_code_and_in_a_request() {
    let found = offenders(VERBATIM_IN_CODE, &[ValueForm::Code]);
    assert_eq!(found, Vec::<String>::new());
}

/// The crates.io registry token and the Terraform Cloud API token are known
/// formats: hidden wherever they stand, in either form.
#[test]
fn registry_tokens_are_hidden_where_they_stand() {
    let cargo = format!(
        "cio{}",
        "aB3dE6gH9jK2mN5pQ8sT1vW4yZ7bC0eF"
            .chars()
            .take(32)
            .collect::<String>()
    );
    let terraform = format!("{}.atlasv1.{}", "aBcDeFgHiJkLmN", "x7Yz_-".repeat(11));
    for (line, secret) in [
        (format!("CARGO_REGISTRIES_CRATES_IO_TOKEN={cargo}"), &cargo),
        (format!("token = \"{cargo}\""), &cargo),
        (
            format!("{{\"credentials\":{{\"app.terraform.io\":{{\"token\":\"{terraform}\"}}}}}}"),
            &terraform,
        ),
        (format!("TF_TOKEN_app_terraform_io={terraform}"), &terraform),
    ] {
        for form in [ValueForm::Code, ValueForm::Config] {
            let spans = secret_spans_in(&line, form);
            let hidden = replaced(&line, &spans);
            assert!(!hidden.contains(secret.as_str()), "{form:?}: {hidden}");
            assert!(hidden.contains("[REDACTED-"), "{form:?}: {hidden}");
        }
    }
}
