//! Where a value starts and ends, and which values a request and a file
//! judge alike: a literal ends at its closing quote, an expression's
//! literals are each a value, a placed value's placeholder is none, and a
//! plain scalar ends with its token. Each rule hides the secret and keeps
//! the text before and after it, runs its look-alike, and a request refuses
//! what a file hides.

use super::*;

/// A generated key of a literal's full length and more.
const KEY: &str = "Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2Ua4";
/// A generated password with symbols.
const PASSWORD: &str = "Tr0ub4dor&3xK9!zQ";
/// A GitHub token's shape.
const GITHUB: &str = "ghp_x7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoI";

/// That `content` loses each of `secrets` and nothing else in its file and
/// in the pathless net, and that a request holding it is refused.
fn hidden_alone(content: &str, path: &str, secrets: &[&str]) {
    for path in [Some(path), None] {
        assert_redacts_each(content, path, secrets);
    }
    assert!(holds_credential_as_request(content), "ran {content:?}");
}

/// That `content` stays as written in its file and in the pathless net, and
/// runs as a request.
fn shown_and_run(content: &str, path: &str) {
    for path in [Some(path), None] {
        assert_eq!(detect(content, path).sanitized, content, "{path:?}");
    }
    assert!(!holds_credential_as_request(content), "refused {content:?}");
}

/// D5: a literal in the expression a credential's name is assigned (a
/// ternary's arm, a fallback's operand, a template's text) is judged as the
/// value the name holds: an opaque one goes, alone, and a readable one, the
/// operators and the code around them stay.
#[test]
fn each_literal_of_an_assigned_expression_is_judged_as_the_named_value() {
    for (content, path) in [
        (
            format!("const apiKey = cond ? \"{KEY}\" : \"otherVisible3\";\n"),
            "src/a.ts",
        ),
        (
            format!("const apiKey = cond ? \"otherVisible3\" : \"{KEY}\";\n"),
            "src/a.ts",
        ),
        (
            format!("const apiKey = isProductionEnvironment ? \"{KEY}\" : \"otherVisible3\";\n"),
            "src/a.ts",
        ),
        (
            format!("const authToken = process.env.AUTH_TOKEN || \"{KEY}\";\n"),
            "src/a.ts",
        ),
        (
            format!("const secretKey = process.env.X ?? \"{KEY}\";\n"),
            "src/a.ts",
        ),
        (
            format!("api_key = \"otherVisible3\" if dev else \"{KEY}\"\n"),
            "src/a.py",
        ),
        (format!("const apiKey2 = `{KEY}${{suffix}}`;\n"), "src/a.ts"),
    ] {
        hidden_alone(&content, path, &[KEY]);
    }
    for (content, path) in [
        (
            "const apiKey = cond ? \"otherVisible3\" : \"fallbackValue\";\n",
            "src/a.ts",
        ),
        (
            "const token = cond ? readToken() : refreshToken();\n",
            "src/a.ts",
        ),
        ("const apiKey = `key-${id}`;\n", "src/a.ts"),
    ] {
        shown_and_run(content, path);
    }
}

/// D4: a literal's value ends at its closing quote. A piece joined after a
/// credential (`+ "tail"`, `+ name`) is the program's text and stays; two
/// pieces each a credential alone are each hidden; pieces no one of which is
/// a credential alone are judged joined, as one value.
#[test]
fn a_literal_value_ends_at_its_closing_quote() {
    for (content, path) in [
        (
            format!("const apiKey = \"{KEY}\" + \"tail-visible1\";\n"),
            "src/a.ts",
        ),
        (
            format!("const token = process.env.X || \"{KEY}\" + \"visible2\";\n"),
            "src/a.ts",
        ),
        (
            format!("api_key = \"{KEY}\" + suffix_visible5\n"),
            "src/a.py",
        ),
    ] {
        hidden_alone(&content, path, &[KEY]);
    }
    let (head, tail) = KEY.split_at(20);
    hidden_alone(
        &format!("const apiKey = \"{head}\" + \"{tail}\";\n"),
        "src/a.ts",
        &[head, tail],
    );
    let joined = detect(
        "const password = \"Fake0\" + \"Moo9Zed\";\n",
        Some("src/a.ts"),
    );
    assert!(
        !joined.sanitized.contains("Fake0") && !joined.sanitized.contains("Moo9Zed"),
        "{:?}",
        joined.sanitized
    );
}

/// D1: the value a pattern finds where a credential stands (the token after
/// `Bearer`, a URL's password) is judged once, the same in a request and in
/// a file: any value is a credential, letters alone too, and a placeholder a
/// document writes in its place is none.
#[test]
fn a_placed_value_is_judged_alike_in_a_request_and_a_file() {
    for (content, secret) in [
        (
            "Authorization: Bearer abcdefghijklmnopqrstuvwx\n",
            "abcdefghijklmnopqrstuvwx",
        ),
        ("mysql://root:hunterhunter@db/app\n", "hunterhunter"),
    ] {
        hidden_alone(content, "docs/api.txt", &[secret]);
    }
    for content in [
        "Authorization: Bearer YOUR_ACCESS_TOKEN_HERE\n",
        "Authorization: Bearer <token>\n",
        "Authorization: Bearer ${TOKEN}\n",
        "postgres://user:password@localhost\n",
        "url = postgres://user:password@localhost:5432/db\n",
        "https://user:pass@example.com\n",
    ] {
        shown_and_run(content, "docs/api.txt");
    }
}

/// D6: prose states a credential's value after `is`, `was`, `is:` or a bare
/// `:` or `=` after the credential's noun: an opaque value goes, the words
/// around it stay, and prose that states no value stays and runs.
#[test]
fn prose_states_a_value_in_its_common_forms() {
    for (content, path) in [
        (format!("// my password was {PASSWORD}\n"), "src/a.ts"),
        (format!("// the password is: {PASSWORD}\n"), "src/a.ts"),
        (
            format!("my password was {PASSWORD}, then I changed it\n"),
            "notes.txt",
        ),
        (format!("the password is: {PASSWORD}\n"), "notes.txt"),
        (format!("// the password = {PASSWORD}\n"), "src/a.ts"),
    ] {
        hidden_alone(&content, path, &[PASSWORD]);
    }
    for (content, path) in [
        ("// the password is required\n", "src/a.ts"),
        ("// the token was expired\n", "src/a.ts"),
        ("// my password was changed\n", "src/a.ts"),
        ("the password is: required\n", "notes.txt"),
        ("my password was changed yesterday\n", "notes.txt"),
    ] {
        shown_and_run(content, path);
    }
}

/// D7: a plain scalar whose first word is one opaque token ends with that
/// token: the text after a blank or a `;` is the line's own.
#[test]
fn a_plain_scalar_ends_with_its_token() {
    for (content, secret) in [
        (format!("Sec-Fake-Api-Key: {GITHUB}  tailJ\n"), GITHUB),
        (format!("Sec-Fake-Api-Key: {GITHUB}; visible10\n"), GITHUB),
        (format!("X-Custom: {GITHUB} tailL\n"), GITHUB),
        (format!("api_key: {KEY}  # rotated\n"), KEY),
    ] {
        hidden_alone(&content, "notes.txt", &[secret]);
    }
    let words = detect("password: correct horse 9\n", Some("config.yaml"));
    assert_eq!(
        words.sanitized, "password: [REDACTED-YAMLSECRETVALUE]\n",
        "a scalar of words is one value"
    );
}

/// D8: a type annotation and a documented placeholder after a credential's
/// name are no value, in a text file as in a request; the WebSocket nonce
/// under `Sec-WebSocket-Key` is the user agent's in a key/value form too.
#[test]
fn types_placeholders_and_the_handshake_nonce_are_no_credentials() {
    for content in [
        "password: str\n",
        "token: Token\n",
        "api_key: str = None\n",
        "dispatchToken: string\n",
        "session_token: Optional[str]\n",
        "secret: string | null\n",
    ] {
        shown_and_run(content, "notes.txt");
    }
    for (content, path) in [
        (
            "const headers = { \"Sec-WebSocket-Key\": \"dGhlIHNhbXBsZSBub25jZQ==\" };\n",
            "src/ws.ts",
        ),
        (
            "{\"Sec-WebSocket-Key\": \"dGhlIHNhbXBsZSBub25jZQ==\"}\n",
            "req.json",
        ),
    ] {
        shown_and_run(content, path);
    }
    // A value under the same names that is no type and no nonce stays hidden.
    for (content, secret) in [
        (format!("password: {PASSWORD}\n"), PASSWORD),
        ("token: Zq8Wv3Xr7Tn2Lp5\n".to_owned(), "Zq8Wv3Xr7Tn2Lp5"),
        (
            "{\"Sec-WebSocket-Key\": \"Zq8Wv3Xr7Tn2Lp5Ab1Cd9Ef4Gh6Jk8Lm0Np2Qr4St9Uv\"}\n"
                .to_owned(),
            "Zq8Wv3Xr7Tn2Lp5Ab1Cd9Ef4Gh6Jk8Lm0Np2Qr4St9Uv",
        ),
    ] {
        hidden_alone(&content, "notes.txt", &[secret]);
    }
}

/// Round 12 retest, D9 and D4: a request is judged by the file judge itself,
/// so what a read of text of no known file hides is refused, and what it
/// shows runs. The same text in a source file and a text file agrees.
#[test]
fn a_request_runs_exactly_what_the_file_judge_shows() {
    for text in [
        "password = \"hunter2-Sup3rS3cret\"",
        "password = \"otherVisible9\"",
        "Bearer abcdefghijklmnopqrstuvwx",
        "headers[\"Authorization\"] = \"Bearer abcdefghijklmnopqrstuvwx\";",
        "const h = { Authorization: \"Bearer abcdefghijklmnopqrstuvwx\" };",
        "const h = { Authorization: `Bearer abcdefghijklmnopqrstuvwx` };",
        "Authorization: Bearer abcdefghijklmnopqrstuvwx",
        "my password was Pl4net-Orb1t9",
        "the password is: s3cretPassw0rd",
        "password: str",
        "token: T | null",
        "  token: this.token,",
        "the password is required",
        "Authorization: Bearer <token>",
        "API_KEY=xxxxxxxxxxxxxxxx",
        "password: hunter2",
        "token: v1alpha1",
        "key: utf8mb4",
        // Round 13 retest: each item, in both directions.
        "db:\n  password: |\n    Fz8kQ2mL9xW4\n",
        "api_key: >-\n  Fz8kQ2mL9xW4\n",
        "secret: |-\n  Fz8kQ2mL9xW4\n",
        "db_password: |\n  Fz8kQ2mL9xW4\n",
        "description: |\n  Fz8kQ2mL9xW4 is no key's\n",
        "apiVersion: v1\nkind: Secret\nstringData:\n  extra: Fz8kQ2mL9xW4\ndata:\n  username: YWRtaW4=\n",
        "kind: ConfigMap\ndata:\n  mode: YWRtaW4=\n",
        "  token: randomBytes(16).toString('hex'),",
        "  apiKey: process.env.API_KEY!,",
        "token: getToken()",
        "  password: hash(pw),",
        "  token: opts.token ?? 'Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2Ua4',",
        "h.Authorization = `Basic ${Buffer.from(`${u}:${p}`).toString('base64')}`;",
        "h.Authorization = `Bearer Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2Ua4`;",
        "  apiKey: null,",
        "  password: undefined,",
        "DB_PASS=Fake0Moo",
        "-----BEGIN OPENSSH PRIVATE KEY-----",
        "export GITHUB_TOKEN=\"ghp_your_ghe_token\"",
        "export GITHUB_TOKEN=\"ghp_your_Hunter2_token\"",
        "export const ssrImportKey = `__vite_ssr_import__`",
        "secret == \"~\"",
        "/// are prose (`password: must be at least 8 characters`); after `NAME=` they\n/// are a value.\npub(super) fn is_sentence(key: &impl KeyFacts, secret: &str) -> bool {",
        // Final round: a literal over lines, a literal's config entry, and
        // a line of code with escaped quotes.
        "import os\n\ndef f(api_key=\"Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2Ua4\"):\n    return api_key\ntoken = \"Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2Ua4\n\"\n",
        "token = \"Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2Ua4\n\"\n",
        "token = \"Zq7Xk2Pm9Vt4Lw8Rb3Nc\\\n6Hy1Jd5Fg0Ts7Qe2Ua4\"\n",
        "message = \"the token is\nmissing here\"\n",
        "x_token = \"//registry.npmjs.org/:_authToken=Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2Ua4\\n\"",
        "x_token = \"//registry.npmjs.org/:_authToken=${NPM_TOKEN}\\n\"",
        "    let cs_credential = format!(\"  var cred = new NetworkCredential(\\\"svc\\\", \\\"{PASSWORD}\\\");\\n\");",
        "    let cs = \"  password = \\\"Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2Ua4\\\";\";",
        // bin16 retest: a name split from its value, an unquoted value and
        // the word after it, literals side by side.
        "const a = `password=${x}DvR8R8i4INNejI81pSk6GW`;",
        "const e = `\nline\npassword=${\"x\"}DvR8R8i4INNejI81pSk6GW\n`;",
        "const b = `API_KEY=${env}Qm7Hx2LpT9vRk4Wn8sZcYb`;",
        "const c = \"password=\" + \"DvR8R8i4INNejI81pSk6GW\";",
        "const u = `https://api.example.com/v1/users/${id}/tokens/${tokenId}`;",
        "password=Qm7Hx2LpT9vRk4Wn8sZcYb build42 then text",
        "password = \"Qm7Hx2LpT9vRk4Wn8sZcYb\" \\\n    \"DvR8R8i4INNejI81pSk6GW\"\n",
        // KNOWN: consistent misses, the same in both (neither hides them).
        "ＰＡＳＳＷＯＲＤ=Hunter2Zebra9xQ",
        "client.login(\"admin\",\"Hunter2Zebra9\")",
        "password=hunter2",
    ] {
        assert_eq!(
            holds_credential_as_request(text),
            detect(text, None).sanitized != text,
            "{text:?}"
        );
    }
    // The known misses stay misses on both sides: listed, not fixed.
    for known in [
        "ＰＡＳＳＷＯＲＤ=Hunter2Zebra9xQ",
        "client.login(\"admin\",\"Hunter2Zebra9\")",
        "password=hunter2",
    ] {
        assert!(
            !holds_credential_as_request(known),
            "{known:?} is now refused: update KNOWN"
        );
    }
    // Text code and prose read alike.
    for text in [
        "password = \"hunter2-Sup3rS3cret\"",
        "password = \"otherVisible9\"",
        "Bearer abcdefghijklmnopqrstuvwx",
        "headers[\"Authorization\"] = \"Bearer abcdefghijklmnopqrstuvwx\";",
        "const h = { Authorization: \"Bearer abcdefghijklmnopqrstuvwx\" };",
        "const h = { Authorization: `Bearer abcdefghijklmnopqrstuvwx` };",
        "my password was Pl4net-Orb1t9",
        "the password is: s3cretPassw0rd",
        "token: T | null",
        "  token: this.token,",
        "the password is required",
        "Authorization: Bearer <token>",
    ] {
        let hidden = detect(text, None).sanitized != text;
        for path in ["src/a.ts", "notes.txt", "src/a.py", "README.md"] {
            assert_eq!(
                detect(text, Some(path)).sanitized != text,
                hidden,
                "{path}: {text:?}"
            );
        }
    }
}

/// D9, D4: the token after the `Bearer` scheme is a credential in every
/// layout that holds it, letters alone too; only the token goes, and a
/// placeholder in its place runs.
#[test]
fn the_bearer_token_is_judged_once_in_every_layout() {
    const TOKEN: &str = "abcdefghijklmnopqrstuvwx";
    for content in [
        format!("Bearer {TOKEN}\n"),
        format!("Authorization: Bearer {TOKEN}\n"),
        format!("headers[\"Authorization\"] = \"Bearer {TOKEN}\";\n"),
        format!("const h = {{ Authorization: \"Bearer {TOKEN}\" }};\n"),
        format!("const h = {{ Authorization: `Bearer {TOKEN}` }};\n"),
    ] {
        for path in ["src/a.ts", "notes.txt"] {
            hidden_alone(&content, path, &[TOKEN]);
        }
    }
    for content in [
        "const h = { Authorization: `Bearer ${token}` };\n",
        "Authorization: Bearer YOUR_ACCESS_TOKEN_HERE\n",
        "headers[\"Authorization\"] = \"Bearer \" + token;\n",
        "Bearer authentication is used\n",
    ] {
        shown_and_run(content, "src/a.ts");
    }
}

/// D1, D2, D3: the expression a credential's name is assigned is walked
/// over its lines and into a composite literal, and each literal it may
/// hold whole is judged as the quoted assignment of it alone: a key in a
/// multi-line ternary, a chosen password in an arm, a Go map's element. An
/// arm that is no credential as a direct assignment stays.
#[test]
fn an_assigned_expression_is_walked_over_lines_and_composite_literals() {
    const KEY: &str = "Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2Ua4";
    for (content, path, secrets) in [
        (
            format!("const apiKey5 =\n  cond\n    ? \"{KEY}\"\n    : \"otherVisible17\";\n"),
            "src/a.ts",
            vec![KEY],
        ),
        (
            format!("const apiKey6 = cond\n  ? \"otherVisible18\"\n  : \"{KEY}\";\n"),
            "src/a.ts",
            vec![KEY],
        ),
        (
            "const password = isProd ? \"Tr0ub4dor&3xK9!zQ\" : \"devVisible1\";\n".to_owned(),
            "src/a.ts",
            vec!["Tr0ub4dor&3xK9!zQ", "devVisible1"],
        ),
        (
            "dbPassword = c ? \"x\" : \"hunter2-Sup3rS3cret\"\n".to_owned(),
            "src/a.py",
            vec!["hunter2-Sup3rS3cret"],
        ),
        (
            format!(
                "var apiKey = map[bool]string{{true: \"{KEY}\", false: \"otherVisible11\"}}[cond]\n"
            ),
            "src/a.go",
            vec![KEY],
        ),
    ] {
        hidden_alone(&content, path, &secrets);
    }
    // Each arm is judged as its direct assignment is.
    for (arm, hidden) in [
        ("devVisible1", true),
        ("x", false),
        ("otherVisible17", false),
    ] {
        let name = if arm == "otherVisible17" {
            "apiKey"
        } else {
            "password"
        };
        let direct = format!("{name} = \"{arm}\"\n");
        assert_eq!(
            detect(&direct, Some("src/a.ts")).sanitized != direct,
            hidden,
            "{direct:?}"
        );
    }
    for content in [
        "const apiKey5 =\n  cond\n    ? \"otherVisible17\"\n    : \"fallbackValue\";\n",
        "var apiKey = map[bool]string{true: \"otherVisible11\", false: \"dev\"}[cond]\n",
        "const token =\n  readToken();\n",
        "token:\n  name: \"service-account\"\n",
    ] {
        shown_and_run(content, "src/a.ts");
    }
}

/// D5: prose that states a value for a chosen credential's noun states its
/// secret when the value mixes letters with a digit or a symbol; prose of
/// words stays and runs.
#[test]
fn prose_states_a_chosen_secret_that_mixes_letters_with_digits_or_symbols() {
    for (content, secret) in [
        ("my password was Pl4net-Orb1t9\n", "Pl4net-Orb1t9"),
        (
            "the password is: hunter2-Sup3rS3cret\n",
            "hunter2-Sup3rS3cret",
        ),
        (
            "then password = hunter2-Sup3rS3cret\n",
            "hunter2-Sup3rS3cret",
        ),
        ("the password is: s3cretPassw0rd\n", "s3cretPassw0rd"),
        ("my password was Gr8pe-Juic3x\n", "Gr8pe-Juic3x"),
    ] {
        for path in ["notes.txt", "README.md"] {
            hidden_alone(content, path, &[secret]);
        }
    }
    for content in [
        "the password is required\n",
        "my password was changed\n",
        "the token was expired\n",
        "the password is password\n",
        "the password is hunter2\n",
        "then password = password2\n",
        "API_PASSWORD=Fake!123\n",
    ] {
        shown_and_run(content, "notes.txt");
    }
}

/// D3 (bin16 retest): an unquoted value ends at its first blank. The word
/// after it is the line's own, digits or not (`build42`, `release1`, `v2`),
/// and stays readable and searchable; only the value goes.
#[test]
fn an_unquoted_value_ends_at_its_first_blank() {
    const VALUE: &str = "Qm7Hx2LpT9vRk4Wn8sZcYb";
    const B64: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYzQ8vT3kLm2";
    for (content, kept) in [
        (
            format!("password={VALUE} build42 then text\n"),
            " build42 then text\n",
        ),
        (format!("token: {VALUE} release1\n"), " release1\n"),
        (format!("DB_PASSWORD={VALUE} v2 # note\n"), " v2 # note\n"),
        (format!("secret_key={B64} mark8m\n"), " mark8m\n"),
        (format!("ACCESS_TOKEN={B64} is the key\n"), " is the key\n"),
        (format!("the password is {VALUE} build42\n"), " build42\n"),
    ] {
        for path in [Some("notes.txt"), None] {
            let shown = detect(&content, path).sanitized;
            let secret = if content.contains(B64) { B64 } else { VALUE };
            assert!(!shown.contains(secret), "{path:?}: {shown:?}");
            assert!(shown.ends_with(kept), "{path:?}: {shown:?}");
            let placeholder = &shown[shown.find("[REDACTED-").expect("placeholder")..];
            assert!(
                placeholder.contains(&format!("]{kept}")),
                "{path:?}: {shown:?}"
            );
        }
    }
    // Words of a value's make a blank apart: each judged alone.
    let shown = detect("SIGNING_KEY=x-9f8.6d0.818 84c.7d6.59a\n", Some("notes.txt")).sanitized;
    assert!(!shown.contains("x-9f8.6d0.818"), "{shown:?}");
    assert!(shown.ends_with("] 84c.7d6.59a\n"), "{shown:?}");
}

/// D6, D8: in a text file, a member access, `undefined`, `null`, a type
/// parameter and the `,` that ends a member are no secret values, as in
/// code and in a request.
#[test]
fn code_in_a_text_file_holds_no_secret_value() {
    for content in [
        "token: T | null\n",
        "secret: T\n",
        "  token: this.token,\n",
        "  secret: opts.secret,\n",
        "  password: undefined,\n",
        "  apiKey: null,\n",
    ] {
        for path in ["notes.txt", "README.md"] {
            shown_and_run(content, path);
        }
    }
    for (content, secret) in [
        ("  password: Fake0Moo9Zed,\n", "Fake0Moo9Zed"),
        ("token: Zq8Wv3Xr7Tn2Lp5\n", "Zq8Wv3Xr7Tn2Lp5"),
    ] {
        hidden_alone(content, "notes.txt", &[secret]);
    }
}

/// D10: a shell parameter expansion keeps its operator: the default after
/// it is the value.
#[test]
fn a_shell_default_keeps_its_operator() {
    const KEY: &str = "Zq7Xk2Pm9Vt4Lw8Rb3Nc6Hy1Jd5Fg0Ts7Qe2Ua4";
    for operator in [":-", ":=", ":+"] {
        let content = format!("API_KEY=\"${{API_KEY{operator}{KEY}}}\"\n");
        for path in ["deploy.sh", "notes.txt", ".env", "app.properties"] {
            hidden_alone(&content, path, &[KEY]);
        }
        // Text a read hid, judged again where every file's rules apply (a
        // provider's boundary), keeps its one placeholder and its syntax.
        let read = detect(&content, None).sanitized;
        assert_eq!(
            detect_in(&read, Scope::Stored(None)).sanitized,
            read,
            "{read:?}"
        );
    }
    let quoted = detect("password = \"hunter2-Sup3rS3cret\"\n", None).sanitized;
    assert_eq!(detect_in(&quoted, Scope::Stored(None)).sanitized, quoted);
    shown_and_run("API_KEY=\"${API_KEY:-}\"\n", "deploy.sh");
}
