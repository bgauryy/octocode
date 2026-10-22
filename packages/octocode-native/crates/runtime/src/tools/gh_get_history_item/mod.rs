//! GitHub history *item fetch* — a single pull request, issue, commit, or
//! comparison by number / ref.
//!
//! History items are mutable (comments are added, reviews change, commits
//! land), so every REST read revalidates its stored copy (an ETag
//! conditional request: a 304 costs no body and no primary rate limit);
//! a read pinned to a commit SHA is immutable.
//!
//! Layout: this module owns the public query, validation, dispatch and the
//! response boundary (sanitize, size cap). `pull_request`/`pr_sections`,
//! `issue`, `commit` and `compare` shape each operation (`file_page` the
//! changed-file page the last two share); `filter`, `inventory` and `patch`
//! own changed files and patch windows, `window` provider-batch paging,
//! `axis` the pagination axes, `remaining` what a page leaves, `pr_menu`,
//! `patch_hop` and `promotion` every `next.*`, and `graphql` the PR fast path.
use crate::num::usize_of;
use crate::response::result::{ToolData, remove_nulls};
use crate::security::scan::{ContentScan, sanitize_provider_text};
use octocode_github::{
    GitHubTransport, ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext,
};
use serde::Deserialize as _;
use serde_json::Value;
use std::path::Path;

/// The path the content scanner sees for history-item provider text.
const SCAN_PATH: &str = "github-history-item";

#[cfg(test)]
mod anchor_tests;
mod axis;
mod commit;
mod compare;
mod file_page;
mod filter;
mod graphql;
#[cfg(test)]
mod graphql_tests;
mod inventory;
mod issue;
mod output;
mod patch;
mod patch_hop;
mod pr_finish;
mod pr_menu;
mod pr_sections;
mod promotion;
mod pull_request;
mod recovery;
mod remaining;
mod text;
mod util;
mod window;

/// Items per page of a provider collection (comments, reviews, commits,
/// patch file pages) when `pageSize` is omitted: the contract default the
/// issue variant declares. Pull requests, commits and comparisons declare
/// none (an omitted patch-free inventory sizes itself to the response page)
/// and page their collections by the same default.
pub(crate) const DEFAULT_PAGE_SIZE: usize =
    crate::contracts::query_limits::gh_get_history_item::issue::PAGE_SIZE_DEFAULT;
/// Characters per body, comment or review window when `length` is omitted
/// (contract `ghGetHistoryItem.semanticPolicy.textWindowChars`).
const DEFAULT_TEXT_WINDOW: usize = crate::id::gh_get_history_item_policy::TEXT_WINDOW_CHARS;

pub use octocode_contract_types::GhGetHistoryItemQuery;
pub(crate) use output::Output;
pub use recovery::recovery;

/// The operation discriminant of a [`GhGetHistoryItemQuery`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ItemOperation {
    PullRequest,
    Issue,
    Commit,
    Compare,
}

/// A validated wire query plus runtime-only execution settings.
#[derive(Clone, Debug)]
pub struct HistoryItemRequest {
    pub query: GhGetHistoryItemQuery,
    /// The sections the row reads, resolved once from the typed query
    /// ([`Sections::resolve`]).
    sections: Sections,
    /// The response page this row's output fits: the call's explicit page
    /// (`responseLength`) or the configured automatic page, shared by the
    /// rows of one call.
    pub auto_page_chars: Option<usize>,
    /// File text at the pull request's head that widens `matchString` hunks
    /// past GitHub's diff context (read once per file and call).
    head_sources: Option<std::sync::Arc<patch::hunks::HeadSources>>,
    /// `query` as a wire row, serialized once: every continuation starts
    /// from it ([`pr_menu::base_public_query`]).
    public: Value,
}

impl HistoryItemRequest {
    /// Parses a validated `ghGetHistoryItem` row (owned or borrowed: the
    /// typed query is read from it without a copy).
    pub fn from_row(row: impl std::borrow::Borrow<Value>) -> Result<Self, serde_json::Error> {
        let query = GhGetHistoryItemQuery::deserialize(row.borrow())?;
        Ok(Self {
            sections: Sections::resolve(&query),
            public: serde_json::to_value(&query)?,
            query,
            auto_page_chars: None,
            head_sources: None,
        })
    }
    /// The sections the row reads.
    pub(crate) fn sections(&self) -> Sections {
        self.sections
    }
    /// Commit/compare (and pull-request) rows that read patch text.
    pub(crate) fn include_diff(&self) -> bool {
        self.sections.has(Section::Patches)
    }
}

/// One section of a history item (`body`, `files`, `patches`, `comments`,
/// `reviewComments`, `reviews`, `commits`, `commitFiles`).
pub(crate) use octocode_contract_types::GhGetHistoryItemQuerySectionsItem as Section;

/// The sections one read returns: the generated `sections`, with the one
/// default resolved (a `matchString` with no section named reads the
/// patches it searches). Continuations set it back on the wire in the
/// published order ([`Sections::wire`]).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Sections(u8);

impl Sections {
    /// Every section, in the published order.
    const ORDER: [Section; 8] = [
        Section::Body,
        Section::Files,
        Section::Patches,
        Section::Comments,
        Section::ReviewComments,
        Section::Reviews,
        Section::Commits,
        Section::CommitFiles,
    ];

    fn bit(section: Section) -> u8 {
        let index = Self::ORDER
            .iter()
            .position(|known| *known == section)
            .unwrap_or_default();
        1 << index
    }
    /// The sections named.
    pub(crate) fn of(sections: impl IntoIterator<Item = Section>) -> Self {
        sections
            .into_iter()
            .fold(Self::default(), |set, section| set.with(section))
    }
    /// The sections `query` reads.
    fn resolve(query: &GhGetHistoryItemQuery) -> Self {
        let named = match query {
            GhGetHistoryItemQuery::PullRequest { sections, .. }
            | GhGetHistoryItemQuery::Issue { sections, .. }
            | GhGetHistoryItemQuery::Commit { sections, .. }
            | GhGetHistoryItemQuery::Compare { sections, .. } => Self::of(sections.iter().copied()),
        };
        if named.is_empty() && gh_get_history_item_query::match_string(query).is_some() {
            Self::of([Section::Patches])
        } else {
            named
        }
    }
    pub(crate) fn has(self, section: Section) -> bool {
        self.0 & Self::bit(section) != 0
    }
    pub(crate) fn with(self, section: Section) -> Self {
        Self(self.0 | Self::bit(section))
    }
    pub(crate) fn is_empty(self) -> bool {
        self.0 == 0
    }
    /// The read lists changed files: as an inventory or with patches.
    pub(crate) fn lists_files(self) -> bool {
        self.has(Section::Files) || self.has(Section::Patches)
    }
    /// The read lists commits (with or without each commit's paths).
    pub(crate) fn lists_commits(self) -> bool {
        self.has(Section::Commits) || self.has(Section::CommitFiles)
    }
    /// The published `sections` value, in published order; `None` when
    /// empty (the summary read).
    pub(crate) fn wire(self) -> Option<Value> {
        let names = Self::ORDER
            .into_iter()
            .filter(|section| self.has(*section))
            .map(|section| Value::from(section.to_string()))
            .collect::<Vec<_>>();
        (!names.is_empty()).then_some(Value::Array(names))
    }
    /// Set these sections on a working continuation query (or clear them
    /// for the summary read).
    pub(crate) fn set_on(self, query: &mut Value) {
        if let Some(fields) = query.as_object_mut() {
            match self.wire() {
                Some(sections) => fields.insert("sections".into(), sections),
                None => fields.remove("sections"),
            };
        }
    }
}

impl HistoryItemRequest {
    /// This comparison with `base`/`head` replaced (resolved commits), for
    /// continuations that must read the same two commits. A ref the wire
    /// contract rejects (an empty one) is a validation error, never a
    /// silent fall back to the unpinned ref.
    pub(super) fn with_compare_refs(&self, base: &str, head: &str) -> Result<Self, ProviderError> {
        use octocode_contract_types::{GhGetHistoryItemQueryBase, GhGetHistoryItemQueryHead};
        let mut pinned = self.clone();
        if let GhGetHistoryItemQuery::Compare {
            base: pinned_base,
            head: pinned_head,
            ..
        } = &mut pinned.query
        {
            *pinned_base = GhGetHistoryItemQueryBase::try_from(base)
                .map_err(|error| validation(&format!("compare base: {error}")))?;
            *pinned_head = GhGetHistoryItemQueryHead::try_from(head)
                .map_err(|error| validation(&format!("compare head: {error}")))?;
            // The cached wire row follows the pinned refs, or every
            // continuation would restate the caller's moving refs.
            pinned.public = serde_json::to_value(&pinned.query)
                .map_err(|error| validation(&format!("compare query: {error}")))?;
        }
        Ok(pinned)
    }
}

impl std::ops::Deref for HistoryItemRequest {
    type Target = GhGetHistoryItemQuery;
    fn deref(&self) -> &GhGetHistoryItemQuery {
        &self.query
    }
}

/// Operation-independent views over the generated wire query, in the
/// engine's `usize` units.
pub(crate) mod gh_get_history_item_query {
    use super::*;
    /// Any `sections` named: the read returns what they name.
    pub(crate) fn has_sections(query: &GhGetHistoryItemQuery) -> bool {
        match query {
            GhGetHistoryItemQuery::PullRequest { sections, .. } => !sections.is_empty(),
            GhGetHistoryItemQuery::Issue { sections, .. } => !sections.is_empty(),
            GhGetHistoryItemQuery::Commit { sections, .. } => !sections.is_empty(),
            GhGetHistoryItemQuery::Compare { sections, .. } => !sections.is_empty(),
        }
    }
    pub(super) fn operation(query: &GhGetHistoryItemQuery) -> ItemOperation {
        match query {
            GhGetHistoryItemQuery::PullRequest { .. } => ItemOperation::PullRequest,
            GhGetHistoryItemQuery::Issue { .. } => ItemOperation::Issue,
            GhGetHistoryItemQuery::Commit { .. } => ItemOperation::Commit,
            GhGetHistoryItemQuery::Compare { .. } => ItemOperation::Compare,
        }
    }
    pub(crate) fn owner(query: &GhGetHistoryItemQuery) -> &str {
        match query {
            GhGetHistoryItemQuery::PullRequest { owner, .. }
            | GhGetHistoryItemQuery::Issue { owner, .. }
            | GhGetHistoryItemQuery::Commit { owner, .. }
            | GhGetHistoryItemQuery::Compare { owner, .. } => owner.as_str(),
        }
    }
    pub(crate) fn repo(query: &GhGetHistoryItemQuery) -> &str {
        match query {
            GhGetHistoryItemQuery::PullRequest { repo, .. }
            | GhGetHistoryItemQuery::Issue { repo, .. }
            | GhGetHistoryItemQuery::Commit { repo, .. }
            | GhGetHistoryItemQuery::Compare { repo, .. } => repo.as_str(),
        }
    }
    pub(crate) fn number(query: &GhGetHistoryItemQuery) -> Option<u64> {
        match query {
            GhGetHistoryItemQuery::PullRequest { number, .. }
            | GhGetHistoryItemQuery::Issue { number, .. } => Some(number.0.get()),
            _ => None,
        }
    }
    pub(crate) fn reference(query: &GhGetHistoryItemQuery) -> Option<&str> {
        match query {
            GhGetHistoryItemQuery::Commit { ref_, .. } => Some(ref_.as_str()),
            _ => None,
        }
    }
    pub(crate) fn base(query: &GhGetHistoryItemQuery) -> Option<&str> {
        match query {
            GhGetHistoryItemQuery::Compare { base, .. } => Some(base.as_str()),
            _ => None,
        }
    }
    pub(crate) fn head(query: &GhGetHistoryItemQuery) -> Option<&str> {
        match query {
            GhGetHistoryItemQuery::Compare { head, .. } => Some(head.as_str()),
            _ => None,
        }
    }
    /// The `include` path patterns (pull request, commit, comparison).
    pub(crate) fn include_patterns(query: &GhGetHistoryItemQuery) -> Vec<&str> {
        match query {
            GhGetHistoryItemQuery::PullRequest { include, .. }
            | GhGetHistoryItemQuery::Commit { include, .. }
            | GhGetHistoryItemQuery::Compare { include, .. } => include
                .iter()
                .flat_map(|include| include.iter())
                .map(|pattern| pattern.as_str())
                .collect(),
            GhGetHistoryItemQuery::Issue { .. } => Vec::new(),
        }
    }
    /// The pull-request head a page continuation is pinned to (`headSha`).
    pub(crate) fn head_sha(query: &GhGetHistoryItemQuery) -> Option<&str> {
        match query {
            GhGetHistoryItemQuery::PullRequest { head_sha, .. } => {
                head_sha.as_deref().map(String::as_str)
            }
            _ => None,
        }
    }
    /// The page of the section a read lists (`page`): every page
    /// continuation names the one section it pages.
    pub(crate) fn page(query: &GhGetHistoryItemQuery) -> Option<usize> {
        match query {
            GhGetHistoryItemQuery::PullRequest { page, .. }
            | GhGetHistoryItemQuery::Issue { page, .. }
            | GhGetHistoryItemQuery::Commit { page, .. }
            | GhGetHistoryItemQuery::Compare { page, .. } => {
                page.as_ref().map(|page| usize_of(page.0.get()))
            }
        }
    }
    pub(crate) fn page_size(query: &GhGetHistoryItemQuery) -> Option<usize> {
        match query {
            GhGetHistoryItemQuery::PullRequest { page_size, .. }
            | GhGetHistoryItemQuery::Commit { page_size, .. }
            | GhGetHistoryItemQuery::Compare { page_size, .. } => {
                page_size.map(|size| usize_of(size.get()))
            }
            GhGetHistoryItemQuery::Issue { page_size, .. } => {
                page_size.as_ref().map(|size| usize_of(size.0.get()))
            }
        }
    }
    /// Items per page of a provider collection (comments, reviews, commits,
    /// patch file pages): `pageSize` capped at one provider batch.
    pub(crate) fn collection_page_size(query: &GhGetHistoryItemQuery) -> usize {
        page_size(query)
            .unwrap_or(DEFAULT_PAGE_SIZE)
            .clamp(1, octocode_github::MAX_PER_PAGE)
    }
    /// Whether a pull request narrows its changed files (`include`,
    /// `status`, `minChanges`).
    pub(crate) fn has_file_filter(query: &GhGetHistoryItemQuery) -> bool {
        matches!(
            query,
            GhGetHistoryItemQuery::PullRequest { include, status, min_changes, .. }
                if include.is_some() || status.is_some() || min_changes.is_some()
        )
    }

    /// The text window offset (`offset`, characters).
    pub(crate) fn char_offset(query: &GhGetHistoryItemQuery) -> Option<usize> {
        match query {
            GhGetHistoryItemQuery::PullRequest { offset, .. }
            | GhGetHistoryItemQuery::Issue { offset, .. }
            | GhGetHistoryItemQuery::Commit { offset, .. }
            | GhGetHistoryItemQuery::Compare { offset, .. } => {
                offset.as_ref().map(|offset| usize_of(offset.0))
            }
        }
    }
    /// The text window length (`length`, characters).
    pub(crate) fn char_length(query: &GhGetHistoryItemQuery) -> Option<usize> {
        match query {
            GhGetHistoryItemQuery::PullRequest { length, .. }
            | GhGetHistoryItemQuery::Issue { length, .. }
            | GhGetHistoryItemQuery::Commit { length, .. }
            | GhGetHistoryItemQuery::Compare { length, .. } => {
                length.as_ref().map(|length| usize_of(length.0.get()))
            }
        }
    }
    pub(crate) fn match_string(query: &GhGetHistoryItemQuery) -> Option<&str> {
        match query {
            GhGetHistoryItemQuery::PullRequest { match_string, .. }
            | GhGetHistoryItemQuery::Commit { match_string, .. }
            | GhGetHistoryItemQuery::Compare { match_string, .. } => {
                match_string.as_ref().map(|text| text.0.as_str())
            }
            GhGetHistoryItemQuery::Issue { .. } => None,
        }
    }
    /// Lines kept around each `matchString` hit in a patch (`contextLines`).
    pub(crate) fn match_context(query: &GhGetHistoryItemQuery) -> Option<usize> {
        match query {
            GhGetHistoryItemQuery::PullRequest { context_lines, .. }
            | GhGetHistoryItemQuery::Commit { context_lines, .. }
            | GhGetHistoryItemQuery::Compare { context_lines, .. } => context_lines
                .as_ref()
                .and_then(|lines| usize::try_from(lines.0).ok()),
            GhGetHistoryItemQuery::Issue { .. } => None,
        }
    }
    /// Whether the query reads a later page of a pull request (a section
    /// page after the first, or a text window that names its offset): the
    /// caller already holds the item header and menu from page one.
    pub(crate) fn later_page(query: &GhGetHistoryItemQuery) -> bool {
        matches!(query, GhGetHistoryItemQuery::PullRequest { .. })
            && (page(query).is_some_and(|page| page > 1) || char_offset(query).is_some())
    }
    /// Whether the read's `extras` names `kind`.
    pub(crate) fn wants(
        query: &GhGetHistoryItemQuery,
        kind: octocode_contract_types::ExtraKind,
    ) -> bool {
        match query {
            GhGetHistoryItemQuery::PullRequest { extras, .. }
            | GhGetHistoryItemQuery::Issue { extras, .. }
            | GhGetHistoryItemQuery::Commit { extras, .. }
            | GhGetHistoryItemQuery::Compare { extras, .. } => extras.contains(&kind),
        }
    }
}

/// Read one history item: its row (`data`, sanitized and within the
/// response budget) and the notes of what its pages leave to read.
pub async fn execute(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
    security: &impl ContentScan,
) -> Result<ToolData, ProviderError> {
    let result = execute_inner(transport, query, context).await;
    let mut output = match result {
        Ok(output) => output,
        Err(mut error) => {
            match error.kind {
                // A commit read of a SHA GitHub does not have (the transport
                // types its "No commit found" as a missing ref).
                ProviderErrorKind::NotFound
                    if error.reason == Some(ProviderErrorReason::RefNotFound)
                        && gh_get_history_item_query::operation(query) == ItemOperation::Commit =>
                {
                    error.message = format!(
                        "Commit \"{}\" not found in {}/{}",
                        gh_get_history_item_query::reference(query).unwrap_or_default(),
                        gh_get_history_item_query::owner(query),
                        gh_get_history_item_query::repo(query)
                    )
                    .into_boxed_str();
                }
                // The number names an issue: the message already says so.
                ProviderErrorKind::NotFound
                    if error.reason == Some(ProviderErrorReason::PullRequestIsIssue) => {}
                // GitHub answers a missing item, ref, or repository with a
                // bare 404, and an unresolvable comparison ref with "No commit
                // found for SHA" even for a tag or branch: name what was asked
                // for, as refs.
                ProviderErrorKind::NotFound => {
                    error.message = not_found_message(query, error.reason).into_boxed_str();
                    if matches!(
                        gh_get_history_item_query::operation(query),
                        ItemOperation::Compare
                    ) && error.reason.is_none()
                    {
                        error.reason = Some(ProviderErrorReason::RefNotFound);
                    }
                }
                ProviderErrorKind::RateLimited => {
                    if let Some(rate_limit) = error.rate_limit.as_mut()
                        && rate_limit.remaining.is_none()
                    {
                        rate_limit.remaining = Some(0);
                    }
                }
                _ => {}
            }
            error.message = sanitize_provider_text(security, &error.message, Path::new(SCAN_PATH))?
                .into_boxed_str();
            return Err(error);
        }
    };
    sanitize_all_strings(&mut output.data, security)?;
    remove_nulls(&mut output.data);
    enforce_response_limit(&output.data, context.budget.max_body_bytes)?;
    Ok(output)
}

/// The message of a not-found item read: the repository, number, or refs
/// the query named (a bare GitHub 404 cannot tell which part is missing).
fn not_found_message(query: &HistoryItemRequest, reason: Option<ProviderErrorReason>) -> String {
    let (owner, repo) = (
        gh_get_history_item_query::owner(query),
        gh_get_history_item_query::repo(query),
    );
    if reason == Some(ProviderErrorReason::RepositoryNotFound) {
        return format!(
            "Repository {owner}/{repo} not found, or private and not accessible to this token"
        );
    }
    let or_repository = "or the repository is missing or private to this token";
    match gh_get_history_item_query::operation(query) {
        ItemOperation::PullRequest => format!(
            "Pull request #{} not found in {owner}/{repo}, {or_repository}",
            gh_get_history_item_query::number(query).unwrap_or_default()
        ),
        ItemOperation::Issue => format!(
            "Issue #{} not found in {owner}/{repo}, {or_repository}",
            gh_get_history_item_query::number(query).unwrap_or_default()
        ),
        ItemOperation::Commit => format!(
            "Commit \"{}\" not found in {owner}/{repo}, {or_repository}",
            gh_get_history_item_query::reference(query).unwrap_or_default()
        ),
        ItemOperation::Compare => format!(
            "Cannot compare {}...{} in {owner}/{repo}: a ref (branch, tag, or SHA) was not found, {or_repository}",
            gh_get_history_item_query::base(query).unwrap_or_default(),
            gh_get_history_item_query::head(query).unwrap_or_default()
        ),
    }
}

fn enforce_response_limit(value: &Value, max_body_bytes: usize) -> Result<(), ProviderError> {
    let size = octocode_engine::json_size::bytes(value).unwrap_or(usize::MAX);
    if size > max_body_bytes {
        return Err(ProviderError::new(
            ProviderErrorKind::ResponseTooLarge,
            format!("GitHub history item response exceeds {max_body_bytes} bytes"),
        ));
    }
    Ok(())
}

async fn execute_inner(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    context.check()?;
    match gh_get_history_item_query::operation(query) {
        ItemOperation::PullRequest => {
            match pull_request::pull_request(transport, query, context).await {
                Err(error) if error.status == Some(404) => {
                    Err(issue::name_issue_number(transport, query, context, error).await)
                }
                result => result,
            }
        }
        ItemOperation::Issue => issue::issue(transport, query, context).await,
        ItemOperation::Commit => commit::commit(transport, query, context).await,
        ItemOperation::Compare => compare::compare(transport, query, context).await,
    }
}

fn validation(message: &str) -> ProviderError {
    ProviderError::new(ProviderErrorKind::Validation, message)
}

async fn fetch(
    transport: &GitHubTransport,
    segments: &[&str],
    query: &[(&str, String)],
    context: &RequestContext,
) -> Result<(Value, bool), ProviderError> {
    context.check()?;
    let response = transport.history_item(segments, query, context).await?;
    Ok((response.value, response.has_more))
}

fn sanitize_all_strings(
    value: &mut Value,
    security: &impl ContentScan,
) -> Result<(), ProviderError> {
    crate::security::sanitize_json(value, &mut |text, _| {
        sanitize_provider_text(security, text, Path::new(SCAN_PATH))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use octocode_contract_types::ErrorCode;
    use serde_json::json;

    /// A read with no section named is the summary: it selects none.
    #[test]
    fn a_summary_read_selects_no_section() {
        let query = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest", "owner":"o", "repo":"r", "number":1,
            "mainGoal":"Read the pull request.","reasoning":"No section named."
        }))
        .expect("summary read");
        assert!(query.sections().is_empty());
        assert_eq!(query.sections().wire(), None);
    }

    /// The typed `sections` are what every handler reads; a `matchString`
    /// with none named reads the patches it searches, on every operation
    /// that takes one.
    #[test]
    fn sections_resolve_from_the_typed_query() {
        let row = |fields: Value| {
            HistoryItemRequest::from_row(super::util::merge(
                json!({"operation":"pullRequest","mainGoal":"g","reasoning":"r",
                    "owner":"o","repo":"r","number":1}),
                fields,
            ))
            .expect("pull request row")
        };
        let read = row(json!({
            "sections":["reviews","patches","body","comments","files","reviewComments"],
            "include":["src/**"],"status":["modified"]
        }));
        // Published order on the wire, whatever order the caller named.
        assert_eq!(
            read.sections().wire(),
            Some(json!([
                "body",
                "files",
                "patches",
                "comments",
                "reviewComments",
                "reviews"
            ]))
        );
        assert!(!read.sections().lists_commits());
        assert_eq!(
            row(json!({"matchString":"needle"})).sections(),
            Sections::of([Section::Patches])
        );
        let commit = HistoryItemRequest::from_row(json!({
            "operation":"commit","mainGoal":"g","reasoning":"r","owner":"o","repo":"r",
            "ref":"head-sha","sections":["patches"],"include":["src/*.rs"]
        }))
        .expect("commit");
        assert_eq!(
            gh_get_history_item_query::operation(&commit),
            ItemOperation::Commit
        );
        assert!(
            HistoryItemRequest::from_row(json!({
                "operation":"commit","owner":"o","repo":"r","ref":"h","base":"b"
            }))
            .is_err()
        );
        assert!(commit.include_diff());
        assert_eq!(
            gh_get_history_item_query::include_patterns(&commit),
            ["src/*.rs"]
        );
        let grep = HistoryItemRequest::from_row(json!({
            "operation":"compare","owner":"o","repo":"r","base":"a","head":"b","matchString":"x"
        }))
        .expect("compare grep");
        assert!(grep.include_diff());
        let issue = HistoryItemRequest::from_row(json!({
            "operation":"issue","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","number":2,
            "sections":["comments"]
        }))
        .expect("issue include");
        assert_eq!(issue.sections(), Sections::of([Section::Comments]));
    }

    struct ReplacingScan;
    impl ContentScan for ReplacingScan {
        fn sanitize(
            &self,
            text: &str,
            _path: &Path,
        ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
            Ok((text.replace("secret", "[MASKED]"), Vec::new()))
        }
    }

    #[test]
    fn missing_identity_is_rejected() {
        // The wire contract requires each operation's identity.
        for row in [
            json!({"operation":"commit","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b"}),
            json!({"operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b"}),
            json!({"operation":"compare","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","base":"x"}),
        ] {
            assert!(HistoryItemRequest::from_row(row).is_err());
        }
        assert!(
            HistoryItemRequest::from_row(
                json!({"operation":"commit","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","ref":"x"}),
            )
            .is_ok()
        );
        // The wire contract rejects a blank comparison ref.
        assert!(
            HistoryItemRequest::from_row(json!({
                "operation":"compare","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","base":"","head":"x"
            }))
            .is_err()
        );
    }

    #[test]
    fn sanitizes_every_nested_returned_string() {
        let mut value = json!({
            "title": "secret",
            "nested": [{"body": "a secret value"}],
            "next": {"tool": "secret-tool", "query": {"path": "secret.rs"}}
        });
        sanitize_all_strings(&mut value, &ReplacingScan)
            .expect("GitHub history test data should be valid");
        assert_eq!(value["title"], "[MASKED]");
        assert_eq!(value["nested"][0]["body"], "a [MASKED] value");
        // The executable `tool` identifier survives verbatim, but the query
        // leaves are scanned: redaction fires only on a real
        // secret, so a legitimate path is untouched while a secret is masked.
        assert_eq!(value["next"]["tool"], "secret-tool");
        assert_eq!(value["next"]["query"]["path"], "[MASKED].rs");
    }

    #[test]
    fn normalized_response_budget_is_enforced() {
        let error = enforce_response_limit(&json!({"body": "abcdefgh"}), 4)
            .expect_err("GitHub history operation should fail in this test");
        assert_eq!(error.kind, ProviderErrorKind::ResponseTooLarge);
        assert_eq!(
            error.message.as_ref(),
            "GitHub history item response exceeds 4 bytes"
        );
    }
}
