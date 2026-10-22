use super::{GitHubTransport, ProviderError, ProviderErrorReason, RequestContext, RequestSpec};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct HistoryRequest {
    pub query: String,
    pub page: usize,
    pub per_page: usize,
    pub sort: Option<String>,
    pub order: Option<String>,
}
#[derive(Clone, Debug)]
pub struct CommitListRequest {
    pub owner: String,
    pub repo: String,
    pub branch: Option<String>,
    pub path: Option<String>,
    pub author: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub page: usize,
    pub per_page: usize,
}
#[derive(Clone, Debug, Default)]
pub struct HistoryPage {
    pub total_count: usize,
    pub incomplete_results: bool,
    pub items: Vec<Value>,
    pub warnings: Vec<String>,
    pub listed: bool,
    pub has_more: bool,
}
#[derive(Clone, Debug)]
pub struct PullListRequest {
    pub owner: String,
    pub repo: String,
    pub state: Option<String>,
    pub head: Option<String>,
    pub base: Option<String>,
    pub sort: Option<String>,
    pub order: Option<String>,
    pub page: usize,
    pub per_page: usize,
}
impl GitHubTransport {
    pub(crate) async fn list_commits(
        &self,
        r: &CommitListRequest,
        context: &RequestContext,
    ) -> Result<HistoryPage, ProviderError> {
        self.list_commits_by_committer(r, None, context).await
    }
    /// `list_commits` plus GitHub's `committer` filter (login or email).
    pub async fn list_commits_by_committer(
        &self,
        r: &CommitListRequest,
        committer: Option<&str>,
        context: &RequestContext,
    ) -> Result<HistoryPage, ProviderError> {
        let mut url = self
            .endpoint()
            .rest(&["repos", &r.owner, &r.repo, "commits"])?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("page", &r.page.to_string())
                .append_pair("per_page", &r.per_page.to_string());
            for (k, v) in [
                ("sha", r.branch.as_deref()),
                ("path", r.path.as_deref()),
                ("author", r.author.as_deref()),
                ("committer", committer),
                ("since", r.since.as_deref()),
                ("until", r.until.as_deref()),
            ] {
                if let Some(v) = v {
                    q.append_pair(k, v);
                }
            }
        }
        let (items, has_more): (Vec<Value>, bool) = self
            .revalidated_json(RequestSpec::get(url), false, context, "commit list")
            .await?;
        Ok(HistoryPage {
            total_count: 0,
            incomplete_results: false,
            items,
            warnings: Vec::new(),
            listed: true,
            has_more,
        })
    }
    /// Confirms that `login` is a GitHub account the token can see
    /// (`GET /users/{login}`, through the response cache). GitHub's commit
    /// search drops an unknown `author:`/`committer:` login without error and
    /// answers the unfiltered total, so a search names its logins here first.
    /// A 404 is a [`ProviderErrorReason::UserNotFound`] failure; any other
    /// failure is the lookup's own.
    pub async fn confirm_user(
        &self,
        login: &str,
        context: &RequestContext,
    ) -> Result<(), ProviderError> {
        let url = self.endpoint().rest(&["users", login])?;
        self.cached_search::<Value>(
            "github-user",
            RequestSpec::get(url),
            context,
            super::search::Lookup::Store,
            "invalid GitHub user response",
            |_| true,
        )
        .await
        .map(|_| ())
        .map_err(|mut error| {
            if error.status == Some(404) {
                error.message = format!("no user named \"{login}\"").into_boxed_str();
                error = error.with_reason(ProviderErrorReason::UserNotFound);
            }
            error
        })
    }
    pub async fn list_pull_requests(
        &self,
        request: &PullListRequest,
        context: &RequestContext,
    ) -> Result<HistoryPage, ProviderError> {
        let mut url = self
            .endpoint()
            .rest(&["repos", &request.owner, &request.repo, "pulls"])?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("page", &request.page.to_string())
                .append_pair("per_page", &request.per_page.to_string());
            // Match the issue listing: an unset state means every lifecycle state.
            let state = match request.state.as_deref() {
                Some(state @ ("closed" | "open")) => state,
                Some("merged") => "closed",
                _ => "all",
            };
            q.append_pair("state", state);
            q.append_pair(
                "sort",
                if request.sort.as_deref() == Some("updated") {
                    "updated"
                } else {
                    "created"
                },
            );
            // GitHub defaults sort=created to ascending (oldest first); an
            // unset order should surface the newest pull requests.
            q.append_pair("direction", request.order.as_deref().unwrap_or("desc"));
            if let Some(head) = &request.head {
                q.append_pair("head", head);
            }
            if let Some(base) = &request.base {
                q.append_pair("base", base);
            }
        }
        let (items, has_more): (Vec<Value>, bool) = self
            .revalidated_json(RequestSpec::get(url), false, context, "pull request list")
            .await?;
        Ok(HistoryPage {
            total_count: 0,
            incomplete_results: false,
            items,
            warnings: Vec::new(),
            listed: true,
            has_more,
        })
    }
    pub async fn search_issues(
        &self,
        request: &HistoryRequest,
        context: &RequestContext,
    ) -> Result<HistoryPage, ProviderError> {
        self.history_search("issues", request, context).await
    }
    pub async fn search_commits(
        &self,
        request: &HistoryRequest,
        context: &RequestContext,
    ) -> Result<HistoryPage, ProviderError> {
        self.history_search("commits", request, context).await
    }
    async fn history_search(
        &self,
        kind: &str,
        request: &HistoryRequest,
        context: &RequestContext,
    ) -> Result<HistoryPage, ProviderError> {
        let mut url = self.endpoint().rest(&["search", kind])?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("q", &request.query)
                .append_pair("page", &request.page.to_string())
                .append_pair("per_page", &request.per_page.to_string());
            if let Some(v) = &request.sort {
                q.append_pair("sort", v);
            }
            if let Some(v) = &request.order {
                q.append_pair("order", v);
            }
        }
        let mut spec = RequestSpec::get(url);
        if kind == "commits" {
            spec.headers.insert(
                reqwest::header::ACCEPT,
                reqwest::header::HeaderValue::from_static("application/vnd.github+json"),
            );
        }
        // Search answers come from the search index: cached like code search.
        let (mut value, withheld): (Value, _) = self
            .cached_search(
                "github-history-search",
                spec,
                context,
                super::search::Lookup::Store,
                "invalid GitHub history search response",
                |value: &Value| {
                    !value
                        .get("incomplete_results")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                },
            )
            .await?;
        Ok(HistoryPage {
            total_count: value
                .get("total_count")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
            incomplete_results: value
                .get("incomplete_results")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            items: match value.get_mut("items").map(Value::take) {
                Some(Value::Array(items)) => items,
                _ => Vec::new(),
            },
            warnings: crate::sso_partial_warning(&withheld).into_iter().collect(),
            listed: false,
            has_more: false,
        })
    }
}
