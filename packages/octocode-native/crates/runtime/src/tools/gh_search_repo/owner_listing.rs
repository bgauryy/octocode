//! An owner-only query's REST repository listing, by latest push.
use octocode_github::{GitHubProvider, ProviderError, RepositorySearchPage, RequestContext};

/// List an owner's repositories by latest push. The listing API cannot rank
/// by relevance, and its own default order is creation (oldest first).
/// Search excludes archived repositories by default (`archived:false`); the
/// listing API cannot, so unless `keep_archived` it filters them and keeps
/// reading provider pages until a page of kept rows, the end of the listing
/// (no Link next), or the page budget. A page never holds more than `per` rows: a provider page
/// whose kept rows would overflow it is left whole to the next call (the
/// cursor resumes at it), so no kept row is cut or shown twice.
pub(super) async fn owner_listing(
    provider: &GitHubProvider,
    owner: &str,
    keep_archived: bool,
    current: usize,
    per: usize,
    context: &RequestContext,
) -> Result<(RepositorySearchPage, OwnerListing), ProviderError> {
    let mut items = Vec::new();
    let mut archived = 0;
    let mut provider_page = current;
    let mut pages_read = 0;
    let more = loop {
        let (batch, has_next) = provider
            .transport
            .list_owner_repositories(owner, "pushed", provider_page, per, context)
            .await?;
        pages_read += 1;
        let read = batch.len();
        let kept = batch
            .into_iter()
            .filter(|item| keep_archived || !item.archived)
            .collect::<Vec<_>>();
        if !items.is_empty() && items.len() + kept.len() > per {
            provider_page -= 1;
            break true;
        }
        archived += read - kept.len();
        items.extend(kept);
        if !has_next || items.len() >= per || pages_read >= MAX_OWNER_LISTING_PAGES {
            break has_next;
        }
        provider_page += 1;
    };
    Ok((
        RepositorySearchPage {
            total_count: items.len(),
            incomplete_results: false,
            items,
        },
        OwnerListing {
            last_page: provider_page,
            pages_read,
            more,
            archived,
        },
    ))
}

/// Provider pages read by one owner-only listing call before it stops.
const MAX_OWNER_LISTING_PAGES: usize = 5;

pub(super) struct OwnerListing {
    /// Last provider page listed; the next cursor is the page after it.
    pub(super) last_page: usize,
    /// Provider pages this call read (a page left to the next call too).
    pub(super) pages_read: usize,
    /// The last page carried a Link `rel="next"`.
    pub(super) more: bool,
    /// Archived repositories skipped on the provider pages read.
    pub(super) archived: usize,
}
