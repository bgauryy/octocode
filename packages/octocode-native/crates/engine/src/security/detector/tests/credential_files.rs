//! Credential files a name rule may not withhold (a kubeconfig copied out of
//! `.kube/`, a GCP service-account key named `<project>-<hex>.json`), read as
//! plain text: their secrets are hidden by content, and only the secret part
//! of each line goes. What says where and who stays readable. Every value is
//! fake.

use super::*;

/// The text `text` becomes at `path`, every secret of `secrets` gone and every
/// piece of `kept` still there, line for line.
fn judged(text: &str, path: Option<&str>, secrets: &[&str], kept: &[&str]) {
    let result = detect(text, path);
    for secret in secrets {
        assert!(
            !result.sanitized.contains(secret),
            "{path:?}: {secret} in {}",
            result.sanitized
        );
    }
    for piece in kept {
        assert!(
            result.sanitized.contains(piece),
            "{path:?}: {piece} lost: {}",
            result.sanitized
        );
    }
    assert_eq!(result.sanitized.lines().count(), text.lines().count());
}

/// A kubeconfig read as plain YAML, whatever its name: a user's
/// `client-key-data` (base64 of a PEM private key, RSA or EC), `token` (a JWT,
/// a bootstrap token or an opaque string), `password` and an auth provider's
/// `client-secret`, `id-token` and `refresh-token` are hidden. The cluster's
/// `certificate-authority-data` and the user's `client-certificate-data` are
/// certificates, public by design (a TLS peer shows its own in every
/// handshake), so they stay, as do the server, names and the issuer URL.
#[test]
fn a_renamed_kubeconfig_hides_each_user_secret_and_keeps_its_certificates() {
    let ca = "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCk1JSUM1ekNDQWMrZ0F3SUJBZ0lCQURBTkJna3Fo";
    let cert = "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCk1JSURJVENDQWdtZ0F3SUJBZ0lJWjQ3";
    let rsa_key = "LS0tLS1CRUdJTiBSU0EgUFJJVkFURSBLRVktLS0tLQpNSUlFb3dJQkFBS0NBUUVBN3E5";
    let ec_key = "LS0tLS1CRUdJTiBFQyBQUklWQVRFIEtFWS0tLS0tCk1IY0NBUUVFSUE5";
    let jwt = "eyJhbGciOiJSUzI1NiIsImtpZCI6IkZha2UifQ.eyJzdWIiOiJzeXN0ZW06c2EifQ.ZmFrZXNpZw";
    let bootstrap = "abcdef.0123456789abcdef";
    let opaque = "Zq7xK2mPlanted9OpaqueTok";
    let kubeconfig = format!(
        "apiVersion: v1\nkind: Config\nclusters:\n- cluster:\n    certificate-authority-data: {ca}\n    server: https://10.0.0.1:6443\n  name: prod\nusers:\n- name: admin\n  user:\n    client-certificate-data: {cert}\n    client-key-data: {rsa_key}\n- name: edge\n  user:\n    client-key-data: {ec_key}\n- name: sa\n  user:\n    token: {jwt}\n- name: boot\n  user:\n    token: {bootstrap}\n- name: static\n  user:\n    token: {opaque}\n- name: basic\n  user:\n    username: admin\n    password: Zq7xK2mPlanted9pw\n- name: oidc\n  user:\n    auth-provider:\n      name: oidc\n      config:\n        client-id: kubernetes\n        client-secret: Zq7xK2mPlanted9ClientSecret\n        id-token: {jwt}\n        refresh-token: Zq7xK2mPlanted9Refresh\n        idp-issuer-url: https://accounts.example.com\n"
    );
    for path in [
        Some("kube/config"),
        Some("k/notes.yaml"),
        Some("ci/admin.conf"),
        None,
    ] {
        judged(
            &kubeconfig,
            path,
            &[
                rsa_key,
                ec_key,
                jwt,
                bootstrap,
                opaque,
                "Zq7xK2mPlanted9pw",
                "Zq7xK2mPlanted9ClientSecret",
                "Zq7xK2mPlanted9Refresh",
            ],
            &[
                ca,
                cert,
                "server: https://10.0.0.1:6443",
                "username: admin",
                "client-id: kubernetes",
                "idp-issuer-url: https://accounts.example.com",
            ],
        );
    }
}

/// A GCP service-account key, by its content whatever its name: the
/// `private_key` and its `private_key_id` (40 hex) are hidden. The project,
/// the client id, the OAuth endpoints and `client_email`, the account's
/// identity, stay.
#[test]
fn a_service_account_key_hides_its_private_key_and_key_id() {
    let key_id = "1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b";
    let body = "MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7Zq7xK2mPlanted9";
    let key = format!(
        "{{\n  \"type\": \"service_account\",\n  \"project_id\": \"my-proj\",\n  \"private_key_id\": \"{key_id}\",\n  \"private_key\": \"-----BEGIN PRIVATE KEY-----\\n{body}\\n-----END PRIVATE KEY-----\\n\",\n  \"client_email\": \"deployer@my-proj.iam.gserviceaccount.com\",\n  \"client_id\": \"104729384756102938475\",\n  \"auth_uri\": \"https://accounts.google.com/o/oauth2/auth\",\n  \"token_uri\": \"https://oauth2.googleapis.com/token\"\n}}\n"
    );
    for path in [
        Some("gcp/my-proj-1a2b3c4d5e6f.json"),
        Some("keys/sa.json"),
        None,
    ] {
        judged(
            &key,
            path,
            &[key_id, body],
            &[
                "\"type\": \"service_account\"",
                "\"project_id\": \"my-proj\"",
                "\"client_id\": \"104729384756102938475\"",
                "deployer@my-proj.iam.gserviceaccount.com",
                "https://oauth2.googleapis.com/token",
            ],
        );
    }
}

/// A service-account address names an identity, as any address does: plain
/// text that grants a role to one shows it.
#[test]
fn a_service_account_address_is_shown() {
    let text = "gcloud projects add-iam-policy-binding my-proj \\\n  --member=serviceAccount:deployer@my-proj.iam.gserviceaccount.com \\\n  --role=roles/storage.admin\n";
    for path in [Some("docs/iam.txt"), Some("deploy/iam.sh"), None] {
        judged(
            text,
            path,
            &[],
            &["serviceAccount:deployer@my-proj.iam.gserviceaccount.com"],
        );
    }
}
