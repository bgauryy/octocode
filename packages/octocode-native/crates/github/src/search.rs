//! GitHub search and repository reads: the request and page types; the
//! searches, their response store and the repository reads are child modules.
use serde::{Deserialize, Serialize};

use super::{ProviderError, ProviderErrorKind};

mod cache;
mod code;
mod repository;

pub(crate) use cache::Lookup;
pub(crate) use repository::{OwnerSources, REPOSITORY_MEMO_KEYS, RepositoryMemo};

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
    /// Organizations whose results GitHub withheld because the token is not
    /// SAML-authorized for them (`x-github-sso: partial-results`). A page
    /// with any is never cached.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sso_partial_organizations: Vec<u64>,
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
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub fork: bool,
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
    /// The git file mode: a `blob` of mode `120000` is a symlink.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
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

/// A search or repository body decoded as `T`, or `message` as a decode error.
fn decode<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
    message: &'static str,
) -> Result<T, ProviderError> {
    serde_json::from_slice(bytes)
        .map_err(|_| ProviderError::new(ProviderErrorKind::Decode, message))
}
