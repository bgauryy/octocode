//! A source too large for the scanner to judge whole: only what the source's
//! structure says withholds a row. Every value below is fake.
use super::*;

/// The limits these searches judge at, and a source past what is judged whole.
const LIMITS: octocode_runtime_base::security::SourceLimits =
    octocode_runtime_base::security::SourceLimits::SMALL;
const OVER_THE_SCANNER: usize = LIMITS.whole_source + LIMITS.whole_source / 10;

fn values(name: &str, text: &str, needle: &str) -> Vec<String> {
    let root = tempfile::tempdir().expect("fixture directory");
    let path = root.path().join(name);
    fs::write(&path, text).expect("fixture");
    let (policy, _) = policy_for(root.path());
    let security = ContentSecurity::with_limits(LIMITS);
    let request = ls_query(
        serde_json::json!({"path": path.to_string_lossy(), "matchString": [needle], "limit": 20}),
        None,
    );
    let found = search_in(&request, &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("serialize");
    // No file row: nothing matched.
    found["files"][0]["matches"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row["value"].as_str().expect("a value").to_owned())
        .collect()
}

/// `kty` by chance in a long base64 string says nothing of the rows beside it.
#[test]
fn a_literal_inside_a_blob_withholds_nothing() {
    let blob = "QUJDkty0REVGRwWVo9".repeat(OVER_THE_SCANNER / 18);
    let text = format!(
        "{{\"font\":\"{blob}\",\n\"name1\": \"ordinary words one\",\n\"name2\": \"ordinary words two\"\n}}\n"
    );
    assert_eq!(
        values("bundle.json", &text, "ordinary words"),
        vec![
            "\"name1\": \"ordinary words one\",",
            "\"name2\": \"ordinary words two\""
        ]
    );
}

/// `kty` as a word, a Rust path or a bare string names no JSON Web Key.
#[test]
fn a_kty_that_is_no_key_withholds_nothing() {
    for spelling in ["the kty field", "Kty::Rsa is kty::Rsa", "\"kty\" is a word"] {
        let filler = format!("{spelling} here\n").repeat(OVER_THE_SCANNER / 20);
        let text = format!("{filler}\"name\": \"ordinary words\"\n");
        assert_eq!(
            values("notes.txt", &text, "ordinary words"),
            vec!["\"name\": \"ordinary words\""],
            "{spelling}"
        );
    }
}

/// A log that refers to a Secret holds no value of one.
#[test]
fn a_reference_to_a_secret_withholds_nothing() {
    let line =
        "{\"level\":\"info\",\"ref\":{\"kind\":\"Secret\",\"name\":\"x\"},\"user\":\"svc\"}\n";
    let mut text = line.repeat(OVER_THE_SCANNER / line.len());
    text.push_str("{\"level\":\"info\",\"msg\":\"ordinary words\"}\n");
    assert_eq!(
        values("events.log", &text, "ordinary words"),
        vec!["{\"level\":\"info\",\"msg\":\"ordinary words\"}"]
    );
}
