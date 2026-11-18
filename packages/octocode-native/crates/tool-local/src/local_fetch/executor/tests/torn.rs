//! A whole read of a source rewritten in place while it is read: its sample
//! returns the old, shorter version whole; the writer then truncates the
//! file and writes a longer version; the rest of the read, at the old
//! length, returns the new version's rest, which starts inside a token.
//! Bytes so joined are no version of the file, and the token's tail they
//! hold matches no secret pattern alone: none of them may be served. Read
//! from the served result. The secret is fake.

use super::*;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::security::ContentSecurity;
use serde_json::Value;

/// A fake GitHub token's body (after `ghp_`).
const BODY: &str = "FAKEtornTok3nQ7wX9mZ2pL4vR8sN1cY6bU3";

/// The version the file holds when it is opened: no secret.
fn before() -> String {
    "needle one\nneedle xxxxxx\n".to_owned()
}

/// The version written while it is read: longer, its token spanning the
/// byte the first version ends at.
fn after() -> String {
    format!("needle key ghp_{BODY} needle tail\nneedle two\nneedle three\n")
}

/// One localFetch of `path`, as an agent receives its result.
fn served(path: &std::path::Path, dir: &std::path::Path) -> Value {
    let result = execute_local_fetch(
        &q(path),
        &Paths(dir.to_path_buf()),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        None,
    );
    let data = serde_json::to_value(&result).expect("row");
    octocode_runtime_base::test_support::staged_response(ToolId::LocalFetch, vec![data])
}

/// Registers `write` for `path`, made after each sample a read takes while
/// it returns `true`; returns the canonical path it is registered under.
fn after_each_sample(
    path: &std::path::Path,
    write: impl FnMut() -> bool + Send + 'static,
) -> PathBuf {
    let source = fs::canonicalize(path).expect("canonical source");
    AFTER_SAMPLE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((source.clone(), Box::new(write)));
    source
}

/// Drops what is still registered for `source`; whether something was.
fn unregister(source: &std::path::Path) -> bool {
    let mut races = AFTER_SAMPLE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let before = races.len();
    races.retain(|(path, _)| path != source);
    races.len() != before
}

/// No piece of the token's body, eight characters or longer, is served.
fn assert_no_token_piece(out: &Value) {
    let text = out.to_string();
    for start in 0..=BODY.len() - 8 {
        let piece = &BODY[start..start + 8];
        assert!(
            !text.contains(piece),
            "a piece of the token, {piece}, is served: {text}"
        );
    }
}

/// A read a rewrite tore, once, is made again: the source served is the
/// version that read settled on, its token shown as its placeholder.
#[test]
fn a_whole_read_a_rewrite_tore_is_served_from_the_settled_read() {
    let temp = Fixture::new();
    let path = temp.0.join("live.txt");
    fs::write(&path, before()).expect("fixture");
    let target = path.clone();
    let source = after_each_sample(&path, move || {
        fs::write(&target, after()).expect("rewrite in place");
        false
    });
    let out = served(&path, &temp.0);
    assert!(
        !unregister(&source),
        "the source was rewritten while it was read"
    );
    assert_no_token_piece(&out);
    let data = &out["results"][0]["data"];
    let content = data["content"].as_str().unwrap_or_default();
    assert!(
        content.contains("needle tail")
            && octocode_runtime_base::security::placeholders(content) > 0,
        "the settled version is served, its token shown as its placeholder: {out}"
    );
}

/// A source rewritten after every sample a read takes never settles: the
/// read fails and says why, and nothing of it is served.
#[test]
fn a_source_rewritten_through_every_read_is_not_served() {
    let temp = Fixture::new();
    let path = temp.0.join("live.txt");
    fs::write(&path, before()).expect("fixture");
    let (target, mut writes) = (path.clone(), 0u64);
    let source = after_each_sample(&path, move || {
        writes += 1;
        let version = if writes % 2 == 1 { after() } else { before() };
        fs::write(&target, version).expect("rewrite in place");
        true
    });
    let out = served(&path, &temp.0);
    assert!(unregister(&source), "the writer was registered");
    assert_no_token_piece(&out);
    let data = &out["results"][0]["data"];
    assert!(
        data["content"].is_null(),
        "no text of the source is served: {out}"
    );
    assert!(
        data.to_string().contains("kept changing while it was read"),
        "the row says why it read nothing: {out}"
    );
}
