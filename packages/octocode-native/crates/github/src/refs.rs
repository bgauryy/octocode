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

/// What a name resolves to, as far as GitHub's refs say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefMatch {
    Branch,
    Tag,
    /// Neither a branch nor a tag: a commit SHA (or a prefix of one).
    Commit,
}

impl RefMatch {
    /// The output spelling (`refKind`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Branch => "branch",
            Self::Tag => "tag",
            Self::Commit => "commit",
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

    /// Every tag name of a repository, from its git refs (one listing,
    /// following any next link): the whole set a caller orders itself,
    /// which GitHub's tag pages (reverse-lexicographic) cannot give. A
    /// repository with no commits has none.
    pub async fn repository_tag_names(
        &self,
        owner: &str,
        repo: &str,
        context: &RequestContext,
    ) -> Result<Vec<String>, ProviderError> {
        #[derive(Deserialize)]
        struct TagRef {
            #[serde(rename = "ref")]
            name: String,
        }
        let mut url =
            Some(
                self.endpoint()
                    .rest(&["repos", owner, repo, "git", "matching-refs", "tags"])?,
            );
        let mut names = Vec::new();
        // GitHub answers the whole listing at once; a linked page is still
        // read, bounded by the request's deadline.
        while let Some(at) = url.take() {
            let response = match self.execute(RequestSpec::get(at), context).await {
                Ok(response) => response,
                // GitHub answers an empty repository's refs with 409.
                Err(error) if error.status == Some(409) => return Ok(names),
                Err(error) => return Err(repository_missing(error)),
            };
            let rows: Vec<TagRef> = serde_json::from_slice(&response.body).map_err(|_| {
                ProviderError::new(
                    ProviderErrorKind::Decode,
                    "invalid GitHub tag refs response",
                )
            })?;
            names.extend(
                rows.into_iter()
                    .filter_map(|row| row.name.strip_prefix("refs/tags/").map(str::to_owned)),
            );
            url = response.next;
        }
        Ok(names)
    }

    /// Whether `reference` names a branch, a tag, or neither (a commit),
    /// given the `commit` it resolved to. A name that is both resolves to
    /// the kind whose ref points at `commit`, a branch first.
    pub async fn reference_kind(
        &self,
        owner: &str,
        repo: &str,
        reference: &str,
        commit: &str,
        context: &RequestContext,
    ) -> Result<RefMatch, ProviderError> {
        let (branch, tag) = tokio::try_join!(
            self.named_ref_sha(owner, repo, "heads", reference, context),
            self.named_ref_sha(owner, repo, "tags", reference, context),
        )?;
        Ok(match (branch, tag) {
            (Some(sha), Some(_)) if sha != commit => RefMatch::Tag,
            (Some(_), _) => RefMatch::Branch,
            (None, Some(_)) => RefMatch::Tag,
            (None, None) => RefMatch::Commit,
        })
    }

    /// Whether `tag` names a tag of `owner/repo`: its tag ref exists (a
    /// branch of the same name does not count).
    pub async fn tag_exists(
        &self,
        owner: &str,
        repo: &str,
        tag: &str,
        context: &RequestContext,
    ) -> Result<bool, ProviderError> {
        Ok(self
            .named_ref_sha(owner, repo, "tags", tag, context)
            .await?
            .is_some())
    }

    /// The object SHA of `refs/{namespace}/{name}`; `None` when it does not
    /// exist (a ref name keeps its `/`, one path segment each).
    async fn named_ref_sha(
        &self,
        owner: &str,
        repo: &str,
        namespace: &str,
        name: &str,
        context: &RequestContext,
    ) -> Result<Option<String>, ProviderError> {
        let mut segments = vec!["repos", owner, repo, "git", "ref", namespace];
        segments.extend(name.split('/'));
        let url = self.endpoint().rest(&segments)?;
        match self.execute(RequestSpec::get(url), context).await {
            Ok(response) => {
                let value: serde_json::Value =
                    serde_json::from_slice(&response.body).map_err(|_| {
                        ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub ref response")
                    })?;
                Ok(value
                    .pointer("/object/sha")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned))
            }
            Err(error) if error.status == Some(404) => Ok(None),
            Err(error) => Err(error),
        }
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
