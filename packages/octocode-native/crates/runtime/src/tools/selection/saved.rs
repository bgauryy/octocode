//! The pages a candidate-page cursor judged, kept so replaying that cursor
//! reads them instead of asking the provider again: a cursor is not consumed
//! by a read, and a judgment bills every time it is made.
//!
//! The opening token names its page: it binds the query, the source identity,
//! the judge and the pool, so the same token over the same source is the same
//! judgment. Only a page the provider answered for good is kept; one that
//! left candidates to a retry judges again, as its receipt promises.
use super::plan::Plan;
use crate::cache::{CONTINUATION_RETENTION, CacheConfig, CacheKey, Store};
use crate::tools::judgment::stats::ClassificationUsage;
use serde::{Deserialize, Serialize};

/// A judged page and what its receipt said of the judging.
#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Page {
    pub(super) plan: Plan,
    pub(super) usage: ClassificationUsage,
}

pub(crate) struct SavedPlans {
    store: Store<Page>,
}

impl Default for SavedPlans {
    fn default() -> Self {
        Self {
            store: Store::new(CacheConfig::derived(), None),
        }
    }
}

impl SavedPlans {
    /// A page's name: the token that opened it and the admission budget its
    /// judgment was cut by.
    fn key(opening: &str, max_chars: usize) -> CacheKey {
        CacheKey {
            namespace: "selection-judgments-v1".into(),
            scope: max_chars.to_string(),
            resource: octocode_engine::digest::sha256(opening.as_bytes()),
        }
    }

    pub(super) fn get(&self, opening: &str, max_chars: usize) -> Option<Page> {
        self.store
            .get(&Self::key(opening, max_chars))
            .map(|saved| saved.value.as_ref().clone())
    }

    pub(super) fn put(
        &self,
        opening: &str,
        max_chars: usize,
        plan: &Plan,
        usage: &ClassificationUsage,
        context: &crate::execution::ExecutionContext,
    ) {
        let page = Page {
            plan: plan.clone(),
            usage: usage.replayed(),
        };
        let weight = octocode_engine::json_size::bytes(&page).unwrap_or(usize::MAX);
        self.store.put_until(
            crate::execution::cancel::deadline_of(context),
            Self::key(opening, max_chars),
            page,
            weight,
            CONTINUATION_RETENTION,
        );
    }

    pub(crate) fn clear_memory(&self) {
        self.store.clear_memory();
    }
}
