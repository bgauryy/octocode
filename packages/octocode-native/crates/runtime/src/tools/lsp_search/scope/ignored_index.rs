//! Request-owned ignored-file discovery. Cancellation belongs to the requesting call;
//! no background walk or workspace result survives it.
use super::*;
use crate::tools::lsp_search::words::file_spells;
use snapshot::build;
mod snapshot;

#[derive(Debug, Default)]
pub(super) struct IgnoredIndex {
    pub(super) files: Vec<String>,
    pub(super) nested: Vec<String>,
    pub(super) complete: bool,
}

impl IgnoredIndex {
    /// The listed files that spell `symbol` as a word, and whether every
    /// listed file was read (only `stopped`, the request's end, cuts it).
    pub(super) fn mentions(
        &self,
        symbol: &str,
        stopped: &(dyn Fn() -> bool + Sync),
    ) -> (BTreeSet<String>, bool) {
        let mut found = BTreeSet::new();
        for file in &self.files {
            if stopped() {
                return (found, false);
            }
            // An unreadable file is not proven free of the name.
            if file_spells(Path::new(file), symbol.as_bytes()).unwrap_or(true) {
                found.insert(file.clone());
            }
        }
        (found, true)
    }
}

impl Scope {
    pub(super) async fn ignored_index(
        &self,
        policy: &PathPolicy,
        cancel: &dyn CancellationCheck,
    ) -> Result<Option<Arc<IgnoredIndex>>, LspFailure> {
        let root = PathBuf::from(&self.root);
        let include = self.include.clone();
        let policy = policy.clone();
        blocking_cancellable(cancel, move |stopped| {
            Arc::new(build(&root, &include, &policy, stopped))
        })
        .await
    }
}

#[cfg(test)]
mod tests;
