//! Real provider answers from `test-utils/provider-answers`, the store the
//! end-to-end proxy serves too. A fixture written by a parser's author holds what
//! the parser reads; these hold what the provider sends. Refresh:
//! `dev.mjs answers:record`.
use super::RouteHttp;
use base64::Engine as _;
use serde_json::Value;
use std::path::PathBuf;

fn store() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../test-utils/provider-answers")
}

/// One recording, by its id: `osv/advisory-ghsa`.
pub(crate) fn recording(case: &str) -> Value {
    let path = store().join(format!("{case}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The JSON answer of the first exchange whose URL holds `url_part`.
pub(crate) fn answer(case: &str, url_part: &str) -> Value {
    exchanges(&recording(case))
        .iter()
        .find(|exchange| url_of(exchange).contains(url_part))
        .map(|exchange| exchange["body"].clone())
        .filter(|body| body.is_object() || body.is_array())
        .unwrap_or_else(|| panic!("{case} holds no JSON answer for {url_part}"))
}

fn exchanges(recording: &Value) -> &[Value] {
    recording["exchanges"].as_array().map_or(&[], Vec::as_slice)
}

fn url_of(exchange: &Value) -> &str {
    exchange["request"]["url"].as_str().unwrap_or_default()
}

fn body_of(exchange: &Value) -> Vec<u8> {
    if let Some(encoded) = exchange["base64"].as_str() {
        return base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .expect("recorded base64");
    }
    match &exchange["body"] {
        Value::String(text) => text.as_bytes().to_vec(),
        json => serde_json::to_vec(json).expect("recorded JSON"),
    }
}

impl RouteHttp {
    /// Answers every GET the recording holds, by `path?query`, and its POSTs in
    /// the order they were recorded.
    pub(crate) fn recorded(case: &str) -> Self {
        let recording = recording(case);
        let mut http = Self::new();
        for exchange in exchanges(&recording) {
            let url = url::Url::parse(url_of(exchange)).expect("recorded URL");
            let status = exchange["status"].as_u64().expect("recorded status");
            let status = u16::try_from(status).expect("an HTTP status");
            if exchange["request"]["method"] == "POST" {
                http.posts.push((status, body_of(exchange)));
                continue;
            }
            let target = match url.query() {
                Some(query) => format!("{}?{query}", url.path()),
                None => url.path().to_owned(),
            };
            http.routes.push(super::Route {
                target,
                accept: None,
                status,
                body: body_of(exchange),
            });
        }
        http
    }
}

fn every_recording() -> Vec<PathBuf> {
    let providers = std::fs::read_dir(store()).expect("the store");
    let mut files = Vec::new();
    for provider in providers {
        let dir = provider.expect("a provider").path();
        if !dir.is_dir() {
            continue;
        }
        for file in std::fs::read_dir(dir).expect("a provider's recordings") {
            files.push(file.expect("a recording").path());
        }
    }
    files
}

fn strings<'a>(value: &'a Value, found: &mut Vec<&'a str>) {
    match value {
        Value::String(text) => found.push(text),
        Value::Array(items) => items.iter().for_each(|item| strings(item, found)),
        Value::Object(fields) => fields.values().for_each(|item| strings(item, found)),
        _ => {}
    }
}

/// Checked in, so nothing in them may look like a credential to the detector
/// every tool output passes through, whichever file a read would name.
#[test]
fn no_recording_holds_what_the_secret_detector_redacts() {
    let security = crate::security::ContentSecurity::new();
    let files = every_recording();
    assert!(!files.is_empty(), "the store is empty");
    for file in files {
        let text = std::fs::read_to_string(&file).expect("a recording");
        let recording: Value = serde_json::from_str(&text).expect("recording JSON");
        let mut leaves = Vec::new();
        strings(&recording, &mut leaves);
        for leaf in leaves {
            if let Some(redacted) = security.sanitize_stored_leaf(leaf, None) {
                let cut = leaf
                    .char_indices()
                    .nth(120)
                    .map_or(leaf.len(), |(at, _)| at);
                panic!(
                    "{}: the detector redacts `{}`, as `{}`",
                    file.display(),
                    &leaf[..cut],
                    redacted.chars().take(120).collect::<String>()
                );
            }
        }
    }
}
