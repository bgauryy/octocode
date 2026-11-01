//! `limit` conformance for the GitHub search tools: each provider request
//! asks `per_page = limit`, an omitted `limit` is the contract default (the
//! page size these tools served before `limit` existed), the contract
//! bounds it by GitHub's documented `per_page` maximum, and a walk at a
//! small `limit` lists every row exactly once.
use super::pages::page_data;
use crate::contracts::query_limits;
use crate::tools::gh_shared::test_support::backed::repo_item;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The small page size the walks use.
const LIMIT: usize = 2;

/// One search endpoint's pages of `rows` at `per` rows a page.
async fn mount_search(server: &MockServer, route: &str, per: usize, rows: &[Value]) {
    for (index, chunk) in rows.chunks(per).enumerate() {
        Mock::given(method("GET"))
            .and(path(route))
            .and(query_param("page", (index + 1).to_string()))
            .and(query_param("per_page", per.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count": rows.len(), "incomplete_results": false, "items": chunk
            })))
            .mount(server)
            .await;
    }
}

/// One REST list endpoint's pages of `rows` at `per` rows a page, each but
/// the last with a Link `rel="next"`.
async fn mount_list(server: &MockServer, route: &str, per: usize, rows: &[Value]) {
    let pages = rows.chunks(per).count();
    for (index, chunk) in rows.chunks(per).enumerate() {
        let page = index + 1;
        let mut response = ResponseTemplate::new(200).set_body_json(json!(chunk));
        if page < pages {
            let link = format!(
                "<{}/api/v3{route}?page={}&per_page={per}>; rel=\"next\"",
                server.uri(),
                page + 1
            );
            response = response.insert_header("link", link.as_str());
        }
        Mock::given(method("GET"))
            .and(path(format!("/api/v3{route}")))
            .and(query_param("page", page.to_string()))
            .and(query_param("per_page", per.to_string()))
            .respond_with(response)
            .mount(server)
            .await;
    }
}

fn code_item(index: usize) -> Value {
    json!({"name": format!("f{index}.rs"), "path": format!("src/f{index}.rs"), "sha": "1",
        "html_url": "https://x",
        "repository": {"full_name": "a/b", "html_url": "https://x", "url": "https://x"}})
}

fn issue_item(number: usize) -> Value {
    json!({"number": number, "title": format!("issue {number}"), "state": "open",
        "user": {"login": "dev"}})
}

fn commit_item(index: usize) -> Value {
    json!({"sha": format!("c{index}"), "commit": {"message": format!("m{index}"),
        "author": {"date": "2026-09-01T00:00:00Z"}}})
}

/// One walk case: the tool, its route and how it pages (search or REST
/// list), the provider rows, the first row, and the listed rows' key and
/// identity field.
struct Case {
    tool: &'static str,
    route: &'static str,
    search: bool,
    rows: Vec<Value>,
    first: Value,
    rows_key: &'static str,
    id: &'static str,
}

fn cases() -> Vec<Case> {
    let five = 1..=5;
    vec![
        Case {
            tool: "ghSearchRepo",
            route: "/api/v3/search/repositories",
            search: true,
            rows: five
                .clone()
                .map(|n| repo_item(&format!("r{n}"), false))
                .collect(),
            first: json!({"operation":"search","keywords":["x"]}),
            rows_key: "repositories",
            id: "repo",
        },
        Case {
            tool: "ghSearchRepo",
            route: "/orgs/o/repos",
            search: false,
            rows: five
                .clone()
                .map(|n| repo_item(&format!("r{n}"), false))
                .collect(),
            first: json!({"operation":"list","owner":"o"}),
            rows_key: "repositories",
            id: "repo",
        },
        Case {
            tool: "ghSearchCode",
            route: "/api/v3/search/code",
            search: true,
            rows: five.clone().map(code_item).collect(),
            first: json!({"owner":"a","keywords":["needle"],"matchIn":["path"]}),
            rows_key: "files",
            id: "path",
        },
        Case {
            tool: "ghSearchHistory",
            route: "/api/v3/search/issues",
            search: true,
            rows: five.clone().map(issue_item).collect(),
            first: json!({"operation":"issue","owner":"o","repo":"r"}),
            rows_key: "issues",
            id: "number",
        },
        Case {
            tool: "ghSearchHistory",
            route: "/repos/o/r/commits",
            search: false,
            rows: five.map(commit_item).collect(),
            first: json!({"operation":"commit","owner":"o","repo":"r"}),
            rows_key: "commits",
            id: "commitSha",
        },
    ]
}

async fn mount(server: &MockServer, case: &Case, per: usize) {
    if case.search {
        mount_search(server, case.route, per, &case.rows).await;
    } else {
        mount_list(server, case.route, per, &case.rows).await;
    }
}

/// Every page of a `limit: 2` walk holds at most 2 rows, its `nextPage`
/// carries the same `limit`, and the walk lists each of the 5 rows once,
/// in provider order. The mocks answer only `per_page=2`.
#[tokio::test]
async fn a_small_limit_walk_lists_every_row_exactly_once() {
    for case in cases() {
        let server = MockServer::builder().start().await;
        mount(&server, &case, LIMIT).await;
        let mut row = case.first.clone();
        row["limit"] = json!(LIMIT);
        let mut listed = Vec::new();
        for _ in 0..=case.rows.len() {
            let data = page_data(&server, case.tool, row.clone()).await;
            let label = format!("{} {}: {data}", case.tool, case.route);
            let rows = data[case.rows_key].as_array().expect(&label);
            assert!(rows.len() <= LIMIT, "{label}");
            listed.extend(rows.iter().map(|row| row[case.id].to_string()));
            let Some(next) = data.pointer("/next/nextPage/input/queries/0") else {
                break;
            };
            assert_eq!(
                next["limit"],
                json!(LIMIT),
                "the cursor carries limit: {label}"
            );
            row = next.clone();
        }
        let expected = case
            .rows
            .iter()
            .map(|item| match case.id {
                "repo" => item["name"].to_string(),
                "commitSha" => item["sha"].to_string(),
                key => item[key].to_string(),
            })
            .collect::<Vec<_>>();
        assert_eq!(listed, expected, "{} {}", case.tool, case.route);
    }
}

/// An omitted `limit` is the page size each tool served before `limit`
/// existed (50 rows), read from the contract: the mocks answer only
/// `per_page=50`.
#[tokio::test]
async fn an_omitted_limit_requests_the_contract_default() {
    assert_eq!(query_limits::gh_search_repo::LIMIT_DEFAULT, 50);
    assert_eq!(query_limits::gh_search_code::LIMIT_DEFAULT, 50);
    assert_eq!(query_limits::gh_search_history::LIMIT_DEFAULT, 50);
    for case in cases() {
        let server = MockServer::builder().start().await;
        let default = match case.tool {
            "ghSearchRepo" => query_limits::gh_search_repo::LIMIT_DEFAULT,
            "ghSearchCode" => query_limits::gh_search_code::LIMIT_DEFAULT,
            _ => query_limits::gh_search_history::LIMIT_DEFAULT,
        };
        mount(&server, &case, default).await;
        let data = page_data(&server, case.tool, case.first.clone()).await;
        let label = format!("{} {}: {data}", case.tool, case.route);
        assert_eq!(
            data[case.rows_key].as_array().map(Vec::len),
            Some(case.rows.len()),
            "{label}"
        );
        assert!(data.pointer("/next/nextPage").is_none(), "{label}");
    }
}

/// Admission bounds `limit` by GitHub's documented `per_page` maximum (100)
/// on every paged operation; 0 is no page.
#[test]
fn admission_bounds_limit_by_githubs_per_page_maximum() {
    for max in [
        query_limits::gh_search_repo::LIMIT_MAXIMUM,
        query_limits::gh_search_code::LIMIT_MAXIMUM,
        query_limits::gh_search_history::LIMIT_MAXIMUM,
    ] {
        assert_eq!(max, octocode_github::MAX_PER_PAGE);
    }
    for (tool, row) in [
        (
            "ghSearchRepo",
            json!({"operation":"search","keywords":["x"]}),
        ),
        ("ghSearchRepo", json!({"operation":"list","owner":"o"})),
        ("ghSearchCode", json!({"owner":"o","keywords":["x"]})),
        (
            "ghSearchHistory",
            json!({"operation":"pullRequest","owner":"o","repo":"r"}),
        ),
        (
            "ghSearchHistory",
            json!({"operation":"issue","owner":"o","repo":"r"}),
        ),
        (
            "ghSearchHistory",
            json!({"operation":"commit","owner":"o","repo":"r"}),
        ),
        (
            "ghSearchHistory",
            json!({"operation":"commit","owner":"o","repo":"r","keywords":["fix"]}),
        ),
    ] {
        let with = |limit: usize| {
            let mut row = row.clone();
            row["limit"] = json!(limit);
            crate::contracts::validate_query(tool, row)
        };
        let label = format!("{tool} {row}");
        assert!(with(octocode_github::MAX_PER_PAGE).is_ok(), "{label}");
        assert!(with(octocode_github::MAX_PER_PAGE + 1).is_err(), "{label}");
        assert!(with(0).is_err(), "{label}");
        let defaulted = crate::contracts::validate_query(tool, row.clone()).expect(&label);
        assert_eq!(defaulted["limit"], json!(50), "{label}");
    }
}
