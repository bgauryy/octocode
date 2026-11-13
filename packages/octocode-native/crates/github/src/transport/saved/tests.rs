use super::*;

fn saved(headers: &[(&str, &str)]) -> SavedAnswer {
    SavedAnswer {
        answer: ProviderAnswer {
            request: "GET x".into(),
            status: 200,
            headers: headers
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
            body: "{}".into(),
            next: None,
            not_found: None,
        },
        observed_ms: 1_000_000,
        redirected: false,
    }
}

/// The table: what each kind of answer does at an age. Ages are relative
/// to each answer's own stated lifetime, never a copied constant.
#[test]
fn the_origin_decides_how_an_answer_may_be_reused() {
    let observed = 1_000_000;
    let ask = |etag: &str| Reuse::Ask(IF_NONE_MATCH, etag.to_owned());
    let private = [("Cache-Control", "private, max-age=60"), ("ETag", "\"e\"")];
    assert_eq!(
        saved(&private).reuse(false, None, observed + 59_000),
        Reuse::Fresh
    );
    assert_eq!(
        saved(&private).reuse(false, None, observed + 60_000),
        ask("\"e\"")
    );
    let no_cache = [("Cache-Control", "no-cache"), ("ETag", "\"e\"")];
    assert_eq!(saved(&no_cache).reuse(false, None, observed), ask("\"e\""));
    let aged = [("cache-control", "public, max-age=300"), ("age", "290")];
    assert_eq!(
        saved(&aged).reuse(false, None, observed + 9_000),
        Reuse::Fresh
    );
    assert_eq!(
        saved(&aged).reuse(false, None, observed + 10_000),
        Reuse::Spent
    );
    let dated = [("last-modified", "Mon, 02 Nov 2026 00:00:00 GMT")];
    assert_eq!(
        saved(&dated).reuse(false, None, observed),
        Reuse::Ask(IF_MODIFIED_SINCE, dated[0].1.to_owned())
    );
    let window = Duration::from_secs(30);
    let bare = [];
    assert_eq!(
        saved(&bare).reuse(false, Some(window), observed + 29_000),
        Reuse::Window
    );
    assert_eq!(
        saved(&bare).reuse(false, Some(window), observed + 30_000),
        Reuse::Spent
    );
    assert_eq!(saved(&bare).reuse(false, None, observed), Reuse::Spent);
    // A validator always wins over the window: it can tell the truth.
    let validated = [("etag", "\"e\"")];
    assert_eq!(
        saved(&validated).reuse(false, Some(window), observed),
        ask("\"e\"")
    );
    // Pinned needs nothing, however old.
    assert_eq!(
        saved(&[]).reuse(true, None, u64::MAX),
        Reuse::Pinned,
        "a pinned answer never ages"
    );
}

/// A redirected answer always asks, so the redirect itself is seen again.
#[test]
fn a_redirected_answer_is_never_reused_without_asking() {
    let mut answer = saved(&[("cache-control", "max-age=60"), ("etag", "\"e\"")]);
    answer.redirected = true;
    let ask = Reuse::Ask(IF_NONE_MATCH, "\"e\"".into());
    assert_eq!(answer.reuse(true, None, answer.observed_ms), ask);
    assert_eq!(answer.reuse(false, None, answer.observed_ms), ask);
    let mut bare = saved(&[]);
    bare.redirected = true;
    assert_eq!(
        bare.reuse(true, Some(Duration::from_secs(1)), bare.observed_ms),
        Reuse::Spent
    );
}

#[test]
fn only_an_answer_something_can_vouch_for_is_kept() {
    let headers = |pairs: &[(&str, &str)]| {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect::<Vec<_>>()
    };
    let window = Some(Duration::from_secs(1));
    assert!(!worth_keeping(&headers(&[]), false, None));
    assert!(worth_keeping(&headers(&[]), true, None));
    assert!(worth_keeping(&headers(&[]), false, window));
    assert!(worth_keeping(&headers(&[("etag", "\"e\"")]), false, None));
    assert!(worth_keeping(
        &headers(&[("cache-control", "max-age=1")]),
        false,
        None
    ));
    assert!(!worth_keeping(
        &headers(&[("cache-control", "no-store"), ("etag", "\"e\"")]),
        true,
        window
    ));
}

#[test]
fn explicit_cache_directives_override_immutable_and_window_reuse() {
    let window = Some(Duration::from_secs(30));
    for directive in ["no-cache", "no-cache=\"etag\"", "No-Cache"] {
        let answer = saved(&[("cache-control", directive)]);
        assert_eq!(
            answer.reuse(false, window, answer.observed_ms),
            Reuse::Spent
        );
        assert_eq!(answer.reuse(true, window, answer.observed_ms), Reuse::Spent);
        let answer = saved(&[("cache-control", directive), ("etag", "\"v1\"")]);
        assert_eq!(
            answer.reuse(true, window, answer.observed_ms),
            Reuse::Ask(IF_NONE_MATCH, "\"v1\"".into())
        );
    }
    let answer = saved(&[("cache-control", "no-store"), ("etag", "\"v1\"")]);
    assert_eq!(answer.reuse(true, window, answer.observed_ms), Reuse::Spent);
    let answer = saved(&[("cache-control", "max-age=0")]);
    assert_eq!(
        answer.reuse(false, window, answer.observed_ms),
        Reuse::Spent
    );
    let answer = saved(&[("cache-control", "max-age=0, must-revalidate")]);
    assert_eq!(answer.reuse(true, window, answer.observed_ms), Reuse::Spent);
    let answer = saved(&[
        ("cache-control", "max-age=0, must-revalidate"),
        ("etag", "\"v1\""),
    ]);
    assert_eq!(
        answer.reuse(true, window, answer.observed_ms),
        Reuse::Ask(IF_NONE_MATCH, "\"v1\"".into())
    );
    let answer = saved(&[
        ("Cache-Control", "max-age=60"),
        ("cache-control", "no-cache"),
    ]);
    assert_eq!(
        answer.reuse(false, window, answer.observed_ms),
        Reuse::Spent
    );
}

#[test]
fn cache_policy_keeps_repeated_fields_and_quoted_directive_values() {
    let answer = saved(&[("cache-control", "max-age=\"60\"")]);
    assert_eq!(answer.reuse(false, None, answer.observed_ms), Reuse::Fresh);
    let answer = saved(&[
        ("cache-control", "max-age=60"),
        ("cache-control", "max-age=0"),
    ]);
    assert_eq!(
        answer.reuse(false, Some(Duration::from_secs(60)), answer.observed_ms),
        Reuse::Spent
    );
    let mut answer = saved(&[("etag", "\"v1\"")]);
    answer.confirm(
        &[
            ("cache-control".into(), "max-age=60".into()),
            ("Cache-Control".into(), "no-store".into()),
        ],
        answer.observed_ms,
    );
    assert!(!worth_keeping(
        &answer.answer.headers,
        true,
        Some(Duration::from_secs(60))
    ));
    assert_eq!(answer.reuse(true, None, answer.observed_ms), Reuse::Spent);
    let answer = saved(&[("cache-control", "extension=\"a,no-store,b\", max-age=60")]);
    assert_eq!(answer.reuse(false, None, answer.observed_ms), Reuse::Fresh);
}

#[test]
fn a_confirmation_restarts_the_answer_and_keeps_its_body() {
    let mut answer = saved(&[("etag", "\"old\""), ("x-kept", "1")]);
    let confirmed = [
        ("ETag".to_owned(), "\"new\"".to_owned()),
        ("Cache-Control".to_owned(), "max-age=9".to_owned()),
        ("x-ratelimit-remaining".to_owned(), "1".to_owned()),
    ];
    answer.confirm(&confirmed, 5);
    assert_eq!(answer.observed_ms, 5);
    assert_eq!(answer.answer.body, "{}");
    let value = |name: &str| header(&answer.answer.headers, name).map(str::to_owned);
    assert_eq!(value("etag").as_deref(), Some("\"new\""));
    assert_eq!(value("cache-control").as_deref(), Some("max-age=9"));
    assert_eq!(value("x-kept").as_deref(), Some("1"));
    assert_eq!(value("x-ratelimit-remaining"), None);
}
