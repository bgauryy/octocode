//! The complete ordered rows of a full-population answer (`issues`,
//! `impact`, `drift`), kept for the continuation pages of that answer.
//!
//! These operations compute their whole row set before a page is cut from
//! it. The first page computes it once; when it issues a continuation it
//! retains the set, and a later page slices it instead of computing it
//! again. Nothing else reads the store: a fresh request (no `page`, or the
//! first) always computes, so a retained set never answers a new question.
//!
//! A set is bound to the exact question: the immutable published snapshot
//! (its id and graph digest) and every query field that shapes the rows,
//! never the window (`offset`, `limit`, `page`). It is sealed as the output
//! gate leaves it, so no credential reaches the store, and it lives as long
//! as any continuation retention ([`CONTINUATION_RETENTION`]).
//!
//! A set that is not retained (too large for the store, expired, evicted,
//! or a memory-mode runtime that ended) is not an error: the page computes
//! the rows again, as it always did, and retains them for the pages after it.
use super::*;
use crate::cache::{CONTINUATION_RETENTION, CacheConfig, CacheKey, Store};
use crate::security::ContentSecurity;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Rows {
    rows: Vec<Value>,
}

/// The retained row sets of one runtime.
pub struct SavedRows {
    store: Store<Rows>,
    policy: String,
}

impl std::fmt::Debug for SavedRows {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("SavedRows").finish_non_exhaustive()
    }
}

/// Rows a page computed: retained once the page states its continuation.
pub(super) type Pending = std::cell::RefCell<Option<Vec<Value>>>;

impl SavedRows {
    /// `disk: None` keeps the sets in this process only.
    pub(crate) fn new(config: CacheConfig, disk: Option<PathBuf>, policy: String) -> Self {
        Self {
            store: Store::new(config, disk),
            policy,
        }
    }

    pub(crate) fn clear_memory(&self) {
        self.store.clear_memory();
    }

    /// The entry of the question `options` asks of snapshot `g`: its id and
    /// graph digest and every field that shapes the rows, never the window.
    fn key(&self, g: &Graph, options: &QueryOptions) -> CacheKey {
        let mut row = query_row(options, &g.id);
        for window in ["offset", "limit", "page", "list"] {
            row.remove(window);
        }
        CacheKey {
            namespace: "graph-rows-v1".into(),
            scope: self.policy.clone(),
            resource: crate::digest::json_sha256(&json!([
                g.id,
                g.manifest["graph"]["sha256"],
                Value::Object(row)
            ])),
        }
    }
}

impl Query<'_> {
    /// The answer's page of a full row set: the retained set when this is a
    /// continuation page that finds one (`compute` does not run), else the
    /// set `compute` builds with the facts the first page states
    /// (`extra`), kept in `pending` to be retained with the continuation.
    pub(super) fn complete_rows(
        &self,
        compute: impl FnOnce() -> Result<(Map<String, Value>, Vec<Value>), GraphOutput>,
    ) -> Result<(Map<String, Value>, Page), GraphOutput> {
        // Only a page after the first reads the store.
        if self.options.page.unwrap_or(1) > 1
            && let Some(saved) = &self.options.rows
            && let Some(hit) = saved.store.get(&saved.key(self.g, self.options))
        {
            return Ok((Map::new(), self.slice(&hit.value.rows)));
        }
        let (extra, rows) = compute()?;
        let page = self.slice(&rows);
        *self.pending.borrow_mut() = Some(rows);
        Ok((extra, page))
    }

    /// The rows of `rows` this request's window asks, with the set's total.
    fn slice(&self, rows: &[Value]) -> Page {
        let window = self.window();
        let start = window.start.min(rows.len());
        let end = window.end.min(rows.len());
        Page {
            rows: rows[start..end].to_vec(),
            total: rows.len(),
            start,
        }
    }

    /// Retain the rows this page computed when it states a continuation.
    pub(super) fn retain_pending(&self, output: &GraphOutput) {
        let Some(saved) = &self.options.rows else {
            return;
        };
        let Some(rows) = self.pending.borrow_mut().take() else {
            return;
        };
        if !output
            .next
            .iter()
            .any(|(channel, _)| matches!(channel, Channel::NextPage))
        {
            return;
        }
        let sealed = ContentSecurity::new()
            .seal_output(json!({"rows": rows}), false, || {
                Ok::<_, std::convert::Infallible>(())
            })
            .unwrap_or_else(|never| match never {})
            .into_value();
        let Ok(sealed) = serde_json::from_value::<Rows>(sealed) else {
            return;
        };
        let bytes = octocode_engine::json_size::bytes(&sealed).unwrap_or(usize::MAX);
        saved.store.put_confirmed(
            saved.key(self.g, self.options),
            sealed,
            bytes,
            CONTINUATION_RETENTION,
        );
    }
}
