//! What a failed git run may report: its stderr without the credential, an
//! authorization header or an internal checkout stage path.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::path::Path;

/// The header value git builds from the helper's answers: HTTP Basic with the
/// `x-access-token` user.
pub(super) fn basic_credential(token: &str) -> String {
    STANDARD.encode(format!("x-access-token:{token}"))
}

/// `text` without the token, its HTTP Basic form, any authorization header
/// a checkout stage or the shared object store under `home`.
pub(super) fn scrub(text: &str, token: Option<&str>, home: &Path) -> String {
    let mut result = text.to_owned();
    if let Some(token) = token {
        // The base64 header value git builds first: it does not contain the
        // token, and git can echo it without the `Authorization:` prefix.
        result = result
            .replace(&basic_credential(token), "[REDACTED]")
            .replace(token, "[REDACTED]");
    }
    // A checkout stage and the object store are internal paths under the
    // Octocode home (git names a linked worktree's common directory); the
    // error names what failed, not where it was staged or stored.
    let homes = [Some(home.to_path_buf()), home.canonicalize().ok()];
    for home in homes.into_iter().flatten() {
        for (internal, name) in [
            (crate::gh_git::stage_root(&home), "<checkout>"),
            (crate::gh_git::store_root(&home), "<store>"),
        ] {
            let pattern = format!(r"{}[^\s'\x22]*", regex::escape(&internal.to_string_lossy()));
            if let Ok(internal) = regex::Regex::new(&pattern) {
                result = internal.replace_all(&result, name).into_owned();
            }
        }
    }
    // Redact any authorization header regardless of scheme. Git sends the
    // helper's answers as `Authorization: Basic <base64(x-access-token:…)>`;
    // the base64 form does NOT contain the plaintext token, so this header
    // pattern — not the token replace above — stops a reversible credential
    // from leaking if git echoes a request.
    static AUTH: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        #[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
        regex::Regex::new(r"(?i)Authorization:\s*(Bearer|token|Basic)\s+\S+")
            .expect("static authorization regex")
    });
    AUTH.replace_all(&result, "Authorization: [REDACTED]")
        .into_owned()
}
