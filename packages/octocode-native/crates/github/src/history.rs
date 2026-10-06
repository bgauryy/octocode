use super::{
    CredentialResolver, GitHubTransport, ProviderError, ProviderErrorKind, RequestContext,
    RequestSpec,
};
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
#[derive(Clone, Debug)]
pub struct HistoryPage {
    pub total_count: usize,
    pub incomplete_results: bool,
    pub items: Vec<Value>,
    pub provider_page: usize,
    pub warnings: Vec<String>,
    pub listed: bool,
    pub has_more: bool,
}
impl Default for HistoryPage {
    fn default() -> Self {
        Self {
            total_count: 0,
            incomplete_results: false,
            items: Vec::new(),
            provider_page: 1,
            warnings: Vec::new(),
            listed: false,
            has_more: false,
        }
    }
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
impl<R: CredentialResolver> GitHubTransport<R> {
    pub async fn list_commits(
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
            provider_page: r.page,
            warnings: Vec::new(),
            listed: true,
            has_more,
        })
    }
    pub async fn canonical_owner_repo(
        &self,
        owner: &str,
        repo: &str,
        context: &RequestContext,
    ) -> Result<(String, String, bool, Vec<String>), ProviderError> {
        match self.repository_metadata(owner, repo, context).await {
            Ok(meta) => {
                if let Some(full) = meta.full_name.as_deref()
                    && let Some((canonical_owner, canonical_repo)) = full.split_once('/')
                    && (canonical_owner != owner || canonical_repo != repo)
                {
                    return Ok((
                        canonical_owner.into(),
                        canonical_repo.into(),
                        true,
                        vec![format!(
                            "Repository {owner}/{repo} was renamed to {canonical_owner}/{canonical_repo} — GitHub search does not follow renames, so the canonical name was searched instead."
                        )],
                    ));
                }
                Ok((owner.into(), repo.into(), false, Vec::new()))
            }
            Err(error) if error.kind == ProviderErrorKind::NotFound => {
                Ok((owner.into(), repo.into(), false, Vec::new()))
            }
            Err(error) => Err(error),
        }
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
            provider_page: request.page,
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
        let (value, _): (Value, bool) = self
            .revalidated_json(spec, false, context, "history search")
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
            items: value
                .get("items")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            provider_page: request.page,
            warnings: Vec::new(),
            listed: false,
            has_more: false,
        })
    }
}
