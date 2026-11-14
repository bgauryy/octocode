//! Tag-ref listings read every page by their own page number, as many as
//! the request's deadline admits, whatever origin a forwarded `Link` names.
use super::*;

#[tokio::test]
async fn tag_names_follow_every_linked_page() {
    let server = MockServer::builder().start().await;
    let pages = 25;
    for page in 1..=pages {
        let mut response = ResponseTemplate::new(200)
            .set_body_json(serde_json::json!([{"ref": format!("refs/tags/v{page}")}]));
        if page < pages {
            let next = format!(
                "{}/api/v3/repos/o/r/git/matching-refs/tags?page={}",
                server.uri(),
                page + 1
            );
            response = response.insert_header("link", format!("<{next}>; rel=\"next\""));
        }
        let mock = Mock::given(method("GET")).and(path("/api/v3/repos/o/r/git/matching-refs/tags"));
        mock.and(query_param("page", page.to_string()))
            .and(query_param("per_page", "100"))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    let endpoint = mock_endpoint(&server);
    let names = anonymous_transport(&endpoint)
        .repository_tag_refs(
            "o",
            "r",
            "",
            &RequestContext::with_timeout(Duration::from_secs(10), 1 << 20),
        )
        .await
        .expect("every page")
        .into_iter()
        .map(|tag| tag.name)
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        (1..=pages)
            .map(|page| format!("v{page}"))
            .collect::<Vec<_>>()
    );
}

/// Behind a proxy that forwards GitHub's own `Link` (naming another host),
/// the tags still come whole, in order, each page asked exactly once.
#[tokio::test]
async fn tag_names_behind_a_proxy_read_each_page_once_in_order() {
    let server = MockServer::builder().start().await;
    let pages = 3;
    for page in 1..=pages {
        let mut response = ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"ref": format!("refs/tags/a{page}")},
            {"ref": format!("refs/tags/b{page}")}
        ]));
        if page < pages {
            let next = format!(
                "<https://api.github.com/repos/o/r/git/matching-refs/tags?page={}>; rel=\"next\"",
                page + 1
            );
            response = response.insert_header("link", next);
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/o/r/git/matching-refs/tags"))
            .and(query_param("page", page.to_string()))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    let endpoint = mock_endpoint(&server);
    let names = anonymous_transport(&endpoint)
        .repository_tag_refs(
            "o",
            "r",
            "",
            &RequestContext::with_timeout(Duration::from_secs(10), 1 << 20),
        )
        .await
        .expect("every page")
        .into_iter()
        .map(|tag| tag.name)
        .collect::<Vec<_>>();
    assert_eq!(names, ["a1", "b1", "a2", "b2", "a3", "b3"]);
    let seen = server.received_requests().await.expect("requests");
    assert_eq!(seen.len(), pages, "one request per page");
    for (index, request) in seen.iter().enumerate() {
        assert_eq!(
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == "page")
                .map(|(_, v)| v.into_owned()),
            Some((index + 1).to_string()),
            "{}",
            request.url
        );
    }
}

/// A ref listing paged by its caller behind such a proxy: each page states
/// `has_more` from the forwarded link and is asked exactly once, by the
/// configured origin.
#[tokio::test]
async fn ref_pages_behind_a_proxy_state_more_and_are_asked_once() {
    let server = MockServer::builder().start().await;
    let pages = 3;
    for page in 1..=pages {
        let mut response = ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"name": format!("t{page}"), "commit": {"sha": format!("{page:040}")}}
        ]));
        if page < pages {
            let next = format!(
                "<https://api.github.com/repos/o/r/tags?page={}&per_page=1>; rel=\"next\"",
                page + 1
            );
            response = response.insert_header("link", next);
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/o/r/tags"))
            .and(query_param("page", page.to_string()))
            .and(query_param("per_page", "1"))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    let endpoint = mock_endpoint(&server);
    let transport = anonymous_transport(&endpoint);
    let context = RequestContext::with_timeout(Duration::from_secs(10), 1 << 20);
    let mut seen = Vec::new();
    for page in 1..=pages {
        let listed = transport
            .repository_refs("o", "r", RefKind::Tags, page, 1, &context)
            .await
            .expect("page");
        assert_eq!(listed.has_more, page < pages, "page {page}");
        seen.extend(listed.refs.into_iter().map(|row| row.name));
    }
    assert_eq!(seen, ["t1", "t2", "t3"]);
    assert_eq!(
        server.received_requests().await.expect("requests").len(),
        pages,
        "one request per page"
    );
}

/// A tag ref keeps the object its ref names: the tagged object of a
/// lightweight tag, the tag object of an annotated one.
#[tokio::test]
async fn tag_refs_keep_the_object_each_ref_names() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/git/matching-refs/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"ref": "refs/tags/rel/1.0", "object": {"sha": "c0ffee", "type": "commit"}},
            {"ref": "refs/tags/v1.0", "object": {"sha": "7a9", "type": "tag"}},
            {"ref": "refs/tags/bare"}
        ])))
        .mount(&server)
        .await;
    let endpoint = mock_endpoint(&server);
    let tags = anonymous_transport(&endpoint)
        .repository_tag_refs(
            "o",
            "r",
            "",
            &RequestContext::with_timeout(Duration::from_secs(10), 1 << 20),
        )
        .await
        .expect("tag refs");
    assert_eq!(
        tags,
        [
            TagRef {
                name: "rel/1.0".into(),
                object: Some(TagObject::Tagged("c0ffee".into()))
            },
            TagRef {
                name: "v1.0".into(),
                object: Some(TagObject::Annotated("7a9".into()))
            },
            TagRef {
                name: "bare".into(),
                object: None
            },
        ]
    );
}

/// A prefix is GitHub's to answer: the listing asks for the refs under it
/// (one release line reads only its own refs, a path prefix keeps its `/`),
/// and returns only names that start with it.
#[tokio::test]
async fn a_tag_prefix_is_asked_of_github_and_bounds_the_names() {
    let server = MockServer::builder().start().await;
    for (route, names) in [
        (
            "/api/v3/repos/o/r/git/matching-refs/tags/v14.",
            &["v14.0.0", "v14.1.0"][..],
        ),
        (
            "/api/v3/repos/o/r/git/matching-refs/tags/release/",
            &["release/1"][..],
        ),
        (
            "/api/v3/repos/o/r/git/matching-refs/tags",
            &["v14.0.0", "v13.0.0", "../x"][..],
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                names
                    .iter()
                    .map(|name| serde_json::json!({"ref": format!("refs/tags/{name}"), "object": {"sha": "c0ffee", "type": "commit"}}))
                    .collect::<Vec<_>>(),
            ))
            .mount(&server)
            .await;
    }
    let transport = anonymous_transport(&mock_endpoint(&server));
    let context = RequestContext::with_timeout(Duration::from_secs(10), 1 << 20);
    for (prefix, route) in [
        ("v14.", "/api/v3/repos/o/r/git/matching-refs/tags/v14."),
        (
            "release/",
            "/api/v3/repos/o/r/git/matching-refs/tags/release/",
        ),
        // A route segment GitHub cannot carry lists every tag, still bounded.
        ("../", "/api/v3/repos/o/r/git/matching-refs/tags"),
    ] {
        let names = transport
            .repository_tag_refs("o", "r", prefix, &context)
            .await
            .expect("tag refs");
        assert!(!names.is_empty(), "{prefix}");
        assert!(
            names.iter().all(|tag| tag.name.starts_with(prefix)),
            "{prefix}: {names:?}"
        );
        let asked = server.received_requests().await.expect("requests");
        assert_eq!(
            asked
                .last()
                .map(|request| request.url.path().to_owned())
                .as_deref(),
            Some(route)
        );
    }
}

/// A GraphQL responder for `object(oid:$tN)` reads: each asked tag object's
/// answer from `targets` (`None`: GitHub names no object).
fn tag_targets_answer(
    targets: Vec<(String, Option<serde_json::Value>)>,
) -> impl Fn(&wiremock::Request) -> ResponseTemplate {
    move |request: &wiremock::Request| {
        let body: serde_json::Value = serde_json::from_slice(&request.body).expect("graphql body");
        let mut repository = serde_json::Map::new();
        for (key, value) in body["variables"].as_object().expect("variables") {
            if !key
                .strip_prefix('t')
                .is_some_and(|at| at.parse::<usize>().is_ok())
            {
                continue;
            }
            let answer = targets
                .iter()
                .find(|(object, _)| value == object)
                .and_then(|(_, answer)| answer.clone())
                .unwrap_or(serde_json::Value::Null);
            repository.insert(key.clone(), answer);
        }
        ResponseTemplate::new(200)
            .set_body_json(serde_json::json!({"data": {"repository": repository}}))
    }
}

fn tagged(kind: &str, oid: &str) -> Option<serde_json::Value> {
    Some(serde_json::json!({"target": {"__typename": kind, "oid": oid}}))
}

/// A page of tags costs one GraphQL read per step of tag nesting, never one
/// read per tag: a lightweight tag names its commit as listed, every
/// annotated tag's target is read together, and a tag of a tag is followed
/// to its commit. Results stay in the tags' order.
#[tokio::test]
async fn annotated_tags_read_their_commits_together() {
    let server = MockServer::builder().start().await;
    let (commit, outer, inner, annotated) = (
        "c".repeat(40),
        "a".repeat(40),
        "b".repeat(40),
        "d".repeat(40),
    );
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(tag_targets_answer(vec![
            (outer.clone(), tagged("Tag", &inner)),
            (inner.clone(), tagged("Commit", &commit)),
            (annotated.clone(), tagged("Commit", &"e".repeat(40))),
        ]))
        .mount(&server)
        .await;
    let transport = anonymous_transport(&mock_endpoint(&server));
    let context = with_token("t", Duration::from_secs(10), 1 << 20);
    let tags = [
        TagRef {
            name: "light".into(),
            object: Some(TagObject::Tagged("f".repeat(40))),
        },
        TagRef {
            name: "nested".into(),
            object: Some(TagObject::Annotated(outer.clone())),
        },
        TagRef {
            name: "annotated".into(),
            object: Some(TagObject::Annotated(annotated.clone())),
        },
    ];
    let commits = transport
        .tag_commits("o", "r", &tags, &context)
        .await
        .expect("commits");
    assert_eq!(
        commits,
        [Some("f".repeat(40)), Some(commit), Some("e".repeat(40))]
    );
    let asked = server.received_requests().await.expect("requests");
    assert!(
        asked
            .iter()
            .all(|request| request.url.path() == "/api/graphql"),
        "{asked:?}"
    );
    assert_eq!(asked.len(), 2, "one read per nesting step");
}

/// Without GraphQL (an anonymous request), and for any object a GraphQL
/// read leaves unanswered, each tag object is read by REST: the same
/// commits, the transport choice no part of the answer. An object GitHub no
/// longer has leaves its tag without a commit.
#[tokio::test]
async fn tag_objects_graphql_cannot_answer_are_read_by_rest() {
    let server = MockServer::builder().start().await;
    let (answered, unanswered, gone) = ("a".repeat(40), "b".repeat(40), "9".repeat(40));
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(tag_targets_answer(vec![(
            answered.clone(),
            tagged("Commit", &"1".repeat(40)),
        )]))
        .mount(&server)
        .await;
    for (object, commit) in [(&answered, "1".repeat(40)), (&unanswered, "2".repeat(40))] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/o/r/git/tags/{object}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "object": {"sha": commit, "type": "commit"}
            })))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/o/r/git/tags/{gone}")))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(serde_json::json!({"message": "Not Found"})),
        )
        .mount(&server)
        .await;
    let transport = anonymous_transport(&mock_endpoint(&server));
    let tags = [answered.clone(), unanswered.clone(), gone].map(|object| TagRef {
        name: object.clone(),
        object: Some(TagObject::Annotated(object)),
    });
    let expected = [Some("1".repeat(40)), Some("2".repeat(40)), None];
    // (request, GraphQL reads, REST reads): every object by REST without
    // GraphQL; with it, one read for all, REST for the two it left.
    for (context, graphql, rest) in [
        (
            RequestContext::with_timeout(Duration::from_secs(10), 1 << 20),
            0,
            3,
        ),
        (with_token("t", Duration::from_secs(10), 1 << 20), 1, 2),
    ] {
        let before = server.received_requests().await.expect("requests").len();
        let commits = transport
            .tag_commits("o", "r", &tags, &context)
            .await
            .expect("commits");
        assert_eq!(commits, expected);
        let asked = server.received_requests().await.expect("requests");
        let asked = &asked[before..];
        let posts = asked
            .iter()
            .filter(|request| request.url.path() == "/api/graphql")
            .count();
        assert_eq!(posts, graphql, "{asked:?}");
        assert_eq!(
            asked.len() - posts,
            rest,
            "REST reads only what GraphQL left"
        );
    }
}
