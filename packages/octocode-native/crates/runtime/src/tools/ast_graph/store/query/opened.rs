//! Snapshots a long-lived process keeps open across queries. Decoding a
//! snapshot's tables dominates a small query (every node and evidence
//! record is read and authenticated), so an MCP server checks a decoded
//! snapshot out for one query and back in after it. A key names the exact
//! published bytes: the snapshot directory, its manifest digest and the
//! `graph.bin` size and modification time, so a publish (a new snapshot or
//! rewritten bytes) never reuses another snapshot's tables.
use super::super::format::{GraphTables, SnapshotReader};
use super::*;
use std::sync::{Mutex, PoisonError};
use std::time::SystemTime;

/// Opened snapshots kept per process. Each holds decoded tables (tens of
/// MB for a large scope); a few cover the scopes one investigation pins.
const KEPT: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Key {
    dir: PathBuf,
    digest: String,
    bytes: u64,
    modified: Option<SystemTime>,
    /// Which tables were decoded (evidence, point or topology).
    tables: String,
}

impl Key {
    pub(super) fn of(dir: &Path, digest: &str, tables: String) -> Option<Self> {
        let metadata = std::fs::metadata(dir.join(GRAPH_FILE)).ok()?;
        Some(Self {
            dir: dir.to_path_buf(),
            digest: digest.to_owned(),
            bytes: metadata.len(),
            modified: metadata.modified().ok(),
            tables,
        })
    }

    fn current(&self) -> bool {
        Self::of(&self.dir, &self.digest, self.tables.clone()).as_ref() == Some(self)
    }
}

struct Opened {
    key: Key,
    tables: GraphTables,
    reader: SnapshotReader,
}

static OPENED: Mutex<Vec<Opened>> = Mutex::new(Vec::new());

/// The decoded snapshot `key` names, removed from the process cache while a
/// query holds it; entries whose bytes changed or vanished are dropped.
pub(super) fn check_out(key: &Key) -> Option<(GraphTables, SnapshotReader)> {
    let mut opened = OPENED.lock().unwrap_or_else(PoisonError::into_inner);
    opened.retain(|entry| entry.key.current());
    let index = opened.iter().position(|entry| &entry.key == key)?;
    let entry = opened.remove(index);
    Some((entry.tables, entry.reader))
}

/// Returns a snapshot after a query that completed; the newest stays first.
pub(super) fn check_in(key: Key, tables: GraphTables, reader: SnapshotReader) {
    let mut opened = OPENED.lock().unwrap_or_else(PoisonError::into_inner);
    opened.retain(|entry| entry.key != key);
    opened.insert(
        0,
        Opened {
            key,
            tables,
            reader,
        },
    );
    opened.truncate(KEPT);
}

impl Graph {
    /// Gives a reusable snapshot back to the process cache.
    pub(super) fn release(&mut self) {
        let Some(key) = self.reuse.take() else {
            return;
        };
        if let Some(reader) = self.selective.take() {
            check_in(key, std::mem::take(&mut self.t), reader.into_inner());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::cancel::NeverCancel;
    use crate::security::ContentSecurity;
    use crate::tools::ast_graph::store::{IngestOptions, ingest};
    use crate::tools::test_support::{Fixture, workspace_policy};

    fn cached(dir: &Path) -> Vec<Key> {
        let opened = OPENED.lock().unwrap_or_else(PoisonError::into_inner);
        opened
            .iter()
            .filter(|entry| entry.key.dir.starts_with(dir))
            .map(|entry| entry.key.clone())
            .collect()
    }

    #[test]
    fn a_reused_snapshot_answers_like_a_fresh_one_until_its_bytes_change() {
        let dir = Fixture::new();
        dir.write("app/a.ts", "export function a(){ missing(); gone(); }\n");
        let built = ingest(
            &IngestOptions {
                path: dir.0.join("app"),
                workspace: Some(dir.0.to_path_buf()),
                ..Default::default()
            },
            &workspace_policy(&dir.0),
            &ContentSecurity::new(),
            &NeverCancel,
            None,
        );
        assert_eq!(built.class, ExitClass::Success, "{}", built.value);
        let dir_path = std::fs::canonicalize(&dir.0).unwrap();
        let ask = |reuse_snapshot| {
            query(
                &QueryOptions {
                    op: "unresolved".into(),
                    workspace: Some(dir.0.clone()),
                    reuse_snapshot,
                    ..Default::default()
                },
                &workspace_policy(&dir.0),
            )
        };
        let fresh = ask(false);
        assert!(
            cached(&dir_path).is_empty(),
            "a one-shot query keeps nothing"
        );
        let first = ask(true);
        let [key] = cached(&dir_path).try_into().expect("one opened snapshot");
        let second = ask(true);
        assert_eq!(
            cached(&dir_path),
            std::slice::from_ref(&key),
            "reused, not reopened"
        );
        for page in [&first, &second] {
            assert_eq!(page.value["rows"], fresh.value["rows"]);
            assert_eq!(page.value["pagination"]["totalItems"], fresh.value["pagination"]["totalItems"]);
        }
        let graph = key.dir.join(GRAPH_FILE);
        let file = std::fs::File::options().write(true).open(&graph).unwrap();
        file.set_modified(SystemTime::now() + std::time::Duration::from_secs(5))
            .unwrap();
        assert!(!key.current(), "rewritten bytes change the key");
        assert!(check_out(&key).is_none());
        assert!(cached(&dir_path).is_empty(), "a stale snapshot is dropped");
    }
}
