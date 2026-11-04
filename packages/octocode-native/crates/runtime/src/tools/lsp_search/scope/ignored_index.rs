//! The ignored family files of a scope, for the importer scan. A walk's result
//! is kept while a real check proves it would list the same files (the walked
//! directories, nested repositories and ignore inputs, like [`super::listing`]);
//! their text is searched as it is now ([`super::spelling_index`]). The walk belongs to the requesting call:
//! cancellation ends it, and an incomplete one is not kept.
use super::spelling_index::WordSearch;
use super::*;
use crate::tools::lsp_search::words::files_spelling;
use crate::tools::page_memo::{PageMemo, Proven, Watch, repository_markers};
use octocode_engine::stamp::Link;
use snapshot::build;
mod snapshot;

#[derive(Debug, Default)]
pub(super) struct IgnoredIndex {
    pub(super) files: Vec<String>,
    pub(super) nested: Vec<String>,
    pub(super) complete: bool,
    /// What proves the walks' result later; taken when the index is kept.
    pub(super) watched: snapshot::Watched,
}

impl IgnoredIndex {
    /// The listed files that spell `symbol` as a word, and whether every
    /// listed file was checked (only `stopped`, the request's end, cuts it).
    /// `clock` is the request's settle clock.
    pub(super) fn mentions(
        &self,
        symbol: &str,
        clock: SystemTime,
        stopped: &(dyn Fn() -> bool + Sync),
    ) -> (BTreeSet<String>, bool) {
        let search = WordSearch::new(symbol, None, clock);
        // An unreadable file is not proven free of the name.
        let (found, read_all) = files_spelling(&self.files, stopped, |file| {
            search.file_spells(file).unwrap_or(true)
        });
        (found.into_iter().cloned().collect(), read_all)
    }
}

/// Kept indexes, by root and family globs, against the one process budget.
static INDEXES: PageMemo<Proven<IgnoredIndex>> = PageMemo::new();

fn current(key: &str, policy: &PathPolicy) -> Option<Arc<IgnoredIndex>> {
    INDEXES.current(key, &policy.identity())
}

/// `index` as served, kept when its walks finished and what they depend on was
/// not written within the settle window before `started`.
fn keep(
    key: String,
    root: &Path,
    policy: &PathPolicy,
    mut index: IgnoredIndex,
    started: SystemTime,
) -> Arc<IgnoredIndex> {
    let watched = std::mem::take(&mut index.watched);
    let complete = index.complete;
    let index = Arc::new(index);
    if !complete {
        return Arc::clone(&index);
    }
    let Some((sources, mut rules)) = watched.inputs(root, policy) else {
        return index;
    };
    rules.extend(octocode_engine::search::walk::external_ignore_inputs(root));
    let proof = Watch {
        root,
        sources: &sources,
        rules: &rules,
        markers: &repository_markers(root),
    }
    .capture(Link::Follow, Some(started));
    if let Some(proof) = proof {
        let bytes = index
            .files
            .iter()
            .chain(&index.nested)
            .map(String::len)
            .chain(sources.iter().map(|path| path.as_os_str().len()))
            .map(|len| len + std::mem::size_of::<PathBuf>())
            .sum();
        INDEXES.put_proven(key, policy.identity(), bytes, proof, Arc::clone(&index));
    }
    index
}

impl Scope {
    pub(super) async fn ignored_index(
        &self,
        policy: &PathPolicy,
        cancel: &dyn CancellationCheck,
    ) -> Result<Option<Arc<IgnoredIndex>>, LspFailure> {
        let base = canonical(&self.root);
        let key = format!("{base}\u{0}{}", self.include.join("\u{0}"));
        let (kept_key, kept_policy) = (key.clone(), policy.clone());
        let kept = blocking_cancellable(cancel, move |_| current(&kept_key, &kept_policy))
            .await?
            .flatten();
        if kept.is_some() {
            return Ok(kept);
        }
        #[cfg(test)]
        if let Some(work) = probe::current() {
            work.ignored_walks
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let root = PathBuf::from(base);
        let include = self.include.clone();
        let policy = policy.clone();
        // Before the walks' first read, on the request's own clock.
        let started = octocode_engine::stamp::settle_clock();
        blocking_cancellable(cancel, move |stopped| {
            keep(
                key,
                &root,
                &policy,
                build(&root, &include, &policy, stopped),
                started,
            )
        })
        .await
    }
}

#[cfg(test)]
mod tests;
