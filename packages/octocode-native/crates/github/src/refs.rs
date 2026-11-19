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

/// A tag of a repository as its git ref names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagRef {
    pub name: String,
    /// The object its ref names; `None` when GitHub left it out.
    pub object: Option<TagObject>,
}

/// The object a tag's ref names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TagObject {
    /// A lightweight tag: the object it tags, a commit (or, rarely, a tree
    /// or blob).
    Tagged(String),
    /// An annotated tag: its tag object, whose target
    /// [`GitHubTransport::tag_commits`] reads.
    Annotated(String),
}

/// What one tag object targets: another tag object, or the object it tags.
#[derive(Clone, Debug, PartialEq, Eq)]
enum TagTarget {
    Tag(String),
    Tagged(String),
}

/// The GraphQL read of `count` tag objects' targets, `$t0`… in order: one
/// request for a page of tags instead of one per tag.
fn tag_targets_document(count: usize) -> String {
    let declared = (0..count)
        .map(|at| format!(",$t{at}:GitObjectID!"))
        .collect::<String>();
    let selected = (0..count)
        .map(|at| format!("t{at}:object(oid:$t{at}){{...on Tag{{target{{__typename oid}}}}}}"))
        .collect::<String>();
    format!(
        "query($owner:String!,$repo:String!{declared}){{repository(owner:$owner,name:$repo){{{selected}}}}}"
    )
}

/// The target GraphQL's `object` answer names for `object`: a `Tag`'s
/// target, or `object` itself when it is no tag; `None` when GitHub named
/// no object.
fn graphql_tag_target(object: &str, answer: &serde_json::Value) -> Option<TagTarget> {
    if !answer.is_object() {
        return None;
    }
    let Some(target) = answer.get("target") else {
        return Some(TagTarget::Tagged(object.to_owned()));
    };
    let oid = target.get("oid")?.as_str()?.to_owned();
    Some(
        match target.get("__typename").and_then(serde_json::Value::as_str) {
            Some("Tag") => TagTarget::Tag(oid),
            _ => TagTarget::Tagged(oid),
        },
    )
}

/// `refs/tags/{prefix}` as route segments under `matching-refs/tags`: none
/// for no prefix, or for one GitHub's route cannot carry (a `.`, `..` or
/// empty segment before the last), which then lists every tag.
fn prefix_segments(prefix: &str) -> Vec<&str> {
    let segments = prefix.split('/').collect::<Vec<_>>();
    let routable = segments.iter().enumerate().all(|(at, segment)| {
        !matches!(*segment, "." | "..") && (at + 1 == segments.len() || !segment.is_empty())
    });
    if prefix.is_empty() || !routable {
        Vec::new()
    } else {
        segments
    }
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

impl From<RefRow> for NamedRef {
    fn from(row: RefRow) -> Self {
        Self {
            name: row.name,
            sha: row.commit.sha,
        }
    }
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
            refs: rows.into_iter().map(NamedRef::from).collect(),
            has_more: response.next.is_some(),
        })
    }

    /// Every ref of `kind` with the commit it names, every page read
    /// ([`Self::list_all`]).
    pub async fn all_refs(
        &self,
        owner: &str,
        repo: &str,
        kind: RefKind,
        context: &RequestContext,
    ) -> Result<Vec<NamedRef>, ProviderError> {
        let rows: Vec<RefRow> = self
            .list_all(
                &["repos", owner, repo, kind.segment()],
                kind.segment(),
                context,
            )
            .await?;
        Ok(rows.into_iter().map(NamedRef::from).collect())
    }

    /// Every tag of a repository whose name starts with `prefix` (every tag
    /// for `""`), from its git refs: the whole set a caller orders itself,
    /// which GitHub's tag pages (reverse-lexicographic) cannot give. GitHub
    /// answers the prefix itself (`matching-refs`), so one release line
    /// reads only its own refs. Each tag keeps the object its ref names, so a
    /// lightweight tag's commit needs no further read
    /// ([`Self::tag_commits`]). A repository with no commits has none.
    pub async fn repository_tag_refs(
        &self,
        owner: &str,
        repo: &str,
        prefix: &str,
        context: &RequestContext,
    ) -> Result<Vec<TagRef>, ProviderError> {
        #[derive(Deserialize)]
        struct Row {
            #[serde(rename = "ref")]
            name: String,
            #[serde(default)]
            object: Option<RowObject>,
        }
        #[derive(Deserialize)]
        struct RowObject {
            sha: Option<String>,
            #[serde(rename = "type")]
            kind: Option<String>,
        }
        let mut segments = vec!["repos", owner, repo, "git", "matching-refs", "tags"];
        segments.extend(prefix_segments(prefix));
        let rows: Vec<Row> = match self.list_all(&segments, "tag refs", context).await {
            Ok(rows) => rows,
            // GitHub answers an empty repository's refs with 409.
            Err(error) if error.status == Some(409) => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let name = row.name.strip_prefix("refs/tags/")?.to_owned();
                let object = row.object.and_then(|object| {
                    let sha = object.sha?;
                    Some(match object.kind.as_deref() {
                        Some("tag") => TagObject::Annotated(sha),
                        _ => TagObject::Tagged(sha),
                    })
                });
                Some(TagRef { name, object })
            })
            .filter(|tag| tag.name.starts_with(prefix))
            .collect())
    }

    /// The commit each of `tags` names, in order: a lightweight tag's
    /// listed object, every annotated tag's target read together
    /// ([`Self::peel_tags`]), and a tag listed without its object read by
    /// name ([`Self::tag_commit`]). `None`: no longer a tag of the
    /// repository.
    pub async fn tag_commits(
        &self,
        owner: &str,
        repo: &str,
        tags: &[TagRef],
        context: &RequestContext,
    ) -> Result<Vec<Option<String>>, ProviderError> {
        let annotated = tags
            .iter()
            .filter_map(|tag| match &tag.object {
                Some(TagObject::Annotated(object)) => Some(object.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut peeled = self.peel_tags(owner, repo, &annotated, context).await?;
        let unlisted = futures_util::future::join_all(tags.iter().map(|tag| async {
            match tag.object {
                None => self.tag_commit(owner, repo, &tag.name, context).await,
                Some(_) => Ok(None),
            }
        }))
        .await;
        tags.iter()
            .zip(unlisted)
            .map(|(tag, unlisted)| {
                Ok(match &tag.object {
                    Some(TagObject::Tagged(object)) => Some(object.clone()),
                    Some(TagObject::Annotated(object)) => peeled.remove(object),
                    None => unlisted?,
                })
            })
            .collect()
    }

    /// The object each annotated tag object in `objects` finally tags, by
    /// tag object SHA: a tag of a tag is followed to what the last one
    /// tags. Each step reads every pending target in one GraphQL request
    /// when the request can use GraphQL ([`Self::tag_targets`]). An object
    /// GitHub no longer has is left out.
    pub(crate) async fn peel_tags(
        &self,
        owner: &str,
        repo: &str,
        objects: &[String],
        context: &RequestContext,
    ) -> Result<std::collections::HashMap<String, String>, ProviderError> {
        let mut peeled = std::collections::HashMap::new();
        let mut seen = std::collections::HashSet::new();
        // In the listing's order, each object once: the read names the same
        // objects in the same order every time, so its kept answer serves a
        // repeated listing (a set's order would change the request).
        let mut pending = objects
            .iter()
            .filter(|object| seen.insert((*object).clone()))
            .map(|object| (object.clone(), object.clone()))
            .collect::<Vec<_>>();
        while !pending.is_empty() {
            let current = pending
                .iter()
                .map(|(_, current)| current.clone())
                .collect::<Vec<_>>();
            let targets = self.tag_targets(owner, repo, &current, context).await?;
            let mut next = Vec::new();
            for (object, current) in pending {
                match targets.get(&current) {
                    Some(TagTarget::Tagged(tagged)) => {
                        peeled.insert(object, tagged.clone());
                    }
                    // A tag object names a tag object already followed only
                    // in an answer no git history can hold: none is taken.
                    Some(TagTarget::Tag(inner)) if seen.insert(inner.clone()) => {
                        next.push((object, inner.clone()));
                    }
                    Some(TagTarget::Tag(_)) | None => {}
                }
            }
            pending = next;
        }
        Ok(peeled)
    }

    /// What each tag object in `objects` targets: one GraphQL request per
    /// [`crate::MAX_PER_PAGE`] objects (a refs page at most) when the
    /// request carries a credential and GraphQL admits it, else, and for any
    /// object that read left unanswered, the object's REST read.
    async fn tag_targets(
        &self,
        owner: &str,
        repo: &str,
        objects: &[String],
        context: &RequestContext,
    ) -> Result<std::collections::HashMap<String, TagTarget>, ProviderError> {
        let mut targets = std::collections::HashMap::new();
        if context.resolved_credential().is_some() && self.graphql_available(context) {
            for batch in objects.chunks(crate::MAX_PER_PAGE) {
                let mut variables = serde_json::json!({ "owner": owner, "repo": repo });
                for (at, object) in batch.iter().enumerate() {
                    variables[format!("t{at}")] = serde_json::json!(object);
                }
                // A failed read leaves its objects to REST, which answers the
                // same. Tag objects cannot change: an answer naming each is
                // kept, so a repeated refs listing sends nothing.
                let named_each = |page: &crate::GraphQlPage| {
                    let repository = page.data.as_ref().and_then(|data| data.get("repository"));
                    (0..batch.len()).all(|at| {
                        repository
                            .and_then(|repository| repository.get(format!("t{at}")))
                            .is_some_and(serde_json::Value::is_object)
                    })
                };
                let Ok(page) = self
                    .execute_object_graphql(
                        &tag_targets_document(batch.len()),
                        variables,
                        context,
                        named_each,
                    )
                    .await
                else {
                    continue;
                };
                let repository = page.data.as_ref().and_then(|data| data.get("repository"));
                for (at, object) in batch.iter().enumerate() {
                    let answer = repository.and_then(|repository| repository.get(format!("t{at}")));
                    if let Some(target) =
                        answer.and_then(|answer| graphql_tag_target(object, answer))
                    {
                        targets.insert(object.clone(), target);
                    }
                }
            }
        }
        let unanswered = objects
            .iter()
            .filter(|object| !targets.contains_key(*object))
            .collect::<Vec<_>>();
        if unanswered.is_empty() {
            return Ok(targets);
        }
        context.check()?;
        let answers = futures_util::future::join_all(
            unanswered
                .iter()
                .map(|object| self.rest_tag_target(owner, repo, object, context)),
        )
        .await;
        for (object, answer) in unanswered.into_iter().zip(answers) {
            if let Some(target) = answer? {
                targets.insert(object.clone(), target);
            }
        }
        Ok(targets)
    }

    /// What tag object `object` targets, by its REST read; `None` when
    /// GitHub has no such tag object.
    async fn rest_tag_target(
        &self,
        owner: &str,
        repo: &str,
        object: &str,
        context: &RequestContext,
    ) -> Result<Option<TagTarget>, ProviderError> {
        let url = self
            .endpoint()
            .rest(&["repos", owner, repo, "git", "tags", object])?;
        let page = match self.execute(RequestSpec::get(url), context).await {
            Ok(page) => page,
            Err(error) if error.status == Some(404) => return Ok(None),
            Err(error) => return Err(error),
        };
        let value: serde_json::Value = serde_json::from_slice(&page.body).map_err(|_| {
            ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub tag response")
        })?;
        let field = |name: &str| {
            value
                .pointer(&format!("/object/{name}"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        Ok(field("sha").map(|sha| match field("type").as_deref() {
            Some("tag") => TagTarget::Tag(sha),
            _ => TagTarget::Tagged(sha),
        }))
    }

    /// Every item of the list `segments` names (a `repos/{owner}/{repo}/…`
    /// route), [`crate::MAX_PER_PAGE`] per page, by our own page number
    /// (whatever origin a forwarded `Link` names) until GitHub links no next
    /// page, bounded by the request's deadline. A missing repository is
    /// `RepositoryNotFound`.
    pub(crate) async fn list_all<T: for<'de> Deserialize<'de>>(
        &self,
        segments: &[&str],
        what: &str,
        context: &RequestContext,
    ) -> Result<Vec<T>, ProviderError> {
        let mut items = Vec::new();
        for page in 1_usize.. {
            let mut url = self.endpoint().rest(segments)?;
            url.query_pairs_mut()
                .append_pair("per_page", &crate::MAX_PER_PAGE.to_string())
                .append_pair("page", &page.to_string());
            let response = self
                .execute(RequestSpec::get(url), context)
                .await
                .map_err(repository_missing)?;
            items.extend(
                serde_json::from_slice::<Vec<T>>(&response.body).map_err(|_| {
                    ProviderError::new(
                        ProviderErrorKind::Decode,
                        format!("invalid GitHub {what} response"),
                    )
                })?,
            );
            if response.next.is_none() {
                break;
            }
        }
        Ok(items)
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
    pub async fn tag_commit(
        &self,
        owner: &str,
        repo: &str,
        tag: &str,
        context: &RequestContext,
    ) -> Result<Option<String>, ProviderError> {
        let Some((object, kind)) = self
            .named_ref_object(owner, repo, "tags", tag, context)
            .await?
        else {
            return Ok(None);
        };
        if kind != "tag" {
            return Ok(Some(object));
        }
        // An annotated tag's ref names the tag object; its target is the commit.
        Ok(self
            .peel_tags(owner, repo, std::slice::from_ref(&object), context)
            .await?
            .remove(&object))
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
        Ok(self
            .named_ref_object(owner, repo, namespace, name, context)
            .await?
            .map(|(sha, _)| sha))
    }

    /// The object `refs/{namespace}/{name}` points at, as its SHA and type;
    /// `None` when it does not exist (a ref name keeps its `/`, one path
    /// segment each).
    async fn named_ref_object(
        &self,
        owner: &str,
        repo: &str,
        namespace: &str,
        name: &str,
        context: &RequestContext,
    ) -> Result<Option<(String, String)>, ProviderError> {
        let mut segments = vec!["repos", owner, repo, "git", "ref", namespace];
        segments.extend(name.split('/'));
        let url = self.endpoint().rest(&segments)?;
        match self.execute(RequestSpec::get(url), context).await {
            Ok(response) => {
                let value: serde_json::Value =
                    serde_json::from_slice(&response.body).map_err(|_| {
                        ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub ref response")
                    })?;
                let field = |name: &str| {
                    value
                        .pointer(&format!("/object/{name}"))
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                };
                Ok(field("sha").map(|sha| (sha, field("type").unwrap_or_default())))
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

pub(crate) fn repository_missing(error: ProviderError) -> ProviderError {
    if error.status == Some(404) {
        error.with_reason(ProviderErrorReason::RepositoryNotFound)
    } else {
        error
    }
}
