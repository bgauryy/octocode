//! Where one page of repositories stands: its pagination, partial facts and warnings.
use super::GhSearchRepoQuery;
use super::owner_listing::OwnerListing;
use crate::id::ToolId;
use crate::response::paging::push_warnings;
use crate::tools::gh_shared::{SearchCompleteness, SearchWindow, apply_partial};
use octocode_github::RepositorySearchPage;
use serde_json::{Value, json};

/// Where one page of repositories stands.
pub(super) struct RepoPage {
    pub(super) current: usize,
    pub(super) per: usize,
    pub(super) more: bool,
    /// The page the next one follows: the last provider page an owner
    /// listing read, else the requested page.
    pub(super) next_from: usize,
    /// GitHub's own match count; an owner listing reports none.
    pub(super) matched: usize,
    /// The search total within the 1,000-result window (a last page may
    /// run past 1,000).
    pub(super) reachable: usize,
    pub(super) incomplete: bool,
    /// An owner listing's REST pages read by this call.
    pub(super) provider_pages: Option<usize>,
    pub(super) archived_skipped: usize,
    /// No rows though the search (or the listing's earlier pages) has some:
    /// the shared response stage restarts it.
    pub(super) past_end: bool,
}

impl RepoPage {
    pub(super) fn of(
        data: &RepositorySearchPage,
        listing: Option<&OwnerListing>,
        current: usize,
        per: usize,
    ) -> Self {
        let reach = SearchWindow::PageStart.reach(data.total_count, per);
        let empty = data.items.is_empty() && current > 1;
        match listing {
            Some(listing) => Self {
                current,
                per,
                more: listing.more,
                next_from: listing.last_page,
                matched: 0,
                reachable: reach.total,
                incomplete: data.incomplete_results,
                provider_pages: Some(listing.pages_read),
                archived_skipped: listing.archived,
                past_end: empty && listing.archived == 0,
            },
            None => Self {
                current,
                per,
                more: current < reach.pages,
                next_from: current,
                matched: data.total_count,
                reachable: reach.total,
                incomplete: data.incomplete_results,
                provider_pages: None,
                archived_skipped: 0,
                past_end: empty && reach.past_last(current),
            },
        }
    }

    /// The rows and their `pagination`. The REST listing reports no total:
    /// `page` is the provider page cursor and `next.nextPage` follows the
    /// real Link header. The listing is ordered by latest push (the `owner`
    /// description says so); page counters are verbose (core field class).
    pub(super) fn value(&self, repositories: Vec<Value>) -> Value {
        use crate::response::pages::PageFacts;
        let pagination = match self.provider_pages {
            Some(read) => {
                let mut pagination = PageFacts::open(self.current, Some(self.per), self.more)
                    .out_of_range(self.past_end)
                    .to_value();
                pagination["providerPagesRead"] = json!(read);
                pagination
            }
            None => PageFacts::counted(self.current, self.per, self.reachable)
                .out_of_range(self.past_end)
                .to_value(),
        };
        json!({"repositories": repositories, "pagination": pagination})
    }

    /// The provider's partial facts and the page's warnings: archived
    /// repositories a listing skipped, a language filter GitHub ignored.
    pub(super) fn disclose(
        &self,
        value: &mut Value,
        query: &GhSearchRepoQuery,
        ignored: Option<String>,
    ) {
        apply_partial(
            value,
            query,
            &SearchCompleteness {
                tool: ToolId::GhSearchRepo,
                matched: self.matched,
                reachable: self.reachable,
                incomplete: self.incomplete,
                page: self.current,
                has_more: self.more,
                subject: "repository",
            },
        );
        if self.archived_skipped > 0 {
            push_warnings(
                value,
                [format!(
                    "{} skipped",
                    crate::response::pages::counted(
                        u64::try_from(self.archived_skipped).unwrap_or(u64::MAX),
                        "archived repository",
                        "archived repositories"
                    )
                )],
            );
        }
        push_warnings(value, ignored);
    }
}
