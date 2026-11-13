use super::{
    EntryKind, MaterializeSkips, Snapshot, TreeEntry, materialize_tree,
    snapshot::{FileIdentity, Store, write_file},
};
use octocode_github::{ProviderErrorKind, RequestContext};
use octocode_io::budget::RequestBudget;
use std::{path::Path, time::Duration};

fn snapshot(home: &Path) -> Snapshot<'_> {
    Snapshot {
        root: home,
        owner: "octo",
        repo: "demo",
        commit: "abc123",
    }
}

fn context() -> RequestContext {
    RequestContext::new(
        RequestBudget::with_timeout(Duration::from_secs(5), 1024),
        None,
    )
}

fn entry(path: &str, bytes: &[u8]) -> TreeEntry {
    TreeEntry {
        path: path.into(),
        kind: EntryKind::File,
        size: Some(bytes.len() as u64),
    }
}

mod resume;
mod snapshots;
