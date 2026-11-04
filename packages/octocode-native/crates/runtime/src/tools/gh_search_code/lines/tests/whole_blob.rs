//! A listed line whose secrecy only the whole blob shows.
use super::*;

/// A listed line of a Kubernetes Secret is judged with the whole blob: the
/// value's own line, and its neighbors, say nothing of the kind above them.
#[test]
fn a_listed_line_is_judged_with_the_whole_blob_when_a_window_cannot_show_it() {
    let security = crate::security::ContentSecurity::new();
    let value = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIz";
    let text = format!(
        "apiVersion: v1\nkind: Secret\nmetadata:\n  name: x\n  a: 1\n  b: 2\n  c: 3\ndata:\n  tls.crt: {value}\n"
    );
    let hits = scan(
        text.as_bytes(),
        "deploy/secret.yaml",
        &["tls.crt".to_owned()],
        "",
        Window {
            before: 1,
            after: 0,
        },
        &security,
    );
    let FileHits::Lines { lines, .. } = hits else {
        panic!("the blob is read");
    };
    assert!(!lines.join("\n").contains(value), "{lines:?}");
    // No such pattern applies at this path: the bytes are plain text.
    let hits = scan(
        text.as_bytes(),
        "deploy/secret.txt",
        &["tls.crt".to_owned()],
        "",
        Window::default(),
        &security,
    );
    let FileHits::Lines { lines, .. } = hits else {
        panic!("the blob is read");
    };
    assert!(lines.join("\n").contains(value), "{lines:?}");
}
