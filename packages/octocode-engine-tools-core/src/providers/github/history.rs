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
    pub skipped_pull_request_pages: usize,
    pub warnings: Vec<String>,
    pub listed: bool,
}
impl Default for HistoryPage {
    fn default() -> Self {
        Self {
            total_count: 0,
            incomplete_results: false,
            items: Vec::new(),
            provider_page: 1,
            skipped_pull_request_pages: 0,
            warnings: Vec::new(),
            listed: false,
        }
    }
}
#[derive(Clone, Debug)]
pub struct IssueListRequest {
    pub owner: String,
    pub repo: String,
    pub state: Option<String>,
    pub assignee: Option<String>,
    pub author: Option<String>,
    pub mentions: Option<String>,
    pub labels: Option<Vec<String>>,
    pub sort: Option<String>,
    pub order: Option<String>,
    pub page: usize,
    pub per_page: usize,
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
const MAX_PR_ONLY_PAGES_TO_SKIP: usize = 5;
impl<R: CredentialResolver> GitHubTransport<R> {
    pub async fn list_commits(
        &self,
        r: &CommitListRequest,
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
                ("since", r.since.as_deref()),
                ("until", r.until.as_deref()),
            ] {
                if let Some(v) = v {
                    q.append_pair(k, v);
                }
            }
        }
        let response = self.execute(RequestSpec::get(url), context).await?;
        let items: Vec<Value> = serde_json::from_slice(&response.body).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Decode,
                "invalid GitHub commit list response",
            )
        })?;
        let seen = (r.page - 1) * r.per_page + items.len();
        Ok(HistoryPage {
            total_count: seen + usize::from(response.next.is_some()),
            incomplete_results: false,
            items,
            provider_page: r.page,
            skipped_pull_request_pages: 0,
            warnings: Vec::new(),
            listed: true,
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
                if let Some(full) = meta.full_name.as_deref() {
                    if let Some((canonical_owner, canonical_repo)) = full.split_once('/')
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
                }
                Ok((owner.into(), repo.into(), false, Vec::new()))
            }
            Err(error) if error.kind == ProviderErrorKind::NotFound => {
                Ok((owner.into(), repo.into(), false, Vec::new()))
            }
            Err(error) => Err(error),
        }
    }
    pub async fn list_issues(
        &self,
        request: &IssueListRequest,
        context: &RequestContext,
    ) -> Result<HistoryPage, ProviderError> {
        let mut provider_page = request.page;
        let mut skipped = 0usize;
        loop {
            let mut url =
                self.endpoint()
                    .rest(&["repos", &request.owner, &request.repo, "issues"])?;
            {
                let mut q = url.query_pairs_mut();
                q.append_pair("page", &provider_page.to_string())
                    .append_pair("per_page", &request.per_page.to_string());
                let state = match request.state.as_deref() {
                    Some("open") | Some("closed") => request.state.as_deref().unwrap_or("all"),
                    _ => "all",
                };
                q.append_pair("state", state);
                if let Some(sort) = request.sort.as_deref() {
                    if matches!(sort, "created" | "updated" | "comments") {
                        q.append_pair("sort", sort);
                    }
                }
                if let Some(order) = &request.order {
                    q.append_pair("direction", order);
                }
                if let Some(assignee) = &request.assignee {
                    q.append_pair("assignee", assignee);
                }
                if let Some(author) = &request.author {
                    q.append_pair("creator", author);
                }
                if let Some(mentions) = &request.mentions {
                    q.append_pair("mentioned", mentions);
                }
                if let Some(labels) = &request.labels {
                    q.append_pair("labels", &labels.join(","));
                }
            }
            let response = self.execute(RequestSpec::get(url), context).await?;
            let last_has_more = response.next.is_some();
            let raw: Vec<Value> = serde_json::from_slice(&response.body).map_err(|_| {
                ProviderError::new(
                    ProviderErrorKind::Decode,
                    "invalid GitHub issue list response",
                )
            })?;
            let items: Vec<Value> = raw
                .into_iter()
                .filter(|item| item.get("pull_request").is_none_or(Value::is_null))
                .collect();
            if !items.is_empty() || !last_has_more || skipped >= MAX_PR_ONLY_PAGES_TO_SKIP {
                let mut warnings = Vec::new();
                if skipped > 0 {
                    warnings.push(format!(
                        "Skipped {skipped} pull-request-only provider page{}; results and pagination now reflect provider page {provider_page}.",
                        if skipped == 1 { "" } else { "s" }
                    ));
                }
                if items.is_empty() && last_has_more {
                    warnings.push(format!(
                        "This provider page contained only pull requests (the GitHub issues endpoint returns both; PRs are filtered out) — follow pagination.nextPage. {}",
                        if skipped >= MAX_PR_ONLY_PAGES_TO_SKIP {
                            format!("The useful-page scan reached its {MAX_PR_ONLY_PAGES_TO_SKIP}-page skip budget.")
                        } else {
                            "Later pages may contain issues.".into()
                        }
                    ));
                }
                let seen = items.len();
                return Ok(HistoryPage {
                    total_count: seen + usize::from(last_has_more),
                    incomplete_results: false,
                    items,
                    provider_page,
                    skipped_pull_request_pages: skipped,
                    warnings,
                    listed: true,
                });
            }
            skipped += 1;
            provider_page += 1;
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
            let state = match request.state.as_deref() {
                Some("closed") | Some("open") => request.state.as_deref().unwrap_or("open"),
                Some("merged") => "closed",
                _ => "open",
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
            if let Some(order) = &request.order {
                q.append_pair("direction", order);
            }
            if let Some(head) = &request.head {
                q.append_pair("head", head);
            }
            if let Some(base) = &request.base {
                q.append_pair("base", base);
            }
        }
        let response = self.execute(RequestSpec::get(url), context).await?;
        let items: Vec<Value> = serde_json::from_slice(&response.body).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Decode,
                "invalid GitHub pull request list response",
            )
        })?;
        let seen = (request.page - 1) * request.per_page + items.len();
        Ok(HistoryPage {
            total_count: seen + usize::from(response.next.is_some()),
            incomplete_results: false,
            items,
            provider_page: request.page,
            skipped_pull_request_pages: 0,
            warnings: Vec::new(),
            listed: true,
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
        let page = self.execute(spec, context).await?;
        let value: Value = serde_json::from_slice(&page.body).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Decode,
                "invalid GitHub history search response",
            )
        })?;
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
            skipped_pull_request_pages: 0,
            warnings: Vec::new(),
            listed: false,
        })
    }
}
