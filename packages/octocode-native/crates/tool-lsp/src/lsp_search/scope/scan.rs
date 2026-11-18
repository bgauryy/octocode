//! The text scan behind the scope.
use super::*;

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
        include: octocode_tools_shared::variant_field::non_empty(include),
        global_ignore: policy.global_ignore(),
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
