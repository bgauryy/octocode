use serde::{Deserialize, Serialize};

use super::{
    CredentialResolver, GitHubTransport, ProviderError, ProviderErrorKind, RequestContext,
    RequestSpec,
};

#[derive(Clone, Debug)]
pub struct CodeSearchRequest {
    pub include_fragments: bool,
    pub query: String,
    pub page: usize,
    pub per_page: usize,
}
#[derive(Clone, Debug)]
pub struct RepositorySearchRequest {
    pub query: String,
    pub sort: Option<String>,
    pub page: usize,
    pub per_page: usize,
}
#[derive(Clone, Debug)]
pub struct TreeRequest {
    pub owner: String,
    pub repo: String,
    pub reference: String,
    pub recursive: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CodeSearchPage {
    pub total_count: usize,
    #[serde(default)]
    pub incomplete_results: bool,
    pub items: Vec<CodeSearchItem>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CodeSearchItem {
    pub name: String,
    pub path: String,
    pub sha: String,
    pub html_url: String,
    pub repository: SearchRepository,
    #[serde(default)]
    pub text_matches: Vec<TextMatch>,
    #[serde(default, rename = "last_modified_at")]
    pub last_modified_at: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TextMatch {
    #[serde(default)]
    pub fragment: String,
    #[serde(default)]
    pub matches: Vec<TextMatchPosition>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TextMatchPosition {
    #[serde(default)]
    pub indices: Vec<i64>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SearchRepository {
    pub full_name: String,
    pub html_url: String,
    pub url: String,
    #[serde(default)]
    pub default_branch: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RepositorySearchPage {
    pub total_count: usize,
    #[serde(default)]
    pub incomplete_results: bool,
    pub items: Vec<RepositorySearchItem>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RepositorySearchItem {
    pub full_name: String,
    pub name: String,
    pub html_url: String,
    pub default_branch: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub stargazers_count: usize,
    #[serde(default)]
    pub forks_count: usize,
    #[serde(default)]
    pub open_issues_count: usize,
    #[serde(default)]
    pub visibility: String,
    #[serde(default)]
    pub topics: Vec<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub pushed_at: Option<String>,
    pub language: Option<String>,
    pub homepage: Option<String>,
    pub license: Option<License>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct License {
    pub spdx_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TreeResponse {
    pub sha: String,
    #[serde(default)]
    pub truncated: bool,
    pub tree: Vec<TreeEntry>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TreeEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub size: Option<u64>,
    pub sha: Option<String>,
    pub url: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RepositoryMetadata {
    pub default_branch: String,
    #[serde(default)]
    pub full_name: Option<String>,
    #[serde(default)]
    pub archived: bool,
}

impl<R: CredentialResolver> GitHubTransport<R> {
    pub async fn repository_auxiliary(
        &self,
        owner: &str,
        repo: &str,
        kind: &str,
        page: usize,
        per_page: usize,
        context: &RequestContext,
    ) -> Result<(serde_json::Value, bool), ProviderError> {
        let mut url = self.endpoint().rest(&["repos", owner, repo, kind])?;
        url.query_pairs_mut()
            .append_pair("page", &page.to_string())
            .append_pair("per_page", &per_page.to_string());
        let response = self.execute(RequestSpec::get(url), context).await?;
        let more = response.next.is_some();
        Ok((
            decode(
                response.body.as_ref(),
                "invalid GitHub repository metadata response",
            )?,
            more,
        ))
    }
    pub async fn search_code(
        &self,
        request: &CodeSearchRequest,
        context: &RequestContext,
    ) -> Result<CodeSearchPage, ProviderError> {
        let mut url = self.endpoint().rest(&["search", "code"])?;
        url.query_pairs_mut()
            .append_pair("q", &request.query)
            .append_pair("page", &request.page.to_string())
            .append_pair("per_page", &request.per_page.to_string());
        let mut spec = RequestSpec::get(url);
        spec.headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static(if request.include_fragments {
                "application/vnd.github.v3.text-match+json"
            } else {
                "application/vnd.github+json"
            }),
        );
        decode(
            self.execute(spec, context).await?.body.as_ref(),
            "invalid GitHub code search response",
        )
    }
    pub async fn search_repositories(
        &self,
        request: &RepositorySearchRequest,
        context: &RequestContext,
    ) -> Result<RepositorySearchPage, ProviderError> {
        let mut url = self.endpoint().rest(&["search", "repositories"])?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs
                .append_pair("q", &request.query)
                .append_pair("page", &request.page.to_string())
                .append_pair("per_page", &request.per_page.to_string());
            if let Some(sort) = &request.sort {
                pairs.append_pair("sort", sort);
            }
        }
        let mut spec = RequestSpec::get(url);
        spec.headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/vnd.github.v3+json"),
        );
        decode(
            self.execute(spec, context).await?.body.as_ref(),
            "invalid GitHub repository search response",
        )
    }
    pub async fn list_owner_repositories(
        &self,
        owner: &str,
        page: usize,
        per_page: usize,
        context: &RequestContext,
    ) -> Result<(Vec<RepositorySearchItem>, bool), ProviderError> {
        let fetch = |kind: &str, name: &str| -> Result<RequestSpec, ProviderError> {
            let mut url = self.endpoint().rest(&["users", name, "repos"])?;
            if kind == "orgs" {
                url = self.endpoint().rest(&["orgs", name, "repos"])?;
            }
            url.query_pairs_mut()
                .append_pair("page", &page.to_string())
                .append_pair("per_page", &per_page.to_string());
            Ok(RequestSpec::get(url))
        };
        let response = match self.execute(fetch("orgs", owner)?, context).await {
            Ok(v) => v,
            Err(e) if e.status == Some(404) => {
                self.execute(fetch("users", owner)?, context).await?
            }
            Err(e) => return Err(e),
        };
        let has_more = response.next.is_some();
        Ok((
            decode(
                response.body.as_ref(),
                "invalid GitHub owner repositories response",
            )?,
            has_more,
        ))
    }
    pub async fn get_tree(
        &self,
        request: &TreeRequest,
        context: &RequestContext,
    ) -> Result<TreeResponse, ProviderError> {
        let mut url = self.endpoint().rest(&[
            "repos",
            &request.owner,
            &request.repo,
            "git",
            "trees",
            &request.reference,
        ])?;
        if request.recursive {
            url.query_pairs_mut().append_pair("recursive", "1");
        }
        decode(
            self.execute(RequestSpec::get(url), context)
                .await?
                .body
                .as_ref(),
            "invalid GitHub tree response",
        )
    }
    pub async fn repository_metadata(
        &self,
        owner: &str,
        repo: &str,
        context: &RequestContext,
    ) -> Result<RepositoryMetadata, ProviderError> {
        let url = self.endpoint().rest(&["repos", owner, repo])?;
        decode(
            self.execute(RequestSpec::get(url), context)
                .await?
                .body
                .as_ref(),
            "invalid GitHub repository metadata response",
        )
    }
}
fn decode<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
    message: &'static str,
) -> Result<T, ProviderError> {
    serde_json::from_slice(bytes)
        .map_err(|_| ProviderError::new(ProviderErrorKind::Decode, message))
}
