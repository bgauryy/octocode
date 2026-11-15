//! Every response-selecting request header participates in cached and replayed identity.
use super::*;

fn anonymous() -> RequestContext {
    RequestContext::with_timeout(Duration::from_secs(5), 1024)
}

fn variant(endpoint: &GitHubEndpoint, header: &'static str, value: &'static str) -> RequestSpec {
    let mut spec = RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
    spec.headers.insert(header, value.parse().unwrap());
    spec
}

#[tokio::test]
async fn all_request_headers_partition_saved_answers_including_explicit_authorization() {
    for header in ["x-variant", "authorization"] {
        let server = MockServer::start().await;
        Mock::given(path("/api/v3/repos/a/b"))
            .respond_with(move |request: &Request| {
                ResponseTemplate::new(200)
                    .insert_header("cache-control", "private, max-age=60")
                    .insert_header("vary", header)
                    .set_body_string(request.headers[header].to_str().unwrap())
            })
            .mount(&server)
            .await;
        let (transport, endpoint, _) = keeping(&server);
        for value in ["first", "second", "first"] {
            let page = transport
                .execute(variant(&endpoint, header, value), &anonymous())
                .await
                .unwrap();
            assert_eq!(page.body.as_ref(), value.as_bytes(), "{header}");
        }
        assert_eq!(requests(&server).await.len(), 2, "{header}");
    }
}

#[tokio::test]
async fn header_variants_remain_distinct_in_no_store_journals_and_resume() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(|request: &Request| {
            ResponseTemplate::new(200)
                .insert_header("cache-control", "no-store")
                .set_body_string(request.headers["x-variant"].to_str().unwrap())
        })
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let context = anonymous();
    for value in ["first", "second"] {
        let page = transport
            .execute(variant(&endpoint, "x-variant", value), &context)
            .await
            .unwrap();
        assert_eq!(page.body.as_ref(), value.as_bytes());
    }
    let evidence = Arc::new(context.evidence());
    assert_eq!(
        evidence.answers.len(),
        2,
        "each representation owns its evidence"
    );
    let resumed = anonymous().resuming(evidence);
    for value in ["first", "second"] {
        let page = transport
            .execute(variant(&endpoint, "x-variant", value), &resumed)
            .await
            .unwrap();
        assert_eq!(page.body.as_ref(), value.as_bytes());
    }
    assert_eq!(requests(&server).await.len(), 2, "resume sends nothing");
}
