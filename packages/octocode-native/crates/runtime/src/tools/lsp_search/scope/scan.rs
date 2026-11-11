//! The text scan behind the scope.
use super::*;

/// Files that spell a name but that ignore rules or a hidden directory
/// keep out of the lexical scans (see [`Scope::ignored_text_files`]).
#[derive(Debug, Default)]
pub(in super::super) struct IgnoredMentions {
    /// Canonical paths, sorted.
    pub(in super::super) files: Vec<String>,
    /// Distinct deciding rules (`.gitignore:1 'gen/'`) of the first files.
    pub(in super::super) rules: Vec<String>,
    /// The scan failed or stopped at its bound: `files` is what it reached,
    /// and ignored files may mention the name beyond it.
    pub(in super::super) unfinished: bool,
    /// Of `files`, those that import the name: the evidence that ignore
    /// rules hide an importer. A file that only spells the name (a bundled
    /// copy declaring its own) is not one.
    pub(in super::super) importing: Vec<String>,
    /// Of `files`, those the import check could not decide (unreadable,
    /// too large, unparsable, or past its file bound): not importers, and
    /// not ruled out.
    pub(in super::super) unchecked: Vec<String>,
    /// Nested repositories (root-relative) the scan did not enter
    /// ([`octocode_engine::search::walk::NestedRepositories`]): ignored
    /// files in them are not counted.
    pub(in super::super) nested_repositories: Vec<String>,
}

impl IgnoredMentions {
    /// Whether a file outside the candidate scan may import the name: one
    /// does, one could not be checked, or the scan for them did not finish.
    /// Such a scan verified every candidate and is still not complete.
    pub(in super::super) fn may_hide_an_importer(&self) -> bool {
        self.unfinished || !self.importing.is_empty() || !self.unchecked.is_empty()
    }

    /// A scan that produced nothing: no file is known, and that is not
    /// proof of none.
    pub(in super::super) fn unfinished() -> Self {
        Self {
            unfinished: true,
            ..Self::default()
        }
    }
}

/// Word-bounded files-only scan of `root` (family `include`, read policy,
/// ignore rules and the default directory prune honored, an include naming
/// a pruned directory lifting it), observing `cancel`: over the kept listing
/// of the scope while its proof holds ([`listing`]), else by a walk that
/// keeps one.
/// The scan covers the whole scope: only the request stops it (its
/// cancellation or its deadline), and a stopped scan keeps nothing. `None`
/// when the scan failed; else the files that spell `symbol` and every file
/// the scan listed.
pub(super) async fn scan_text(
    root: &str,
    include: &[String],
    symbol: &str,
    policy: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Option<(BTreeSet<String>, Arc<[PathBuf]>)>, LspFailure> {
    if symbol.trim().is_empty() {
        return Ok(None);
    }
    // The walk follows no symlink, so a file it reaches under a canonical
    // root is canonical: the root is resolved once, not each file.
    #[cfg(test)]
    if let Some(work) = super::probe::current() {
        work.canonicalized
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    let base = canonical(root);
    let (kept_base, kept_include, kept_policy) = (base.clone(), include.to_vec(), policy.clone());
    let kept = blocking_cancellable(cancel, move |_| {
        listing::current(&kept_base, &kept_include, &kept_policy)
    })
    .await?
    .flatten();
    if let Some(files) = kept {
        let word = symbol.to_owned();
        let clock = octocode_engine::stamp::settle_clock();
        return Ok(blocking_cancellable(cancel, move |stopped| {
            let spelling = listing::spelling(&files, &word, clock, stopped)?;
            Some((spelling, files))
        })
        .await?
        .flatten());
    }
    let options = TextSearchOptions {
        path: base.clone(),
        pattern: word_pattern(symbol),
        output: octocode_engine::types::SearchOutput::FilesWithMatch,
        include: crate::tools::variant_field::non_empty(include),
        ..TextSearchOptions::default()
    };
    let (walk_policy, walk_include) = (policy.clone(), include.to_vec());
    // Before the walk's first read, on the request's own clock.
    let started = octocode_engine::stamp::settle_clock();
    #[cfg(test)]
    if let Some(work) = super::probe::current() {
        work.text_scans
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    let scanned = blocking_cancellable(cancel, move |stopped| {
        let recorder = Arc::new(listing::Recorder::new(&walk_policy, &base, &walk_include));
        let filter: Arc<dyn octocode_engine::search::text_search::TextSearchPathFilter> =
            recorder.clone();
        let parsed = search_cancellable(options, filter, stopped).ok()?;
        if parsed
            .stats
            .cap_reasons
            .contains(&octocode_engine::types::CapReason::Cancelled)
        {
            return None;
        }
        let listed = recorder.keep(&walk_include, started);
        Some((
            parsed
                .files
                .into_iter()
                .map(|file| {
                    let path = Path::new(&file.path);
                    if path.is_absolute() {
                        file.path
                    } else {
                        Path::new(&base).join(path).to_string_lossy().into_owned()
                    }
                })
                .collect::<BTreeSet<_>>(),
            listed,
        ))
    })
    .await?
    .flatten();
    Ok(scanned)
}
