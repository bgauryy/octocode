use super::*;

#[test]
fn redirects_resolve_relative_locations_and_keep_the_fragment() {
    let fake = Fake::default()
        .status("https://a.example.org/old", 301, Some("/moved"))
        .status(
            "https://a.example.org/moved",
            308,
            Some("https://b.example.org/guide"),
        )
        .page("https://b.example.org/guide", "text/html", PAGE);
    let selection = run(&fake, "https://a.example.org/old#faq", 1_000).unwrap();
    assert_eq!(selection.url.as_str(), "https://b.example.org/guide#faq");
    assert_eq!(selection.text, "## FAQ\nNone.\n");
    assert_eq!(
        *fake.requested.borrow(),
        [
            "https://a.example.org/old",
            "https://a.example.org/moved",
            "https://b.example.org/guide"
        ]
    );
}

#[test]
fn a_redirect_is_rechecked_as_https_without_credentials() {
    let fake = Fake::default()
        .status(
            "https://a.example.org/down",
            302,
            Some("http://a.example.org/plain"),
        )
        .status(
            "https://a.example.org/creds",
            302,
            Some("https://u:p@a.example.org/x"),
        );
    for path in ["down", "creds"] {
        let error = run(&fake, &format!("https://a.example.org/{path}"), 1_000)
            .err()
            .unwrap();
        assert_eq!(error.code, ErrorCode::SecurityValidationFailed, "{path}");
    }
    assert_eq!(
        fake.requested.borrow().len(),
        2,
        "the refused hop is never requested"
    );
}

#[test]
fn at_most_five_redirects_are_followed() {
    let mut fake = Fake::default();
    let hops: Vec<String> = (0..=6)
        .map(|n| format!("https://r.example.org/{n}"))
        .collect();
    let locations: Vec<&'static str> = (1..=6)
        .map(|n| &*Box::leak(format!("/{n}").into_boxed_str()))
        .collect();
    for n in 0..6 {
        fake = fake.status(&hops[n], 302, Some(locations[n]));
    }
    fake = fake.page(&hops[5], "text/plain", "five");
    assert_eq!(
        run(&fake, &hops[0], 100).unwrap().text,
        "five\n",
        "five redirects are followed"
    );
    let mut fake = Fake::default();
    for n in 0..6 {
        fake = fake.status(&hops[n], 302, Some(locations[n]));
    }
    fake = fake.page(&hops[6], "text/plain", "six");
    let error = run(&fake, &hops[0], 100).err().unwrap();
    assert_eq!(error.code, ErrorCode::ClassificationContextFailed);
    assert!(error.message.contains("more than 5"), "{}", error.message);
    assert_eq!(fake.requested.borrow().len(), 6);
}

#[test]
fn http_failures_and_redirects_without_location_fail_the_capture() {
    let fake = Fake::default()
        .status("https://a.example.org/private", 403, None)
        .status("https://a.example.org/nowhere", 302, None);
    let forbidden = run(&fake, "https://a.example.org/private", 100)
        .err()
        .unwrap();
    assert_eq!(forbidden.code, ErrorCode::ClassificationContextFailed);
    assert!(
        forbidden.message.contains("HTTP 403"),
        "{}",
        forbidden.message
    );
    let nowhere = run(&fake, "https://a.example.org/nowhere", 100)
        .err()
        .unwrap();
    assert_eq!(nowhere.code, ErrorCode::ClassificationContextFailed);
    assert!(nowhere.message.contains("Location"), "{}", nowhere.message);
}
