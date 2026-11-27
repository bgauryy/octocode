use super::*;
use reqwest::header::ACCEPT;

#[test]
fn request_headers_have_one_order_independent_secret_free_identity() {
    let mut left = RequestSpec::get(Url::parse("https://api.github.com/repos/a/b").unwrap());
    left.headers
        .insert("x-first", HeaderValue::from_static("private-first-value"));
    left.headers
        .insert("x-second", HeaderValue::from_static("private-second-value"));
    let mut right = RequestSpec::get(left.url.clone());
    right
        .headers
        .insert("x-second", HeaderValue::from_static("private-second-value"));
    right
        .headers
        .insert("x-first", HeaderValue::from_static("private-first-value"));
    let key = request_key(&left);
    assert_eq!(key, request_key(&right));
    assert!(!key.contains("private-first-value") && !key.contains("private-second-value"));
    right
        .headers
        .insert("x-second", HeaderValue::from_static("different-value"));
    assert_ne!(key, request_key(&right));
}

#[test]
fn request_header_identity_keeps_repeated_values_and_non_text_bytes() {
    let mut left = RequestSpec::get(Url::parse("https://api.github.com/repos/a/b").unwrap());
    left.headers
        .append(ACCEPT, HeaderValue::from_static("application/json"));
    left.headers
        .append(ACCEPT, HeaderValue::from_static("text/plain"));
    let mut right = left.clone();
    right.headers.remove(ACCEPT);
    right
        .headers
        .append(ACCEPT, HeaderValue::from_static("application/json"));
    right
        .headers
        .append(ACCEPT, HeaderValue::from_static("application/xml"));
    assert_ne!(request_key(&left), request_key(&right));
    right.headers.remove(ACCEPT);
    right
        .headers
        .append(ACCEPT, HeaderValue::from_static("text/plain"));
    right
        .headers
        .append(ACCEPT, HeaderValue::from_static("application/json"));
    assert_ne!(request_key(&left), request_key(&right));

    let mut right = left.clone();
    left.headers
        .insert("x-opaque", HeaderValue::from_bytes(&[0x80]).unwrap());
    right
        .headers
        .insert("x-opaque", HeaderValue::from_bytes(&[0x81]).unwrap());
    assert_ne!(request_key(&left), request_key(&right));
}

#[test]
fn request_header_identity_keeps_method_url_and_file_projection() {
    let spec =
        RequestSpec::get(Url::parse("https://api.github.com/repos/a/b/git/blobs/sha").unwrap())
            .reading_file("nested/a\nb.rs");
    let key = request_key(&spec);
    assert_eq!(key.split_whitespace().next(), Some("GET"));
    assert_eq!(key.split_whitespace().nth(1), Some(spec.url.as_str()));
    assert_eq!(requested_file(&key), Some("nested/a\nb.rs"));
}

#[test]
fn forgetting_a_replayed_answer_requires_a_fresh_read() {
    let spec = RequestSpec::get(Url::parse("https://api.github.com/repos/a/b").expect("url"));
    let page = ResponsePage {
        status: 200,
        headers: HeaderMap::new(),
        body: Bytes::from_static(b"{\"id\":1}"),
        next: None,
        redirected: false,
    };
    let saved = ProviderEvidence {
        answers: vec![answer_of(&request_key(&spec), &page).expect("answer")],
        notes: BTreeMap::new(),
    };
    let journal = Journal::resuming(Arc::new(saved));
    assert!(journal.answers(&spec));
    assert!(journal.replay(&spec).expect("replay").is_ok());
    journal.forget(&spec);
    assert!(journal.evidence().is_empty());
    assert!(
        !journal.answers(&spec),
        "forgotten evidence cannot answer again"
    );
    assert!(
        journal.replay(&spec).is_none(),
        "a retry must read the origin"
    );
}

#[test]
fn replacing_and_forgetting_answers_preserve_the_last_request_order() {
    let journal = Journal::default();
    let page = |value: &'static [u8]| ResponsePage {
        status: 200,
        headers: HeaderMap::new(),
        body: Bytes::from_static(value),
        next: None,
        redirected: false,
    };
    let first = RequestSpec::get(Url::parse("https://api.github.com/repos/a/first").expect("url"));
    let second =
        RequestSpec::get(Url::parse("https://api.github.com/repos/a/second").expect("url"));
    journal.record_page(&first, &page(b"old"));
    journal.record_page(&second, &page(b"second"));
    journal.record_page(&first, &page(b"new"));
    let evidence = journal.evidence();
    assert_eq!(evidence.answers.len(), 2);
    assert_eq!(evidence.answers[0].request, request_key(&second));
    assert_eq!(evidence.answers[1].request, request_key(&first));
    assert_eq!(evidence.answers[1].body, "new");

    journal.forget(&second);
    journal.forget(&second);
    journal.record_page(&second, &page(b"returned"));
    let evidence = journal.evidence();
    assert_eq!(evidence.answers.len(), 2);
    assert_eq!(evidence.answers[0].request, request_key(&first));
    assert_eq!(evidence.answers[1].request, request_key(&second));
    assert_eq!(evidence.answers[1].body, "returned");
}

/// A body written and read as JSON is the JSON string a `String` was, and
/// every hand-off from a page to an answer and back shares its bytes.
#[test]
fn a_body_is_a_shared_json_string() {
    let text = "{\"a\":\"\u{e9}\\n\"}\n";
    let body = AnswerBody::from(text);
    let json = serde_json::to_string(&body).expect("json");
    assert_eq!(json, serde_json::to_string(text).expect("json"));
    assert_eq!(
        serde_json::from_str::<AnswerBody>(&json).expect("body"),
        text
    );
    assert!(AnswerBody::from_utf8(Bytes::from_static(&[0xff])).is_none());

    let page = ResponsePage {
        status: 200,
        headers: HeaderMap::new(),
        body: Bytes::copy_from_slice(text.as_bytes()),
        next: None,
        redirected: false,
    };
    let answer = answer_of("GET x", &page).expect("text");
    assert_eq!(answer.body.as_bytes().as_ptr(), page.body.as_ptr());
    assert_eq!(page_of(&answer).body.as_ptr(), page.body.as_ptr());
}
