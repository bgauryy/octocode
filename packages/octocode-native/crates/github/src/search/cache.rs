//! The search response store: slots, single flight, and pages kept only when complete.
use serde::{Deserialize, Serialize};

use super::decode;
use crate::{GitHubTransport, ProviderError, ProviderErrorKind, RequestContext, RequestSpec};

impl GitHubTransport {
    /// A search-API read through the response cache. Search answers come
    /// from GitHub's search index, which already trails writes, so an
    /// identical search (same URL, `Accept`, endpoint and credential) is
    /// served from the cache until the store's volatile TTL ends: no request,
    /// so no search spacing wait and no search rate-limit spend. A page
    /// `complete` rejects (GitHub's `incomplete_results`) is not stored: that
    /// is exactly what a retry must not get back. Single flight: an identical
    /// search already in flight (a sibling row of the batch) is waited for,
    /// and its stored page answers this one, so the duplicate spends no
    /// search quota and no spacing wait. `lookup` says whether the caller
    /// already found the store empty.
    pub(crate) async fn cached_search<T: Serialize + for<'de> Deserialize<'de>>(
        &self,
        namespace: &str,
        spec: RequestSpec,
        context: &RequestContext,
        lookup: Lookup,
        invalid: &'static str,
        complete: impl Fn(&T) -> bool,
    ) -> Result<(T, Vec<u64>), ProviderError> {
        let (partition, key) = self.search_cache_slot(namespace, &spec, context)?;
        let ticket = search_flights().join((partition.clone(), key.clone()));
        let mut stored = context.budget.wait(ticket.turn()).await?;
        if (lookup == Lookup::Store || *stored)
            && let Some(page) = self.stored_page(&partition, &key).await
        {
            return Ok((page, Vec::new()));
        }
        let response = self
            .execute(spec, context)
            .await
            .map_err(unsearchable_repository)?;
        let page: T = decode(response.body.as_ref(), invalid)?;
        let withheld = response.sso_partial_organizations();
        if withheld.is_empty()
            && complete(&page)
            && let Ok(bytes) = serde_json::to_vec(&page)
        {
            self.cache
                .put(
                    &partition,
                    key,
                    crate::CachedContent {
                        etag: None,
                        bytes: bytes.into(),
                        resolved_ref: String::new(),
                        has_more: false,
                    },
                    crate::Retention::Revalidate,
                )
                .await;
            *stored = true;
        }
        Ok((page, withheld))
    }

    /// A stored search page for `spec`, when the store still holds one.
    pub(super) async fn cached_page<T: for<'de> Deserialize<'de>>(
        &self,
        namespace: &str,
        spec: &RequestSpec,
        context: &RequestContext,
    ) -> Result<Option<T>, ProviderError> {
        let (partition, key) = self.search_cache_slot(namespace, spec, context)?;
        Ok(self.stored_page(&partition, &key).await)
    }

    /// The page stored in search slot `(partition, key)`.
    async fn stored_page<T: for<'de> Deserialize<'de>>(
        &self,
        partition: &crate::Partition,
        key: &str,
    ) -> Option<T> {
        self.cache
            .get(partition, key)
            .await
            .and_then(|cached| serde_json::from_slice::<T>(&cached.bytes).ok())
    }

    /// The store slot of a search: the partition carries the endpoint and
    /// credential, the key the URL and `Accept`.
    fn search_cache_slot(
        &self,
        namespace: &str,
        spec: &RequestSpec,
        context: &RequestContext,
    ) -> Result<(crate::Partition, String), ProviderError> {
        let partition = self.cache_partition(context, None)?;
        Ok((partition, crate::cache_key::request_key(namespace, spec)))
    }
}

/// The process's in-flight searches, one per `(credential, search slot)`.
/// Whether a search reads the store before it sends.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Lookup {
    /// Serve a stored page when there is one.
    Store,
    /// The caller just found none: read again only when a search of this
    /// flight stored one meanwhile.
    Missed,
}

/// Search flights; a slot says whether a search of its wave stored a page.
fn search_flights() -> &'static crate::KeyedFlight<(crate::Partition, String), bool> {
    static FLIGHTS: crate::KeyedFlight<(crate::Partition, String), bool> =
        crate::KeyedFlight::transient();
    &FLIGHTS
}

/// A 422 "cannot be searched" names what GitHub could not search: "users and
/// repositories" is the `repo:`/`user:` scope (the repository was not found,
/// not a bad query); "users" alone is a person qualifier (`author:`,
/// `assignee:`, …) whose login does not exist or is not visible.
fn unsearchable_repository(mut error: ProviderError) -> ProviderError {
    if error.status == Some(422) && error.message.contains("cannot be searched") {
        error.kind = ProviderErrorKind::NotFound;
        let reason = if error.message.contains("repositories") {
            crate::ProviderErrorReason::RepositoryNotFound
        } else {
            crate::ProviderErrorReason::UserNotFound
        };
        error = error.with_reason(reason);
    }
    error
}
