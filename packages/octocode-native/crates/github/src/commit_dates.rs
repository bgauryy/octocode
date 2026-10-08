//! Last-commit dates of repository paths at one commit.
//!
//! One GraphQL request answers up to [`MAX_PATHS_PER_REQUEST`] paths: each
//! path is an aliased `history(first:1, path:)` on the commit object. Paths
//! travel as GraphQL variables, never inside the document text, so quotes,
//! backslashes, and non-ASCII names need no escaping.
use super::{GitHubTransport, GraphQlPage, ProviderError, ProviderErrorKind, RequestContext};
use serde_json::{Map, Value, json};

/// Aliased `history` connections per GraphQL request.
pub const MAX_PATHS_PER_REQUEST: usize = 100;

/// Dates answered for one commit's paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathDates {
    /// The commit's own `committedDate` (RFC 3339, UTC).
    pub commit_date: Option<String>,
    /// Per requested path, in input order: the `committedDate` of the last
    /// commit at or before the commit that touched it. `None` when the path
    /// has no history there or its request failed.
    pub dates: Vec<Option<String>>,
    /// The first failure. Requests after it are not sent, so every path of
    /// the failed and later requests stays `None`.
    pub error: Option<ProviderError>,
}

/// The GraphQL document for `count` paths: variables `owner`, `name`, `oid`,
/// and `p0..p{count-1}`; the alias of path `i` is `p{i}`.
pub(crate) fn path_dates_document(count: usize) -> String {
    let mut variables = String::from("$owner:String!,$name:String!,$oid:GitObjectID!");
    let mut selections = String::from("committedDate");
    for index in 0..count {
        variables.push_str(&format!(",$p{index}:String!"));
        selections.push_str(&format!(
            " p{index}:history(first:1,path:$p{index}){{nodes{{committedDate}}}}"
        ));
    }
    format!(
        "query({variables}){{repository(owner:$owner,name:$name){{object(oid:$oid){{...on Commit{{{selections}}}}}}}}}"
    )
}

/// The variables binding [`path_dates_document`] to one request.
pub(crate) fn path_dates_variables(owner: &str, repo: &str, commit: &str, paths: &[&str]) -> Value {
    let mut variables = Map::new();
    variables.insert("owner".into(), json!(owner));
    variables.insert("name".into(), json!(repo));
    variables.insert("oid".into(), json!(commit));
    for (index, path) in paths.iter().enumerate() {
        variables.insert(format!("p{index}"), json!(path));
    }
    Value::Object(variables)
}

/// The commit date and the `count` path dates of one response; an answer
/// without the commit object is an error, and so are GraphQL errors that
/// left any requested alias unanswered.
fn parse_path_dates(
    page: &GraphQlPage,
    count: usize,
) -> Result<(Option<String>, Vec<Option<String>>), ProviderError> {
    let commit = page
        .data
        .as_ref()
        .and_then(|data| data.pointer("/repository/object"))
        .filter(|object| object.is_object());
    let Some(commit) = commit else {
        let message = page.errors.first().map_or_else(
            || "GitHub GraphQL returned no commit for the listed ref".to_owned(),
            |error| format!("GitHub GraphQL: {}", error.message),
        );
        return Err(ProviderError::new(ProviderErrorKind::NotFound, message));
    };
    let date = |value: Option<&Value>| value.and_then(Value::as_str).map(str::to_owned);
    let mut answered = true;
    let dates = (0..count)
        .map(|index| {
            let history = commit.get(format!("p{index}"));
            answered &= history.is_some_and(Value::is_object);
            date(history.and_then(|history| history.pointer("/nodes/0/committedDate")))
        })
        .collect();
    if !answered && let Some(error) = page.errors.first() {
        return Err(ProviderError::new(
            ProviderErrorKind::Decode,
            format!("GitHub GraphQL: {}", error.message),
        ));
    }
    Ok((date(commit.get("committedDate")), dates))
}

impl GitHubTransport {
    /// The last-commit date of each of `paths` at `commit` (a full SHA) and
    /// the commit's own date, in one GraphQL request per
    /// [`MAX_PATHS_PER_REQUEST`] paths, sent in order under the shared
    /// executor. A disabled, throttled, or tokenless GraphQL sends nothing.
    pub async fn path_commit_dates(
        &self,
        owner: &str,
        repo: &str,
        commit: &str,
        paths: &[&str],
        context: &RequestContext,
    ) -> PathDates {
        let mut result = PathDates {
            commit_date: None,
            dates: vec![None; paths.len()],
            error: None,
        };
        if paths.is_empty() {
            return result;
        }
        if let Err(error) = self.graphql_ready(context) {
            result.error = Some(error);
            return result;
        }
        for (chunk, slice) in paths.chunks(MAX_PATHS_PER_REQUEST).enumerate() {
            let answer = self
                .execute_graphql(
                    &path_dates_document(slice.len()),
                    path_dates_variables(owner, repo, commit, slice),
                    context,
                )
                .await
                .and_then(|page| parse_path_dates(&page, slice.len()));
            match answer {
                Ok((commit_date, dates)) => {
                    result.commit_date = result.commit_date.or(commit_date);
                    let start = chunk * MAX_PATHS_PER_REQUEST;
                    result.dates[start..start + dates.len()].clone_from_slice(&dates);
                }
                Err(error) => {
                    result.error = Some(error);
                    break;
                }
            }
        }
        result
    }

    /// GraphQL can be sent now: enabled, a credential is configured (GitHub
    /// GraphQL rejects anonymous calls), and the bucket is not blocked.
    fn graphql_ready(&self, context: &RequestContext) -> Result<(), ProviderError> {
        if !self.graphql_enabled {
            return Err(ProviderError::new(
                ProviderErrorKind::Configuration,
                "GitHub GraphQL is disabled",
            ));
        }
        if context.resolved_credential().is_none() {
            return Err(ProviderError::new(
                ProviderErrorKind::Authentication,
                "GitHub GraphQL needs a token",
            ));
        }
        if !self.graphql_available(context) {
            return Err(ProviderError::new(
                ProviderErrorKind::RateLimited,
                "GitHub GraphQL rate limit is exhausted",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CredentialSource, GitHubEndpoint, RequestBudget, ResolvedCredential, RetryPolicy};
    use std::time::Duration;
    use wiremock::{
        Mock, MockServer, Request, Respond, ResponseTemplate,
        matchers::{method, path},
    };

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    fn transport(server: &MockServer) -> GitHubTransport {
        let endpoint =
            GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).unwrap())
                .unwrap();
        GitHubTransport::new(
            endpoint,
            RetryPolicy {
                max_attempts: 1,
                base_delay: Duration::from_millis(1),
                max_retry_after: Duration::from_secs(1),
            },
        )
        .unwrap()
    }

    /// A request budget; `token` attaches a credential.
    fn context(token: bool) -> RequestContext {
        RequestContext::new(
            RequestBudget::with_timeout(Duration::from_secs(5), 1 << 20),
            token.then(|| ResolvedCredential::new("secret", CredentialSource::Environment)),
        )
    }

    /// Answers every alias in the request with a date derived from its
    /// path variable's length, so a test can tell which path got which date.
    struct Echo;
    impl Respond for Echo {
        fn respond(&self, request: &Request) -> ResponseTemplate {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let mut object = Map::new();
            object.insert("committedDate".into(), json!("2026-01-31T10:00:00Z"));
            for (name, value) in body["variables"].as_object().unwrap() {
                if let Some(index) = name.strip_prefix('p') {
                    let day = 1 + value.as_str().unwrap().chars().count() % 28;
                    object.insert(
                        format!("p{index}"),
                        json!({"nodes":[{"committedDate": format!("2025-02-{day:02}T00:00:00Z")}]}),
                    );
                }
            }
            ResponseTemplate::new(200)
                .set_body_json(json!({"data":{"repository":{"object": object}}}))
        }
    }

    fn graphql_bodies(requests: &[Request]) -> Vec<Value> {
        requests
            .iter()
            .filter(|request| request.url.path() == "/api/graphql")
            .map(|request| serde_json::from_slice(&request.body).unwrap())
            .collect()
    }

    #[test]
    fn the_document_names_each_path_by_variable_only() {
        let document = path_dates_document(2);
        assert_eq!(
            document,
            "query($owner:String!,$name:String!,$oid:GitObjectID!,$p0:String!,$p1:String!){repository(owner:$owner,name:$name){object(oid:$oid){...on Commit{committedDate p0:history(first:1,path:$p0){nodes{committedDate}} p1:history(first:1,path:$p1){nodes{committedDate}}}}}}"
        );
    }

    #[tokio::test]
    async fn one_request_dates_every_path_and_the_commit() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(Echo)
            .expect(1)
            .mount(&server)
            .await;
        let dates = transport(&server)
            .path_commit_dates("a", "b", SHA, &["src", "README.md"], &context(true))
            .await;
        assert_eq!(dates.error, None);
        assert_eq!(dates.commit_date.as_deref(), Some("2026-01-31T10:00:00Z"));
        assert_eq!(
            dates.dates,
            vec![
                Some("2025-02-04T00:00:00Z".into()),
                Some("2025-02-10T00:00:00Z".into())
            ]
        );
        let bodies = graphql_bodies(&server.received_requests().await.unwrap());
        assert_eq!(
            bodies[0]["variables"],
            json!({"owner":"a","name":"b","oid":SHA,"p0":"src","p1":"README.md"})
        );
    }

    #[tokio::test]
    async fn paths_past_one_hundred_are_chunked_in_order() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(Echo)
            .expect(3)
            .mount(&server)
            .await;
        let names = (0..201).map(|n| "x".repeat(n % 7 + 1)).collect::<Vec<_>>();
        let paths = names.iter().map(String::as_str).collect::<Vec<_>>();
        let dates = transport(&server)
            .path_commit_dates("a", "b", SHA, &paths, &context(true))
            .await;
        assert_eq!(dates.error, None);
        assert_eq!(dates.dates.len(), 201);
        for (name, date) in names.iter().zip(&dates.dates) {
            let day = 1 + name.len() % 28;
            assert_eq!(
                date.as_deref(),
                Some(format!("2025-02-{day:02}T00:00:00Z").as_str())
            );
        }
        let bodies = graphql_bodies(&server.received_requests().await.unwrap());
        let sizes = bodies
            .iter()
            .map(|body| body["variables"].as_object().unwrap().len() - 3)
            .collect::<Vec<_>>();
        assert_eq!(sizes, [100, 100, 1]);
    }

    #[tokio::test]
    async fn quotes_backslashes_and_unicode_travel_as_variables() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(Echo)
            .mount(&server)
            .await;
        let tricky = [
            r#"a "quoted" name.md"#,
            r"back\slash\}.txt",
            "docs/日本語 ✓.md",
            "x){repository(owner:\"evil\"",
        ];
        let dates = transport(&server)
            .path_commit_dates("a", "b", SHA, &tricky, &context(true))
            .await;
        assert_eq!(dates.error, None);
        assert!(dates.dates.iter().all(Option::is_some), "{dates:?}");
        let body = &graphql_bodies(&server.received_requests().await.unwrap())[0];
        let document = body["query"].as_str().unwrap();
        assert_eq!(document, path_dates_document(4));
        for (index, raw) in tricky.iter().enumerate() {
            assert_eq!(body["variables"][format!("p{index}")], *raw);
            assert!(!document.contains(raw), "{document}");
        }
    }

    #[tokio::test]
    async fn a_graphql_error_dates_nothing_and_names_the_failure() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": null,
                "errors": [{"message": "Timeout on validation of query"}]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let dates = transport(&server)
            .path_commit_dates("a", "b", SHA, &["one", "two"], &context(true))
            .await;
        assert_eq!(dates.dates, vec![None, None]);
        assert_eq!(dates.commit_date, None);
        let error = dates.error.expect("error");
        assert!(error.message.contains("Timeout on validation"), "{error:?}");
    }

    #[tokio::test]
    async fn a_failed_chunk_stops_later_requests_and_keeps_earlier_dates() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(Echo)
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(ResponseTemplate::new(502))
            .mount(&server)
            .await;
        let names = (0..250).map(|n| format!("f{n}")).collect::<Vec<_>>();
        let paths = names.iter().map(String::as_str).collect::<Vec<_>>();
        let dates = transport(&server)
            .path_commit_dates("a", "b", SHA, &paths, &context(true))
            .await;
        assert!(dates.error.is_some());
        assert!(dates.dates[..100].iter().all(Option::is_some));
        assert!(dates.dates[100..].iter().all(Option::is_none));
        assert_eq!(
            graphql_bodies(&server.received_requests().await.unwrap()).len(),
            2
        );
    }

    #[tokio::test]
    async fn no_token_or_disabled_graphql_sends_nothing() {
        let server = MockServer::start().await;
        let anonymous = transport(&server)
            .path_commit_dates("a", "b", SHA, &["one"], &context(false))
            .await;
        assert_eq!(
            anonymous.error.map(|error| error.kind),
            Some(ProviderErrorKind::Authentication)
        );
        let mut disabled = transport(&server);
        disabled.graphql_enabled = false;
        let disabled = disabled
            .path_commit_dates("a", "b", SHA, &["one"], &context(true))
            .await;
        assert_eq!(
            disabled.error.map(|error| error.kind),
            Some(ProviderErrorKind::Configuration)
        );
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_path_without_history_has_no_date() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"data":{"repository":{"object":{
                    "committedDate": "2026-01-31T10:00:00Z",
                    "p0": {"nodes": []},
                    "p1": {"nodes": [{"committedDate": "2024-05-06T07:08:09Z"}]}
                }}}}),
            ))
            .mount(&server)
            .await;
        let dates = transport(&server)
            .path_commit_dates("a", "b", SHA, &["gone", "kept"], &context(true))
            .await;
        assert_eq!(dates.error, None);
        assert_eq!(dates.dates, vec![None, Some("2024-05-06T07:08:09Z".into())]);
    }
}
