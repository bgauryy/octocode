//! Provider tokens and credential-bearing URLs under names that say nothing:
//! the value's own shape is what marks it, in prose, YAML and code alike.

use super::*;

/// A fake body of `len` characters from a token's alphabet, as random as a
/// generator's.
fn body(len: usize) -> String {
    "ZpcQH1_QgDQufjvdpwF2GZoV6UGjx7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoIwLsbUAFXniClAUADRuS4"
        .chars()
        .cycle()
        .take(len)
        .collect()
}

fn hex(len: usize) -> String {
    "419a2109fd3a5e32640e5f0122cee0c78bbffee2c3937d4e"
        .chars()
        .cycle()
        .take(len)
        .collect()
}

/// The neutral places a value sits in: a sentence, a list row, a YAML item
/// and a code constant. None names a credential.
fn neutral(value: &str) -> [(String, &'static str); 4] {
    [
        (format!("row 7 {value}\n"), "notes/prov.txt"),
        (
            format!("- entry 7 was `{value}` until rotated\n"),
            "docs/prov.md",
        ),
        (format!("item7: {value}\n"), "conf/prov.yaml"),
        (
            format!("export const c07 = \"{value}\"; // 7\n"),
            "src/prov.ts",
        ),
    ]
}

/// A provider's token is hidden whole wherever it sits, and only it.
#[test]
fn provider_tokens_under_neutral_names_are_hidden() {
    let discord = format!("O{}.{}.{}", body(23), "CJmyA_", body(38));
    for token in [
        format!("GOCSPX-{}", body(28)),
        format!("dckr_pat_{}", body(27)),
        format!("1//0{}", body(60)),
        format!("v1.{}", hex(40)),
        format!("pypi-AgEIcHlwaS5vcmc{}", body(81)),
        format!("pypi-AgEIcHlwaS5vcmc{}", body(140)),
        format!("EAAC{}", body(120).replace('_', "x")),
        format!(
            "eyJhbGciOiJSUzI1NiIsImtpZCI6ImFkdiJ9.{}.{}",
            body(86),
            body(120)
        ),
        format!("AGE-SECRET-KEY-1{}", hex(58).to_uppercase()),
        discord,
    ] {
        for (content, path) in neutral(&token) {
            for path in [Some(path), None] {
                assert_redacts_only(&content, path, &token);
            }
        }
    }
}

/// A URL that carries a credential loses the credential and keeps the rest:
/// its scheme, host, path and the other parameters.
#[test]
fn credentials_in_urls_and_connection_strings_are_hidden() {
    let sas = format!("{}%3D", body(43));
    let presigned = hex(64);
    let dsn_key = hex(32);
    let query_token = body(24);
    let password = "Txkv8kGCunaGKYxjlY";
    let cases: [(String, &str); 12] = [
        (
            format!("https://adv.blob.core.windows.net/c/f.txt?sv=2022-11-02&sp=rw&sig={sas}"),
            &sas,
        ),
        (
            format!(
                "https://bucket.s3.amazonaws.com/k?X-Amz-Date=20260101T000000Z&X-Amz-Signature={presigned}"
            ),
            &presigned,
        ),
        (
            format!("https://{dsn_key}@o123456.ingest.sentry.io/4501234"),
            &dsn_key,
        ),
        (
            format!("https://deploy:{password}@git.example.com/org/repo.git"),
            password,
        ),
        (
            format!("https://example.com/hook?token={query_token}&x=1"),
            &query_token,
        ),
        (
            format!("https://example.com/login?user=svc&password={password}"),
            password,
        ),
        (format!("svc:{password}@tcp(db:3306)/app"), password),
        (
            format!("mysql://root:{password}@10.0.0.5:3306/app"),
            password,
        ),
        (
            format!("host=db user=svc password={password} dbname=app"),
            password,
        ),
        (format!("password={password} host=db dbname=app"), password),
        (
            format!("Server=tcp:db,1433;Database=app;User ID=sa;Password={password};Encrypt=True;"),
            password,
        ),
        (
            format!("jdbc:sqlserver://db:1433;databaseName=app;user=sa;password={password};"),
            password,
        ),
    ];
    for (value, secret) in &cases {
        for (content, path) in neutral(value) {
            for path in [Some(path), None] {
                assert_redacts_only(&content, path, secret);
            }
        }
    }
}

/// A password a command takes as an argument, a stored password's hash and
/// a form body's credential fields go; the command and the other fields
/// stay.
#[test]
fn command_arguments_hashes_and_form_fields_are_hidden() {
    for (content, path, secrets) in [
        (
            "curl -u admin:KeOTk!KGKP#IuwbB9 https://api.example.com/v1\n",
            "lay.sh",
            &["KeOTk!KGKP#IuwbB9"][..],
        ),
        (
            "mysql -u root -pn1EVVoOH62ABFtR3 app\n",
            "lay.sh",
            &["n1EVVoOH62ABFtR3"],
        ),
        (
            "sshpass -p 'nePD6!n1Oe#EJz8y9' ssh root@host\n",
            "lay.sh",
            &["nePD6!n1Oe#EJz8y9"],
        ),
        (
            "docker login -u svc -p 7pKNrBaIWuNRxQ436J6P registry.example.com\n",
            "lay.sh",
            &["7pKNrBaIWuNRxQ436J6P"],
        ),
        (
            "svc:$apr1$Y2MVdy9m$HOMncteftIqb8H6aioznlK\n",
            "htpasswd.txt",
            &["$apr1$Y2MVdy9m$HOMncteftIqb8H6aioznlK"],
        ),
        (
            "root:{SHA}FuoeJcg1Bgi9Tkyaa2KmxXb0sW6=\n",
            "htpasswd.txt",
            &["{SHA}FuoeJcg1Bgi9Tkyaa2KmxXb0sW6="],
        ),
        (
            "username=svc&password=jeirRvWywvWI1ABC&grant_type=password&client_secret=Yn8NgbPWlDtlgYaEi6XjPYhp\n",
            "lay.http",
            &["jeirRvWywvWI1ABC", "Yn8NgbPWlDtlgYaEi6XjPYhp"],
        ),
    ] {
        for path in [Some(path), None] {
            assert_redacts_each(content, path, secrets);
        }
    }
}

/// The delimiter before a secret is the text's own: an Azure AD client
/// secret in quotes keeps both of them.
#[test]
fn a_secret_between_delimiters_keeps_them() {
    let secret = "INR8Q~1oCAs5gxV2QLnMwwlDXMdux2dSK8S01iIF";
    for (content, path) in neutral(secret) {
        assert_redacts_only(&content, Some(path), secret);
    }
}

/// Text with a token's punctuation and none of its substance stays: dotted
/// names, a version, a reference, a placeholder, a URL with no credential.
#[test]
fn ordinary_text_with_a_tokens_punctuation_stays() {
    for content in [
        "OrganizationMembershipList.create.someVeryLongMethodNameForTests\n",
        "NotificationPreferenceStore.update.anotherQuiteLongMethodNameHere\n",
        "the release is v1.2 and the tag v1.20.3 follows\n",
        "ratio 1//0 is undefined; see 1//2 below\n",
        "GOCSPX- is the prefix of a Google OAuth client secret\n",
        "see https://example.com/docs?page=2&sort=name#tokens for the list\n",
        "https://user@example.com/path and ssh://git@github.com/org/repo.git\n",
        "https://example.com/reset?token=<token>&next=/home\n",
        "fetch(`${base}/login?token=${encodeURIComponent(token)}`)\n",
        "curl -u \"$USER:$PASS\" https://api.example.com/v1\n",
        "curl -u username:password https://api.example.com/v1\n",
        "mysql -u root -p app\n",
        "mysql --port=3306 -u root app\n",
        "docker login -u svc --password-stdin registry.example.com\n",
        "sshpass -p \"$SSH_PASS\" ssh root@host\n",
        "echo \"$1$2\" and price is $5$ per unit\n",
        "host=db user=svc dbname=app sslmode=require\n",
        "import { tcp } from \"net\"; connect(user, host);\n",
    ] {
        for path in [Some("notes/readme.md"), None] {
            let result = detect(content, path);
            assert_eq!(result.sanitized, content, "{path:?}");
        }
    }
}
