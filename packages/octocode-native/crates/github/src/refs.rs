//! Repository branches, tags, and language byte counts.
use super::{
    GitHubTransport, ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext,
    RequestSpec,
};
use serde::Deserialize;

/// Which named refs a [`RefPage`] lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefKind {
    Branches,
    Tags,
}

impl RefKind {
    fn segment(self) -> &'static str {
        match self {
            Self::Branches => "branches",
            Self::Tags => "tags",
        }
    }
}

/// A branch or tag name and the commit it points at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedRef {
    pub name: String,
    pub sha: String,
}

/// One provider page of branches or tags, in GitHub's order.
#[derive(Clone, Debug, Default)]
pub struct RefPage {
    pub refs: Vec<NamedRef>,
    /// GitHub links a next page.
    pub has_more: bool,
}

#[derive(Deserialize)]
struct RefRow {
    name: String,
    commit: RefCommit,
}

#[derive(Deserialize)]
struct RefCommit {
    sha: String,
}

impl GitHubTransport {
    /// Page `page` (1-based) of a repository's branches or tags, `per_page`
    /// at a time; a missing repository is `RepositoryNotFound`.
    pub async fn repository_refs(
        &self,
        owner: &str,
        repo: &str,
        kind: RefKind,
        page: usize,
        per_page: usize,
        context: &RequestContext,
    ) -> Result<RefPage, ProviderError> {
        let mut url = self
            .endpoint()
            .rest(&["repos", owner, repo, kind.segment()])?;
        url.query_pairs_mut()
            .append_pair("per_page", &per_page.to_string())
            .append_pair("page", &page.to_string());
        let response = self
            .execute(RequestSpec::get(url), context)
            .await
            .map_err(repository_missing)?;
        let rows: Vec<RefRow> = serde_json::from_slice(&response.body).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Decode,
                format!("invalid GitHub {} response", kind.segment()),
            )
        })?;
        Ok(RefPage {
            refs: rows
                .into_iter()
                .map(|row| NamedRef {
                    name: row.name,
                    sha: row.commit.sha,
                })
                .collect(),
            has_more: response.next.is_some(),
        })
    }

    /// Bytes of code per language on the default branch, largest first.
    pub async fn repository_languages(
        &self,
        owner: &str,
        repo: &str,
        context: &RequestContext,
    ) -> Result<Vec<(String, u64)>, ProviderError> {
        let url = self.endpoint().rest(&["repos", owner, repo, "languages"])?;
        let response = self
            .execute(RequestSpec::get(url), context)
            .await
            .map_err(repository_missing)?;
        let languages: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&response.body).map_err(|_| {
                ProviderError::new(
                    ProviderErrorKind::Decode,
                    "invalid GitHub languages response",
                )
            })?;
        let mut rows = languages
            .into_iter()
            .map(|(name, bytes)| {
                bytes.as_u64().map(|bytes| (name, bytes)).ok_or_else(|| {
                    ProviderError::new(
                        ProviderErrorKind::Decode,
                        "invalid GitHub languages byte count",
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        rows.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        Ok(rows)
    }
}

fn repository_missing(error: ProviderError) -> ProviderError {
    if error.status == Some(404) {
        error.with_reason(ProviderErrorReason::RepositoryNotFound)
    } else {
        error
    }
}
