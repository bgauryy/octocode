//! Values under a credential-like name that say where a credential is kept,
//! how it is addressed or what it looks like, and hold none.

use super::*;

/// What a generator makes: hidden under every name that can hold a value.
const OPAQUE: &str = "Zq7xLm2Pz9Kd4Wr8Tn3Vb6Hc";

fn kept(content: &str, path: &str) -> bool {
    detect(content, Some(path)).sanitized == content
}

/// A reference, a locator, a template, a specifier or a format keyword under
/// a credential-like name stays readable.
#[test]
fn references_specifiers_and_descriptions_stay_readable() {
    let hex = "04".to_owned() + &"1098496bbf7456d40e4aed363f3a400f".repeat(4);
    let public = format!("export const publicKey = \"{hex}\";\n");
    let mut wrong = Vec::new();
    for (content, path) in [
        (
            "  \"secret\": \"auth.errors.secret_missing\",\n",
            "i18n/en.json",
        ),
        (
            "{\"password\": \"errors.password.required\"}\n",
            "i18n/en.json",
        ),
        (
            "const secretName = \"projects/demo/secrets/db-password/versions/latest\";\n",
            "src/labels.ts",
        ),
        (
            "const cacheKey = \"user:profile:{id}:settings\";\n",
            "src/labels.ts",
        ),
        (
            "const key = \"tenant:{tenantId}:user:{userId}\";\n",
            "src/labels.ts",
        ),
        ("token: vault:secret/data/app#token\n", "conf/he.yaml"),
        ("password: vault:secret/data/app#password\n", "conf/he.yaml"),
        ("{\"token\": \"{{ vault_token }}\"}\n", "conf/x.json"),
        ("SECRET_NAME=prod/app/database\n", "prod.env"),
        ("SECRET_NAME=prod/app/database\n", "notes/prod.env.txt"),
        (public.as_str(), "src/misc.ts"),
        (
            "<input type=\"password\" name=\"current-password\">\n",
            "web/index.html",
        ),
        (
            "<input type=\"password\" name=\"new-password\" autocomplete=\"new-password\">\n",
            "web/index.html",
        ),
        (
            "{\n  \"dependencies\": {\n    \"secret-store\": \"workspace:^\",\n    \"my-token-lib\": \"github:acme/token-lib#v2\",\n    \"api-key-kit\": \"npm:@acme/api-key-kit@1.2.3\",\n    \"auth-token-x\": \"file:../auth-token-x\",\n    \"token-range\": \">=1.0.0 <2.0.0\"\n  },\n  \"scripts\": { \"token\": \"node scripts/mint-token.js --dry\" }\n}\n",
            "package.json",
        ),
        (
            "    \"@aws-sdk/token-providers\": \"npm:3.1127.0\"\n    \"@aws-sdk/types\": \"npm:^3.974.5\"\n",
            "yarn.lock",
        ),
        (
            "    \"@aws-sdk/credential-provider-node\": \"npm:^3.972.39\"\n    \"@smithy/core\": \"npm:^3.33.3\"\n",
            "yarn.lock",
        ),
        (
            "const API_KEY_HEADER = \"X-Custom-Api-Key-Header\";\n",
            "src/c.ts",
        ),
        (
            "const tokenPattern = \"^[A-Za-z0-9_-]{20,}$\";\n",
            "src/c.ts",
        ),
        ("const keyAlgorithm = \"RSASSA-PKCS1-v1_5\";\n", "src/c.ts"),
        (
            "const kmsKeyId = \"arn:aws:kms:us-east-1:123456789012:key/1234abcd-12ab-34cd-56ef-1234567890ab\";\n",
            "src/c.ts",
        ),
        (
            "const kmsKeyId = \"1234abcd-12ab-34cd-56ef-1234567890ab\";\n",
            "src/c.ts",
        ),
        (
            "const passwordLabel = \"Enter your password\";\n",
            "src/c.ts",
        ),
        (
            "const passwordField = \"#login-password-input\";\n",
            "src/c.ts",
        ),
        ("const tokenType = \"Bearer\";\n", "src/c.ts"),
    ] {
        if !kept(content, path) {
            wrong.push(format!(
                "hidden in {path}: {}",
                detect(content, Some(path)).sanitized
            ));
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
}

/// The same names with a generated value: hidden. A name that describes a
/// credential holds one when its value is opaque, and a public name does not
/// make a private key public.
#[test]
fn an_opaque_value_under_the_same_names_is_hidden() {
    let pem = "-----BEGIN RSA PRIVATE KEY-----\\nMIIEpAIBAAKCAQEA7Zq7xLm2Pz9Kd4Wr8Tn3Vb6HcZq7xLm2Pz9Kd4Wr8Tn3Vb6Hc\\n-----END RSA PRIVATE KEY-----";
    let mut wrong = Vec::new();
    for (content, path, secret) in [
        (
            format!("  \"secret\": \"{OPAQUE}\",\n"),
            "i18n/en.json",
            OPAQUE,
        ),
        (
            format!("const secretName = \"{OPAQUE}\";\n"),
            "src/labels.ts",
            OPAQUE,
        ),
        (
            format!("const cacheKey = \"{OPAQUE}\";\n"),
            "src/labels.ts",
            OPAQUE,
        ),
        (format!("token: {OPAQUE}\n"), "conf/he.yaml", OPAQUE),
        (format!("token: vault:{OPAQUE}\n"), "conf/he.yaml", OPAQUE),
        (format!("SECRET_NAME={OPAQUE}\n"), "prod.env", OPAQUE),
        (
            format!("SECRET_NAME={OPAQUE}\n"),
            "notes/prod.env.txt",
            OPAQUE,
        ),
        (
            format!("export const publicKey = \"{pem}\";\n"),
            "src/misc.ts",
            "MIIEpAIBAAKCAQEA7Zq7x",
        ),
        (
            format!("<input type=\"password\" value=\"{OPAQUE}\">\n"),
            "web/index.html",
            OPAQUE,
        ),
        (
            format!("{{\n  \"dependencies\": {{\n    \"secret-store\": \"{OPAQUE}\"\n  }}\n}}\n"),
            "package.json",
            OPAQUE,
        ),
        (
            format!(
                "    \"@aws-sdk/token-providers\": \"{OPAQUE}\"\n    \"@aws-sdk/types\": \"npm:^3.974.5\"\n"
            ),
            "yarn.lock",
            OPAQUE,
        ),
        (
            format!("const API_KEY_HEADER = \"{OPAQUE}\";\n"),
            "src/c.ts",
            OPAQUE,
        ),
        (
            format!("const tokenPattern = \"{OPAQUE}\";\n"),
            "src/c.ts",
            OPAQUE,
        ),
        (
            format!("const keyAlgorithm = \"{OPAQUE}\";\n"),
            "src/c.ts",
            OPAQUE,
        ),
        (
            format!("const kmsKeyId = \"{OPAQUE}\";\n"),
            "src/c.ts",
            OPAQUE,
        ),
        (
            "token: admin:opensesame\n".to_owned(),
            "conf/x.yaml",
            "opensesame",
        ),
        (
            "const password = \"admin:opensesame\";\n".to_owned(),
            "src/x.ts",
            "opensesame",
        ),
        (
            "{\"password\": \"correct.horse.battery\"}\n".to_owned(),
            "conf/x.json",
            "correct.horse.battery",
        ),
    ] {
        let result = detect(&content, Some(path));
        if result.sanitized.contains(secret) {
            wrong.push(format!("readable in {path}: {content}"));
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
}

/// Pieces written side by side are one literal; a quoted key on the next
/// line is the next entry, and its value is judged on its own.
#[test]
fn the_next_entry_s_key_is_no_piece_of_a_value() {
    let content = format!(
        "    \"@aws-sdk/token-providers\": \"npm:3.1127.0\"\n    \"api-token\": \"{OPAQUE}\"\n"
    );
    let result = detect(&content, Some("yarn.lock"));
    assert!(!result.sanitized.contains(OPAQUE), "{}", result.sanitized);
    assert!(
        result.sanitized.contains("npm:3.1127.0"),
        "{}",
        result.sanitized
    );
    let joined = "token = \"Fake0Zq7x\"\n  \"Moo9ZedLm2Pz9Kd4\"\n";
    let result = detect(joined, Some("src/a.py"));
    assert!(
        !result.sanitized.contains("Moo9ZedLm2Pz9Kd4"),
        "{}",
        result.sanitized
    );
}
