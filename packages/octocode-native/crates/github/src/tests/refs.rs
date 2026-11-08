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
        .repository_tag_names(
            "o",
            "r",
            &RequestContext::with_timeout(Duration::from_secs(10), 1 << 20),
        )
        .await
        .expect("every page");
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
        .repository_tag_names(
            "o",
            "r",
            &RequestContext::with_timeout(Duration::from_secs(10), 1 << 20),
        )
        .await
        .expect("every page");
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
