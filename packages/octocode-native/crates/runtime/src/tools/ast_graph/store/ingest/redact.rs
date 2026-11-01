//! Secret redaction of a projected snapshot before it reaches the disk.
use super::*;

#[cfg(test)]
thread_local! {
    /// Redaction passes this test thread ran (ingest tests read it).
    pub(in crate::tools::ast_graph::store) static PASSES: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

/// Bytes of joined strings one secret scan reads. The scanner takes bounded
/// text, and most snapshots hold no secret, so a clean batch is cleared in
/// one scan.
const SECRET_SCAN_BATCH: usize = 1 << 20;

/// Redacts every string of `tables` the secret scanner flags (a token used as
/// an import specifier is a package name; a diagnostic can quote source), so
/// no secret of the source reaches the disk, then re-sorts the lookups the
/// new strings reorder. Digest strings are hashes the staleness check
/// compares, never source text, so they are kept as they are.
pub(in crate::tools::ast_graph::store) fn redact_secrets(
    tables: &mut format::GraphTables,
    security: &ContentSecurity,
) {
    #[cfg(test)]
    PASSES.with(|passes| passes.set(passes.get() + 1));
    for source in &mut tables.sources.descriptors.snapshots {
        source.path = security.sanitize_text(&source.path, None).content;
    }
    let digests = tables
        .digests
        .iter()
        .map(|(_, string)| *string as usize)
        .collect::<std::collections::HashSet<_>>();
    let strings = &tables.strings;
    let mut batches = Vec::new();
    let mut start = 0;
    while start < strings.len() {
        let mut end = start;
        let mut size = 0;
        while end < strings.len() && (end == start || size < SECRET_SCAN_BATCH) {
            size += strings[end].len() + 1;
            end += 1;
        }
        batches.push(start..end);
        start = end;
    }
    // Batches are independent: scan them on every core.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let redacted = std::thread::scope(|scope| {
        let workers = (0..crate::tools::workers::workers(batches.len(), None))
            .map(|_| {
                scope.spawn(|| {
                    let mut redacted = Vec::new();
                    while let Some(batch) =
                        batches.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
                    {
                        let scanned = batch.clone().filter(|index| !digests.contains(index));
                        let text = scanned
                            .clone()
                            .map(|index| strings[index].as_str())
                            .collect::<Vec<_>>()
                            .join("\n");
                        if security.sanitize_text(&text, None).has_secrets {
                            redacted.extend(scanned.filter_map(|index| {
                                let result = security.sanitize_text(&strings[index], None);
                                result.has_secrets.then_some((index, result.content))
                            }));
                        }
                    }
                    redacted
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .flat_map(|worker| {
                // A failed scan must not publish unredacted strings.
                worker
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect::<Vec<_>>()
    });
    if redacted.is_empty() {
        return;
    }
    for (index, content) in redacted {
        tables.strings[index] = content;
    }
    tables.index();
}
