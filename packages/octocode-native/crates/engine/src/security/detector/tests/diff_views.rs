//! Round 14, B: a unified diff's hunks are judged as the text they change, the
//! marker column (`+`, `-`, a blank) aside, so a YAML block scalar's body and a
//! Kubernetes Secret's `data` read as in the file; each value goes from its own
//! line, and the marker and every byte around the value stay.

use super::*;

const BODY: &str = "Hk3Lm9Pq2Rs7Tv5Wx1";
const OLD_BODY: &str = "Ab3Cd9Ef2Gh7Jk5Lm1";
const ENCODED: &str = "cDRzc3cwcmQ=";

/// The patch the retest read: a block scalar changed on `+` and context lines,
/// then a ConfigMap and a Secret added as two documents.
fn manifest_patch() -> String {
    format!(
        "diff --git a/deploy/app.yaml b/deploy/app.yaml\nindex 1111111..2222222 100644\n--- a/deploy/app.yaml\n+++ b/deploy/app.yaml\n@@ -1,3 +1,15 @@\n config:\n   token: |-\n     {OLD_BODY}\n+  token: |-\n+    {BODY}\n+---\n+kind: ConfigMap\n+data:\n+  cacheKeyPrefix: app-cache\n+---\n+apiVersion: v1\n+kind: Secret\n+metadata:\n+  name: creds\n+data:\n+  username: YWRtaW4=\n+  password: {ENCODED}\n"
    )
}

/// Every value of the diff goes, one placeholder each, at every path a diff is
/// read under (a `.patch` or `.txt` file, a GitHub patch, the pathless net);
/// the markers, the names and the ConfigMap's value stay.
#[test]
fn a_diff_hides_the_values_its_text_holds() {
    let patch = manifest_patch();
    for path in [
        Some("x.patch"),
        Some("x.txt"),
        Some("github-history-item"),
        None,
    ] {
        assert_redacts_each(&patch, path, &[OLD_BODY, BODY, "YWRtaW4=", ENCODED]);
        let shown = detect(&patch, path).sanitized;
        assert!(
            shown.contains("+  cacheKeyPrefix: app-cache\n"),
            "{path:?}: {shown}"
        );
        assert!(
            shown.contains("+  username: [REDACTED-"),
            "{path:?}: {shown}"
        );
        assert!(shown.contains("+    [REDACTED-"), "{path:?}: {shown}");
    }
    // A GitHub patch starts at its first hunk header.
    let github = patch[patch.find("@@").expect("hunk")..].to_owned();
    assert_redacts_each(
        &github,
        Some("github-history-item"),
        &[OLD_BODY, BODY, "YWRtaW4=", ENCODED],
    );
}

/// Removed and context lines are judged as the added ones are, and a hunk ends
/// at the next file's header: the next file's lines are its own.
#[test]
fn every_side_of_a_hunk_is_judged_and_the_next_file_starts_fresh() {
    let patch = format!(
        "@@ -1,3 +1,3 @@\n db:\n-  password: |\n-    {OLD_BODY}\n+  password: |\n+    {BODY}\n--- a/notes.md\n+++ b/notes.md\n@@ -1 +1 @@\n-the token is expired\n+the token is refreshed\n"
    );
    assert_redacts_each(&patch, Some("x.patch"), &[OLD_BODY, BODY]);
}

/// A value over several lines of a hunk goes with each line's marker kept: the
/// replacement keeps the lines, and every one still starts with its marker.
#[test]
fn a_value_over_several_lines_keeps_every_lines_marker() {
    let body = "MIIBODEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let patch = format!(
        "@@ -0,0 +1,4 @@\n+-----BEGIN RSA PRIVATE KEY-----\n+{body}\n+{body}\n+-----END RSA PRIVATE KEY-----\n"
    );
    let result = detect(&patch, Some("key.patch"));
    assert!(!result.sanitized.contains(body), "{}", result.sanitized);
    assert_eq!(result.sanitized.lines().count(), patch.lines().count());
    for line in result.sanitized.lines().skip(1) {
        assert!(
            line.starts_with('+'),
            "{line:?} lost its marker: {}",
            result.sanitized
        );
    }
}

/// What reads as prose, a name or a placeholder on a hunk's lines stays as it
/// does in the file; and text with no hunk header is no diff, its `+` and `-`
/// its own (`-DMAX_TOKENS=4096`, a list item).
#[test]
fn a_diffs_readable_lines_stay() {
    for text in [
        "@@ -1 +1,3 @@\n config:\n+  password: must be at least 8 characters\n+  token: ${API_TOKEN}\n",
        "@@ -1 +1,2 @@\n kind: ConfigMap\n+data:\n+  cacheKeyPrefix: app-cache\n",
        "+  token: |-\n+    Hk3Lm9Pq2Rs7Tv5Wx1\n",
        "-DMAX_TOKENS=4096\n- item one\n",
    ] {
        for path in [Some("x.patch"), None] {
            assert_eq!(detect(text, path).sanitized, text, "{path:?}");
        }
    }
}

/// The input screen is the file judge: a request holding a diff is refused
/// exactly where a read of the same text hides something, `+` lines with no
/// hunk header being no diff in both.
#[test]
fn a_request_holding_a_diff_is_refused_as_a_read_hides_it() {
    let patch = manifest_patch();
    for text in [
        patch.as_str(),
        "@@ -1 +1,2 @@\n+  token: |-\n+    Hk3Lm9Pq2Rs7Tv5Wx1\n",
        "@@ -0,0 +1,3 @@\n+kind: Secret\n+data:\n+  username: YWRtaW4=\n",
        "+  token: |-\n+    Hk3Lm9Pq2Rs7Tv5Wx1\n",
        "+    Hk3Lm9Pq2Rs7Tv5Wx1",
        "+  password: Hunter2Xq9vB7mK\n",
        "@@ -1 +1,2 @@\n config:\n+  password: must be at least 8 characters\n",
    ] {
        let hides = detect(text, None).sanitized != text;
        assert_eq!(holds_credential_as_request(text), hides, "{text:?}");
    }
}

/// Retest R3: the old side of a hunk (`-` and context lines) and its new side
/// (`+` and context lines) are two documents. A ConfigMap turned into a Secret
/// keeps its removed ConfigMap values shown: the `kind: Secret` of the new side
/// decides nothing on the old one, and the new side's values under it go.
#[test]
fn each_side_of_a_hunk_is_its_own_document() {
    let patch = format!(
        "@@ -1,5 +1,5 @@\n apiVersion: v1\n-kind: ConfigMap\n+kind: Secret\n data:\n-  mode: production\n+  password: {ENCODED}\n"
    );
    let reversed = format!(
        "@@ -1,5 +1,5 @@\n apiVersion: v1\n-kind: Secret\n+kind: ConfigMap\n data:\n-  password: {ENCODED}\n+  mode: production\n"
    );
    for path in [Some("x.patch"), None] {
        for text in [&patch, &reversed] {
            let shown = detect(text, path).sanitized;
            assert!(shown.contains("  mode: production\n"), "{path:?}: {shown}");
            assert!(!shown.contains(ENCODED), "{path:?}: {shown}");
            assert_redacts_each(text, path, &[ENCODED]);
        }
    }
}

/// A context line belongs to both sides: a value on it goes when either side
/// reads it as a Secret's value, and every line keeps its marker.
#[test]
fn a_context_line_goes_when_either_side_hides_it() {
    for (old, new) in [("ConfigMap", "Secret"), ("Secret", "ConfigMap")] {
        let patch = format!(
            "@@ -1,3 +1,3 @@\n apiVersion: v1\n-kind: {old}\n+kind: {new}\n data:\n   password: {ENCODED}\n"
        );
        assert_redacts_each(&patch, Some("x.patch"), &[ENCODED]);
        let shown = detect(&patch, Some("x.patch")).sanitized;
        assert_eq!(shown.lines().count(), patch.lines().count(), "{shown}");
        assert!(
            shown.contains(&format!("-kind: {old}\n+kind: {new}\n")),
            "{shown}"
        );
    }
}

/// A key changed inside its armor: each side holds a whole key, the other
/// side's line between its lines. Both bodies go, every line keeps its marker
/// and the diff keeps its lines.
#[test]
fn a_key_changed_inside_its_armor_hides_both_bodies() {
    let old = "MIIBOldEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let new = "MIIBNewEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let patch = format!(
        "@@ -1,3 +1,3 @@\n -----BEGIN RSA PRIVATE KEY-----\n-{old}\n+{new}\n -----END RSA PRIVATE KEY-----\n"
    );
    let result = detect(&patch, Some("key.patch"));
    let shown = &result.sanitized;
    assert!(!shown.contains(old) && !shown.contains(new), "{shown}");
    assert_eq!(shown.lines().count(), patch.lines().count(), "{shown}");
    let markers: Vec<char> = shown
        .lines()
        .skip(1)
        .filter_map(|l| l.chars().next())
        .collect();
    assert_eq!(markers, [' ', '-', '+', ' '], "{shown}");
}
