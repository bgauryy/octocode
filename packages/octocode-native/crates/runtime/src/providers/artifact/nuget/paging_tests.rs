//! NuGet discovery paging: page sizes past the registry skip limit.
use super::super::query::artifact_query;
use super::super::test_http::test_budget;
use super::*;
use crate::providers::artifact::http::{
    ArtifactHttp, ArtifactHttpFuture, ArtifactHttpRequest, ArtifactHttpResponse,
};
use octocode_io::budget::RequestBudget;
use serde_json::json;

/// Search mock: echoes `take` synthetic rows starting at `skip` and
/// records every search request's (skip, take).
struct SearchMock(std::sync::Mutex<Vec<(u64, u64)>>);

impl ArtifactHttp for SearchMock {
    fn get<'a>(
        &'a self,
        req: ArtifactHttpRequest,
        _budget: &'a RequestBudget,
    ) -> ArtifactHttpFuture<'a> {
        let body = if req.url.path().ends_with("/v3/index.json") {
            json!({"resources": [{"@type": "SearchQueryService/3.5.0", "@id": "https://azuresearch-usnc.nuget.org/query"}]})
        } else {
            let param = |name: &str| {
                req.url
                    .query_pairs()
                    .find(|(key, _)| key == name)
                    .and_then(|(_, value)| value.parse::<u64>().ok())
                    .unwrap()
            };
            let (skip, take) = (param("skip"), param("take"));
            self.0.lock().unwrap().push((skip, take));
            let data: Vec<_> = (skip..skip + take)
                .map(|index| json!({"id": format!("pkg{index}"), "version": "1.0.0"}))
                .collect();
            json!({"totalHits": 50_000, "data": data})
        };
        Box::pin(async move {
            Ok(ArtifactHttpResponse {
                status: 200,
                body: serde_json::to_vec(&body).unwrap(),
            })
        })
    }
}

#[tokio::test]
async fn nuget_tail_pages_respect_page_size_past_the_skip_limit() {
    let http = SearchMock(Default::default());
    let b = test_budget();
    let client = RegistryClient::uncached(&http, &b);
    let q = artifact_query(
        json!({"operation":"search","ecosystem": ArtifactType::Nuget, "keywords": ["json"]}),
        None,
    );
    let at = |offset: u64| ArtifactProviderState {
        offset: Some(offset),
        ..Default::default()
    };
    let names = |page: &ArtifactProviderPage| {
        page.artifacts
            .iter()
            .map(|a| a.package_name.clone())
            .collect::<Vec<_>>()
    };
    let page = nuget(&q, &at(3000), &client).await.unwrap();
    assert_eq!(
        names(&page),
        (3000..3020).map(|n| format!("pkg{n}")).collect::<Vec<_>>()
    );
    assert_eq!(page.next_state.as_ref().unwrap().offset, Some(3020));
    // Past skip=3000 the tail is sliced locally from one bounded window.
    let page = nuget(&q, &at(3020), &client).await.unwrap();
    assert_eq!(
        names(&page),
        (3020..3040).map(|n| format!("pkg{n}")).collect::<Vec<_>>()
    );
    // The final reachable page serves the last rows; the shared page
    // bounds end it with an honest terminal limit, and no request
    // starts at or past result 4000.
    let page = nuget(&q, &at(3998), &client).await.unwrap();
    assert_eq!(names(&page), ["pkg3998", "pkg3999"]);
    let mut paged = q.clone();
    artifact_search_query::set_page(&mut paged, 200);
    assert!(super::super::page_out_of_range(&paged).is_none());
    artifact_search_query::set_page(&mut paged, 201);
    assert_eq!(
        super::super::page_out_of_range(&paged).map(|limit| limit.reason),
        Some(crate::response::paging::PartialReason::ProviderResultCap)
    );
    assert!(
        super::super::validate_cursor_state(
            ArtifactType::Nuget,
            &at(3999),
            artifact_search_query::page_size(&q)
        )
        .is_ok()
    );
    assert_eq!(
        super::super::validate_cursor_state(
            ArtifactType::Nuget,
            &at(4000),
            artifact_search_query::page_size(&q)
        )
        .expect_err("past the reach")
        .code,
        ErrorCode::InvalidInput
    );
    assert!(
        http.0
            .lock()
            .unwrap()
            .iter()
            .all(|&(skip, take)| skip <= 3000 && take <= 1000)
    );
}

/// "Latest" reads registration pages newest first until one holds a stable
/// release, however many prerelease-only pages come first: no fixed page
/// count decides it.
#[tokio::test]
async fn latest_reads_past_any_number_of_prerelease_only_pages() {
    let prerelease_pages = 6;
    let mut pages = vec![json!({"lower": "1.0.0", "upper": "1.0.0", "items": [
        {"catalogEntry": {"id": "Newtonsoft.Json", "version": "1.0.0"}}
    ]})];
    for n in 1..=prerelease_pages {
        let version = format!("2.0.0-beta.{n}");
        pages.push(json!({"lower": version, "upper": version, "items": [
            {"catalogEntry": {"id": "Newtonsoft.Json", "version": version}}
        ]}));
    }
    let http = super::super::test_http::RouteHttp::routes(vec![
        (
            "/v3/registration5/newtonsoft.json/index.json",
            json!({"items": pages}),
        ),
        (
            "/v3/index.json",
            json!({"resources": [{"@type": "RegistrationsBaseUrl/3.6.0",
                "@id": "https://api.nuget.org/v3/registration5/"}]}),
        ),
    ]);
    let b = super::super::test_http::test_budget();
    let client = RegistryClient::uncached(&http, &b);
    let q = super::super::query::artifact_query(
        json!({"operation":"resolve","ecosystem": ArtifactType::Nuget, "packageName": "Newtonsoft.Json"}),
        None,
    );
    let page = nuget(&q, &ArtifactProviderState::default(), &client)
        .await
        .expect("nuget latest");
    assert_eq!(page.artifacts[0].version.as_deref(), Some("1.0.0"));
}
