//! S1, S2, S4: a streamed source shows what a source held whole shows, line for
//! line: a Secret's values go and their names stay, what no walk found to be a
//! Secret or a JSON Web Key stays, a miss behind a placeholder says so, and the
//! notes count and size what they state exactly. Every value below is fake.
use super::*;

/// `text` as a read holding it whole serves it: the in-memory oracle.
fn held_whole(text: &str, path: &Path) -> String {
    crate::tools::source_view::prepare::sealed_view(
        text.as_bytes(),
        path,
        &crate::security::ContentSecurity::new(),
    )
    .expect("sealed")
    .text
    .into_owned()
}

fn named(suffix: &str, content: &str) -> tempfile::NamedTempFile {
    let mut file = tempfile::Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("temp");
    std::io::Write::write_all(&mut file, content.as_bytes()).expect("write");
    file
}

/// `head`, then a ConfigMap document of plain values past the whole-file ceiling.
fn yaml_past_whole(head: &str) -> String {
    let mut text = format!("{head}---\napiVersion: v1\nkind: ConfigMap\ndata:\n");
    let mut at = 0;
    while text.len() < past_whole() {
        text.push_str(&format!("  pad{at:07}: plain words value {at}\n"));
        at += 1;
    }
    text
}

fn values(count: usize) -> String {
    (0..count)
        .map(|at| format!("  item{at:02}: c3ludGhldGljLXNlY3JldC12YWx1ZS0w{at:02}==\n"))
        .collect()
}

/// Lines `1..=last` of `file` read as a range, streamed.
fn streamed_lines(file: &tempfile::NamedTempFile, last: usize) -> String {
    let read = fetch(
        file,
        &query(serde_json::json!({"path":file.path(),"ranges":[{"startLine":1,"endLine":last}]})),
        None,
    );
    assert_eq!(read.status, ReadStatus::Success, "{read:?}");
    read.content.expect("a page")
}

fn first_lines(text: &str, last: usize) -> String {
    text.split_inclusive('\n').take(last).collect()
}

/// S1: a Secret's values go and their names, nested block lines' names too, stay,
/// on both sides of `kind`, as a source held whole shows them.
#[test]
fn a_streamed_secret_hides_its_values_as_a_whole_read_does() {
    let tail = "stringData:\n  config.yaml: |\n    user: admin\n    password: notreal123\n  quoted: \"hunter2-not-real\" # a comment\n";
    for head in [
        format!(
            "apiVersion: v1\nkind: Secret\nmetadata:\n  name: demo\ndata:\n{}{tail}",
            values(6)
        ),
        format!(
            "apiVersion: v1\ndata:\n{}{tail}kind: Secret\nmetadata:\n  name: demo\n",
            values(6)
        ),
    ] {
        let text = yaml_past_whole(&head);
        let file = named(".yaml", &text);
        let last = head.lines().count();
        let whole = first_lines(&held_whole(&text, file.path()), last);
        assert!(
            whole.contains("  item03: [REDACTED-KUBERNETESSECRETS]\n"),
            "{whole}"
        );
        assert!(
            whole.contains("    user: [REDACTED-KUBERNETESSECRETS]\n"),
            "{whole}"
        );
        assert_eq!(
            streamed_lines(&file, last),
            whole,
            "the streamed lines are the whole read's"
        );
    }
}

/// S2: a ConfigMap and a JSON object whose key type is no `kty` hold no secret
/// any walk found: the streamed read masks nothing, as a whole read does.
#[test]
fn a_streamed_source_with_no_secret_masks_nothing() {
    let config = yaml_past_whole(&format!(
        "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: demo\ndata:\n{}",
        values(6)
    ));
    let mut json = String::from(
        "{\n  \"ktx\": \"RSA\",\n  \"d\": \"Fake0Scalar9Not4Real8Value7Long6Enough5Xy\",\n  \"p\": \"Fake0Prime9Not4Real8Value7Long6Enough5XyZ\",\n  \"pad\": [\n",
    );
    let mut at = 0;
    while json.len() < past_whole() {
        json.push_str(&format!("    \"plain words value {at:08}\",\n"));
        at += 1;
    }
    json.push_str("    \"end\"\n  ]\n}\n");
    for (suffix, text) in [(".yaml", config), (".json", json)] {
        let file = named(suffix, &text);
        let whole = first_lines(&held_whole(&text, file.path()), 12);
        assert!(
            !whole.contains("[REDACTED"),
            "the whole read masks nothing: {whole}"
        );
        let read = fetch(
            &file,
            &query(serde_json::json!({"path":file.path(),"ranges":[{"startLine":1,"endLine":12}]})),
            None,
        );
        assert_eq!(read.content.as_deref(), Some(whole.as_str()), "{suffix}");
        assert!(
            !read
                .warnings
                .iter()
                .any(|warning| warning.starts_with("redactedContent")),
            "{suffix}: {:?}",
            read.warnings
        );
    }
}

/// S1: a `matchString` miss on text a placeholder hides says the match ran on
/// redacted text, as a whole read's miss does; a miss in a source with nothing
/// hidden says nothing of it.
#[test]
fn a_streamed_miss_behind_a_placeholder_says_so() {
    // The fact counts the file's 4 hidden values on their 4 lines.
    let hint = crate::tools::source_view::no_match::redacted_miss(4, 4);
    let secret = named(
        ".yaml",
        &yaml_past_whole(&format!(
            "apiVersion: v1\nkind: Secret\ndata:\n{}",
            values(4)
        )),
    );
    let plain = named(
        ".yaml",
        &yaml_past_whole(&format!(
            "apiVersion: v1\nkind: ConfigMap\ndata:\n{}",
            values(4)
        )),
    );
    let miss = |file: &tempfile::NamedTempFile, needle: &str| {
        fetch(
            file,
            &query(serde_json::json!({"path":file.path(),"matchString":[needle]})),
            None,
        )
    };
    let hidden = miss(&secret, "c3ludGhldGljLXNlY3JldC12YWx1ZS0w03");
    assert_eq!(hidden.status, ReadStatus::Empty, "{hidden:?}");
    assert!(
        hidden.hints.iter().any(|shown| *shown == hint),
        "{hidden:?}"
    );
    let absent = miss(&plain, "nothing-here-at-all");
    assert_eq!(absent.status, ReadStatus::Empty, "{absent:?}");
    assert!(
        !absent
            .hints
            .iter()
            .any(|shown| shown.starts_with(crate::tools::source_view::no_match::REDACTED_MISS)),
        "{absent:?}"
    );
}

/// S4b: a page of two ranges counts every placeholder it shows.
#[test]
fn a_ranges_page_counts_every_placeholder_it_shows() {
    let head = format!(
        "apiVersion: v1\nkind: Secret\ndata:\n{}metadata:\n  name: demo\n  note: plain\nstringData:\n{}",
        values(6),
        values(6)
    );
    let text = yaml_past_whole(&head);
    let file = named(".yaml", &text);
    let read = fetch(
        &file,
        &query(
            serde_json::json!({"path":file.path(),"ranges":[{"startLine":4,"endLine":9},{"startLine":14,"endLine":19}]}),
        ),
        None,
    );
    let content = read.content.as_deref().expect("a page");
    let shown = content.matches("[REDACTED-").count();
    assert_eq!(shown, 12, "{content}");
    assert!(
        read.warnings
            .iter()
            .any(|warning| *warning == crate::security::redacted_content_warning(shown)),
        "{:?}",
        read.warnings
    );
}

/// S4c: a refusal and the streamed note state the source's size and the limit
/// exactly, so their comparison reads true however close they are.
#[test]
fn sizes_are_stated_exactly() {
    let text = "q".repeat(99) + "\n";
    let text = text.repeat(past_whole() / 100 + 1);
    let file = named(".txt", &text);
    let len = text.len();
    let refused = fetch(
        &file,
        &query(serde_json::json!({"path":file.path(),"view":"compact"})),
        None,
    );
    let message = refused.error.as_deref().expect("refused");
    assert!(message.contains(&format!("{len} bytes")), "{message}");
    assert!(
        message.contains(&format!("{}-byte", LIMITS.whole_source)),
        "{message}"
    );
    let read = fetch(&file, &query(serde_json::json!({"path":file.path()})), None);
    assert!(
        read.warnings
            .iter()
            .any(|warning| warning.starts_with(&format!("Large source ({len} bytes"))),
        "{:?}",
        read.warnings
    );
}
