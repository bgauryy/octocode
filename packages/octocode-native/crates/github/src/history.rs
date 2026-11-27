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
    /// A REST list's exact row count, when known: a complete first page, or
    /// the last page GitHub links when the same list is read one row per page
    /// ([`GitHubTransport::list_size`]). `None` for a search, a later page, or
    /// a provider that links no last page.
    pub list_total: Option<usize>,
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
    /// The commit list `r` names, with GitHub's `committer` filter (login or email).
    pub async fn list_commits_by_committer(
        &self,
        r: &CommitListRequest,
        committer: Option<&str>,
        context: &RequestContext,
    ) -> Result<HistoryPage, ProviderError> {
        let spec = self.commit_list_spec(r, committer)?;
        let url = spec.url.clone();
        let page = self.read_commit_list(spec, context).await?;
        Ok(self
            .with_list_total(page, r.page, &url, context, "commit list")
            .await)
    }
    /// `page` with its list's exact size when it is page 1 of several: one
    /// more request, the same list one row per page. A size read that fails
    /// or names no last page leaves the total unstated, never the page.
    async fn with_list_total(
        &self,
        mut page: HistoryPage,
        number: usize,
        url: &reqwest::Url,
        context: &RequestContext,
        what: &str,
    ) -> HistoryPage {
        page.list_total = match (number, page.has_more) {
            (1, false) => Some(page.items.len()),
            (1, true) => self.list_size(url, context, what).await.ok().flatten(),
            _ => None,
        };
        page
    }
    /// How many rows the REST list `url` names holds: read one row per page,
    /// GitHub's `Link` names the last page, which is the row count. A list
    /// with no next page holds the rows that read returned; one that links a
    /// next page but no last page has no stated size (`None`). One request.
    pub(crate) async fn list_size(
        &self,
        url: &reqwest::Url,
        context: &RequestContext,
        what: &str,
    ) -> Result<Option<usize>, ProviderError> {
        let kept: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(key, _)| key != "page" && key != "per_page")
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect();
        let mut probe = url.clone();
        probe
            .query_pairs_mut()
            .clear()
            .extend_pairs(kept)
            .append_pair("page", "1")
            .append_pair("per_page", "1");
        let (items, links): (Vec<Value>, _) = self
            .read_json_linked(RequestSpec::get(probe), context, what)
            .await?;
        Ok(match (links.last_page, links.has_more) {
            (Some(last), _) => Some(last),
            (None, false) => Some(items.len()),
            (None, true) => None,
        })
    }
    /// The commit list `r` asks for, as one request.
    pub(crate) fn commit_list_spec(
        &self,
        r: &CommitListRequest,
        committer: Option<&str>,
    ) -> Result<RequestSpec, ProviderError> {
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
        Ok(RequestSpec::get(url))
    }
    /// Reads the commit list `spec` asks for.
    pub(crate) async fn read_commit_list(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
    ) -> Result<HistoryPage, ProviderError> {
        let (items, has_more): (Vec<Value>, bool) =
            self.read_json(spec, context, "commit list").await?;
        Ok(HistoryPage {
            total_count: 0,
            incomplete_results: false,
            items,
            warnings: Vec::new(),
            listed: true,
            has_more,
            list_total: None,
        })
    }
    /// Confirms that `login` is a GitHub account the token can see
    /// (`GET /users/{login}`). GitHub's commit
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
        self.read_search::<Value>(
            RequestSpec::get(url),
            context,
            "invalid GitHub user response",
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
            .read_json(RequestSpec::get(url.clone()), context, "pull request list")
            .await?;
        let page = HistoryPage {
            total_count: 0,
            incomplete_results: false,
            items,
            warnings: Vec::new(),
            listed: true,
            has_more,
            list_total: None,
        };
        Ok(self
            .with_list_total(page, request.page, &url, context, "pull request list")
            .await)
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
        let spec = |page: usize, per_page: usize| -> Result<RequestSpec, ProviderError> {
            let mut url = self.endpoint().rest(&["search", kind])?;
            {
                let mut q = url.query_pairs_mut();
                q.append_pair("q", &request.query)
                    .append_pair("page", &page.to_string())
                    .append_pair("per_page", &per_page.to_string());
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
            Ok(spec)
        };
        let (mut value, withheld): (Value, _) = self
            .read_search_page(
                spec,
                request.page,
                request.per_page,
                context,
                "invalid GitHub history search response",
            )
            .await?;
        Ok(HistoryPage {
            total_count: value
                .get("total_count")
                .and_then(Value::as_u64)
                .map_or(0, |count| usize::try_from(count).unwrap_or(usize::MAX)),
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
            list_total: None,
        })
    }
}
