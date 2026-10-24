//! The page walk of the shared checks (`checks.rs` re-exports it): follow
//! every page lead of a tool from a start call, checking each page.

use super::{assert_leads_valid, assert_output_contract, canonical, leads};
use serde_json::Value;
use std::collections::BTreeSet;

/// Pages a walk follows before it fails as unbounded.
const WALK_CAP: usize = 200;

/// One executed call of a walk: its public output and the ids of the rows it
/// delivered.
pub struct Page {
    pub output: Value,
    pub rows: Vec<String>,
}

/// What a walk saw: every row id (each exactly once) and every call it made,
/// in order.
#[derive(Debug, Default)]
pub struct Walk {
    pub rows: Vec<String>,
    pub queries: Vec<Value>,
}

/// A page walk of `tool` from `start`: each call's output satisfies the output
/// contract and offers only contract-valid leads; every page lead (`next.<k>`,
/// `k` a contract page channel, naming `tool`) is followed breadth-first,
/// verbatim. A branch's last page may re-offer the one call that continues
/// past it (it is not rerun). Fails on any other repeated call, a row
/// delivered twice, or more than
/// [`WALK_CAP`] calls. Drive it with [`walk`], or call [`Walker::next_query`]
/// and [`Walker::accept`] from an async test.
pub struct Walker {
    tool: String,
    queue: std::collections::VecDeque<Value>,
    seen_queries: BTreeSet<String>,
    seen_rows: BTreeSet<String>,
    walk: Walk,
}

impl Walker {
    pub fn new(tool: &str, start: Value) -> Self {
        Self {
            tool: tool.to_owned(),
            queue: std::collections::VecDeque::from([start]),
            seen_queries: BTreeSet::new(),
            seen_rows: BTreeSet::new(),
            walk: Walk::default(),
        }
    }

    /// The next call to make, or `None` when every page lead has been followed.
    pub fn next_query(&mut self) -> Option<Value> {
        let query = self.queue.pop_front()?;
        assert!(
            self.seen_queries.insert(canonical(&query)),
            "{} walk repeats a call: {query}",
            self.tool
        );
        assert!(
            self.walk.queries.len() < WALK_CAP,
            "{} walk still paging after {WALK_CAP} calls",
            self.tool
        );
        self.walk.queries.push(query.clone());
        Some(query)
    }

    /// Check the page the last [`Walker::next_query`] call produced and queue
    /// its page leads.
    pub fn accept(&mut self, page: Page) {
        assert_output_contract(&self.tool, &page.output);
        assert_leads_valid(&page.output);
        for row in page.rows {
            assert!(
                self.seen_rows.insert(row.clone()),
                "{} walk delivers `{row}` twice (call {})",
                self.tool,
                self.walk.queries.len()
            );
            self.walk.rows.push(row);
        }
        let page_leads = leads(&page.output)
            .into_iter()
            .filter(|lead| lead.kind == "continue" && lead.tool == self.tool)
            .collect::<Vec<_>>();
        // A branch's last page may re-offer the one call that continues past
        // it (a localSearch match page that ends its file group re-offers
        // `nextPage`), so a linear reader never dead-ends; the walk does not
        // rerun it. Any other repeated call still fails in `next_query`.
        let branch_end = page_leads.len() == 1;
        for lead in page_leads {
            let key = canonical(&lead.query);
            if branch_end
                && (self.seen_queries.contains(&key)
                    || self.queue.iter().any(|queued| canonical(queued) == key))
            {
                continue;
            }
            self.queue.push_back(lead.query);
        }
    }

    pub fn finish(self) -> Walk {
        self.walk
    }
}

/// [`Walker`] driven by a synchronous `exec`.
pub fn walk(tool: &str, start: Value, mut exec: impl FnMut(&Value) -> Page) -> Walk {
    let mut walker = Walker::new(tool, start);
    while let Some(query) = walker.next_query() {
        walker.accept(exec(&query));
    }
    walker.finish()
}
