//! Where one page of repositories stands: its pagination, partial facts and warnings.
use super::owner_listing::OwnerListing;
use crate::id::ToolId;
use crate::response::paging::push_warnings;
use crate::tools::gh_shared::{
    SearchCompleteness, SearchWindow, apply_partial, push_warning_front,
};
use octocode_contract_types::GhSearchRepoQuery;
use octocode_engine::num::saturating_u64;
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
    /// Organizations GitHub withheld from the answer (SAML enforcement).
    sso_partial: Vec<u64>,
    /// An owner listing: REST pages that state no total.
    pub(super) listing: bool,
    /// Rows an owner listing's archive filter left out ([`OwnerListing`]).
    pub(super) archive_filtered: usize,
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
                sso_partial: data.sso_partial_organizations.clone(),
                listing: true,
                archive_filtered: listing.filtered_out,
                past_end: empty && listing.filtered_out == 0,
            },
            None => Self {
                current,
                per,
                more: current < reach.pages,
                next_from: current,
                matched: data.total_count,
                reachable: reach.total,
                incomplete: data.incomplete_results,
                sso_partial: data.sso_partial_organizations.clone(),
                listing: false,
                archive_filtered: 0,
                past_end: empty && reach.past_last(current),
            },
        }
    }

    /// The rows and their `pagination`: the page starts at its provider
    /// page's first row and holds the rows shown. The REST listing reports no
    /// total and `next.nextPage` follows the real Link header; the listing is
    /// ordered by latest push (the `owner` description says so). A search
    /// states its reachable total.
    pub(super) fn value(&self, repositories: Vec<Value>) -> Value {
        use crate::response::pages::PageFacts;
        let start = PageFacts::start_of(self.current, self.per);
        let pagination = if self.listing {
            PageFacts::open(start, repositories.len(), self.more)
                .out_of_range(self.past_end)
                .to_value()
        } else {
            PageFacts::open(start.min(self.reachable), repositories.len(), self.more)
                .with_total(self.reachable)
                .out_of_range(self.past_end)
                .to_value()
        };
        json!({"repositories": repositories, "pagination": pagination})
    }

    /// The provider's partial facts and the page's warnings: the rows a
    /// listing's archive filter left out (a filter, never a gap), a language
    /// filter GitHub ignored.
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
        state_cap_once(value);
        if self.archive_filtered > 0 {
            use octocode_contract_types::GitHubArchive;
            let archive = super::access::archive(query);
            let (mode, one, many) = match archive {
                GitHubArchive::Only => (
                    "only",
                    "non-archived repository",
                    "non-archived repositories",
                ),
                GitHubArchive::Exclude => {
                    ("exclude", "archived repository", "archived repositories")
                }
                // Nothing is filtered when archived rows are included.
                GitHubArchive::Include => ("include", "repository", "repositories"),
            };
            push_warnings(
                value,
                [format!(
                    "filters.archive \"{mode}\" left out {} (a filter, not a gap)",
                    crate::response::pages::counted(
                        saturating_u64(self.archive_filtered),
                        one,
                        many
                    )
                )],
            );
        }
        push_warnings(value, ignored);
        if let Some(warning) = octocode_github::sso_partial_warning(&self.sso_partial) {
            push_warning_front(value, warning);
        }
    }
}

/// The provider's result cap, stated once. [`apply_partial`] writes it as a
/// warning (the only statement carrying the full match count) and a
/// `partialReasons` entry; the warning stays, beside the `isPartial` and
/// `countScope: "capped"` facts. The reason name would only restate it.
fn state_cap_once(value: &mut Value) {
    const CAP: &str = "providerResultCap";
    let Some(object) = value.as_object_mut() else {
        return;
    };
    let emptied = object
        .get_mut("partialReasons")
        .and_then(Value::as_array_mut)
        .is_some_and(|reasons| {
            reasons.retain(|reason| reason != CAP);
            reasons.is_empty()
        });
    if emptied {
        object.remove("partialReasons");
    }
}
