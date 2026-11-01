//! `limit` on artifactSearch discovery: rows per registry page, 20 when
//! omitted (the contract default, today's page size), at most 100 (the
//! smallest documented registry page maximum: crates.io and Packagist
//! `per_page`); a small-`limit` walk lists every package exactly once.
use super::*;
use crate::contracts::query_limits::artifact_search::search as limits;

/// An npm registry of `total` packages `p0…`, answering `size` rows from
/// `from`.
struct NpmRegistry {
    total: u64,
}

impl wiremock::Respond for NpmRegistry {
    fn respond(&self, request: &wiremock::Request) -> wiremock::ResponseTemplate {
        let param = |name: &str| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == name)
                .and_then(|(_, value)| value.parse::<u64>().ok())
        };
        let from = param("from").unwrap_or(0);
        let end = (from + param("size").unwrap_or(u64::MAX - from)).min(self.total);
        let objects: Vec<Value> = (from..end)
            .map(|n| json!({"package": {"name": format!("p{n}"), "version": "1.0.0"}}))
            .collect();
        wiremock::ResponseTemplate::new(200)
            .set_body_json(json!({"objects": objects, "total": self.total}))
    }
}

async fn npm_registry(total: u64) -> wiremock::MockServer {
    let server = wiremock::MockServer::builder().start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(NpmRegistry { total })
        .mount(&server)
        .await;
    server
}

fn names(data: &Value) -> Vec<String> {
    data["artifacts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row["packageName"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// A `limit: 3` walk of 7 packages lists 3, 3 and 1; each `nextPage`
/// carries the limit and the walk lists every package once.
#[tokio::test]
async fn a_small_limit_walk_lists_every_package_once() {
    let server = npm_registry(7).await;
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    let mut row = json!({"operation":"search","ecosystem":"npm","keywords":["x"],"limit":3,
        "registryUrl": format!("http://127.0.0.1:{}", server.address().port())});
    let (mut listed, mut sizes) = (Vec::new(), Vec::new());
    for _ in 0..=7 {
        let data = execute(typed(&row), call(deadline, true))
            .await
            .expect("discovery succeeds");
        let page = names(&data);
        sizes.push(page.len());
        listed.extend(page);
        let Some(next) = data.pointer("/next/nextPage/input/queries/0") else {
            break;
        };
        assert_eq!(next["limit"], 3, "the cursor carries limit: {data}");
        row = next.clone();
    }
    assert_eq!(sizes, [3, 3, 1]);
    assert_eq!(listed, (0..7).map(|n| format!("p{n}")).collect::<Vec<_>>());
}

/// An omitted `limit` is the contract default, 20 rows: today's page size.
#[tokio::test]
async fn an_omitted_limit_is_the_contract_default() {
    assert_eq!(limits::LIMIT_DEFAULT, 20);
    let server = npm_registry(100).await;
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    let row = json!({"operation":"search","ecosystem":"npm","keywords":["x"],
        "registryUrl": format!("http://127.0.0.1:{}", server.address().port())});
    let data = execute(typed(&row), call(deadline, true))
        .await
        .expect("discovery succeeds");
    assert_eq!(names(&data).len(), limits::LIMIT_DEFAULT, "{data}");
    assert_eq!(data["pagination"]["pageSize"], 20, "{data}");
}

/// Admission bounds `limit` by 100 on every searchable registry; a
/// resolve takes no `limit`, and 0 is no page.
#[test]
fn admission_bounds_limit_by_the_smallest_registry_maximum() {
    assert_eq!(limits::LIMIT_MAXIMUM, 100);
    for ecosystem in [
        "npm",
        "crates",
        "maven",
        "nuget",
        "go",
        "packagist",
        "rubygems",
    ] {
        let admit = |limit: u64| {
            crate::contracts::validate_query(
                "artifactSearch",
                json!({"operation":"search","ecosystem":ecosystem,"keywords":["x"],"limit":limit}),
            )
        };
        assert!(admit(100).is_ok(), "{ecosystem}");
        assert!(admit(101).is_err(), "{ecosystem}");
        assert!(admit(0).is_err(), "{ecosystem}");
    }
    assert!(
        crate::contracts::validate_query(
            "artifactSearch",
            json!({"operation":"resolve","ecosystem":"npm","packageName":"zod","limit":5}),
        )
        .is_err()
    );
}
