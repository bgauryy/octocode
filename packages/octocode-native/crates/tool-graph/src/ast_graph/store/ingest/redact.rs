//! Secret redaction of a projected snapshot before it reaches the disk.
use super::*;

#[cfg(test)]
thread_local! {
    /// Redaction passes this test thread ran (ingest tests read it).
    pub(in crate::ast_graph::store) static PASSES: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
    /// Strings this test thread's redaction passes handed to the scan.
    pub(in crate::ast_graph::store) static SCANNED: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

/// How an injected sanitizer fault behaves (tests only).
#[cfg(test)]
#[derive(Clone, Copy)]
pub(in crate::ast_graph::store) enum Fault {
    /// The sanitizer's own guard recovers and replaces the text.
    Recovered,
    /// The scan unwinds.
    Unwind,
}

#[cfg(test)]
static FAULTS: std::sync::Mutex<Vec<(String, Fault)>> = std::sync::Mutex::new(Vec::new());

/// Makes the sanitizer fail on every text containing `marker`.
#[cfg(test)]
pub(in crate::ast_graph::store) fn inject_redact_fault(marker: &str, fault: Fault) {
    FAULTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((marker.to_owned(), fault));
}

/// The stored-leaf sanitizer: `Some` with the text to store instead.
fn sanitize(security: &ContentSecurity, text: &str) -> Option<String> {
    #[cfg(test)]
    {
        let faults = FAULTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        match faults
            .iter()
            .find(|(marker, _)| text.contains(marker.as_str()))
        {
            Some((_, Fault::Recovered)) => {
                // As the sanitizer's guard: the panic is recovered and the
                // text replaced.
                let recovered = std::panic::catch_unwind(|| {
                    assert!(text.is_empty(), "injected sanitizer panic");
                });
                assert!(recovered.is_err());
                return Some(octocode_engine::security::SANITIZER_FAILURE_PLACEHOLDER.to_owned());
            }
            Some((_, Fault::Unwind)) => assert!(text.is_empty(), "injected redact panic"),
            None => {}
        }
    }
    security.sanitize_stored_leaf(text, None)
}

use format::NodeKind;
use octocode_engine::contain::Phase::SecretScan;

/// A redaction whose scan unwound: where, and the files holding the string
/// whose scan unwound alone (empty when no single string did).
#[derive(Debug)]
pub(in crate::ast_graph::store) struct Unscanned {
    pub(in crate::ast_graph::store) panic: octocode_engine::contain::Contained,
    pub(in crate::ast_graph::store) files: Vec<String>,
}

/// The files whose records hold string `index`, root-relative, sorted: a
/// file's own key, and the strings of its symbols, declarations,
/// diagnostics and call sites.
fn owners(tables: &format::GraphTables, index: usize) -> Vec<String> {
    let Ok(string) = u32::try_from(index) else {
        return Vec::new();
    };
    let file_of = |node: u32| tables.nodes.get(node as usize).map(|node| node.file);
    let mut files = std::collections::BTreeSet::new();
    for (id, node) in tables.nodes.iter().enumerate() {
        if [node.key, node.name, node.detail].contains(&string) {
            match node.kind {
                NodeKind::File => files.extend(u32::try_from(id).ok()),
                _ => files.extend([node.file]),
            }
        }
    }
    for diagnostic in &tables.diagnostics {
        if [diagnostic.code, diagnostic.message].contains(&string) {
            files.insert(diagnostic.file);
        }
    }
    for declaration in &tables.declarations {
        if [
            declaration.test_identity,
            declaration.occurrence,
            declaration.assessment,
        ]
        .contains(&string)
        {
            files.extend(file_of(declaration.node));
        }
    }
    for witness in &tables.witnesses {
        if [witness.argument, witness.occurrence].contains(&string) {
            files.extend(file_of(witness.edge.src));
        }
    }
    for site in &tables.unresolved {
        if [site.argument, site.occurrence, site.callee, site.reason].contains(&string) {
            files.extend(file_of(site.src));
        }
    }
    let mut named = files
        .into_iter()
        .filter_map(|file| tables.nodes.get(file as usize))
        .filter(|node| node.kind == NodeKind::File)
        .filter_map(|node| tables.strings.get(node.key as usize).map(str::to_owned))
        .collect::<Vec<_>>();
    named.sort_unstable();
    named.dedup();
    named
}

/// Bytes of joined strings one secret scan reads. The scanner takes bounded
/// text, and most snapshots hold no secret, so a clean batch is cleared in
/// one scan.
const SECRET_SCAN_BATCH: usize = 1 << 20;

/// The string dictionaries of the snapshot at `dir` when the same build
/// inputs built it (`identity`, the known-format list among them): each of
/// their strings is what this build's secret scan stores for that text, so
/// the scan finds nothing in it again. `None` when another identity built it
/// or it no longer reads whole.
pub(super) fn scanned_strings(
    dir: &Path,
    identity: &str,
    cancel: &dyn CancellationCheck,
) -> Option<[format::Strings; 2]> {
    let manifest = read_manifest(dir)?;
    if manifest["buildIdentity"] != identity {
        return None;
    }
    let digest = manifest["graph"]["sha256"].as_str()?;
    format::SnapshotReader::open(&dir.join(GRAPH_FILE), digest, cancel)
        .and_then(|mut reader| reader.dictionaries(cancel))
        .ok()
}

/// Whether the sorted dictionary `sorted` holds `text`.
fn holds(sorted: &format::Strings, text: &str) -> bool {
    let (mut low, mut high) = (0, sorted.len());
    while low < high {
        let middle = low + (high - low) / 2;
        match sorted.get(middle).map(|stored| stored.cmp(text)) {
            Some(std::cmp::Ordering::Equal) => return true,
            Some(std::cmp::Ordering::Less) => low = middle + 1,
            _ => high = middle,
        }
    }
    false
}

/// Redacts every string of `tables` the secret scanner flags (a token used as
/// an import specifier is a package name; a diagnostic can quote source), so
/// no secret of the source reaches the disk, then re-sorts the lookups the
/// new strings reorder. Digest strings are hashes the staleness check
/// compares, never source text, so they are kept as they are, and a string
/// `scanned` holds ([`scanned_strings`]) is one the scan stored as it is:
/// a rebuild after an edit scans only what the edit brought. `Err` when a
/// scan unwound: the tables may hold unscanned strings and must not be
/// published.
pub(in crate::ast_graph::store) fn redact_secrets(
    tables: &mut format::GraphTables,
    security: &ContentSecurity,
    scanned: Option<&[format::Strings]>,
) -> Result<(), Unscanned> {
    #[cfg(test)]
    PASSES.with(|passes| passes.set(passes.get() + 1));
    for source in &mut tables.sources.descriptors.snapshots {
        if let std::borrow::Cow::Owned(path) = security.sanitize_text(&source.path, None).content {
            source.path = path;
        }
    }
    let digests = tables
        .digests
        .iter()
        .map(|(_, string)| *string as usize)
        .collect::<std::collections::HashSet<_>>();
    let strings = &tables.strings;
    // What the scan reads: every string but the digests and the ones a
    // snapshot of the same inputs stores as the scan left them.
    let unread = (0..strings.len())
        .filter(|index| {
            !digests.contains(index)
                && !scanned.is_some_and(|stored| {
                    stored
                        .iter()
                        .any(|dictionary| holds(dictionary, &strings[*index]))
                })
        })
        .collect::<Vec<_>>();
    #[cfg(test)]
    SCANNED.with(|count| count.set(count.get() + unread.len()));
    let mut batches = Vec::new();
    let mut start = 0;
    while start < unread.len() {
        let mut end = start;
        let mut size = 0;
        while end < unread.len() && (end == start || size < SECRET_SCAN_BATCH) {
            size += strings[unread[end]].len() + 1;
            end += 1;
        }
        batches.push(start..end);
        start = end;
    }
    // Batches are independent: scan them on every core. A panic the
    // sanitizer's own guard recovered already replaced its string alone; a
    // batch whose scan unwound fails the redaction, so nothing unscanned
    // reaches the disk, after a rescan of its strings one by one finds the
    // first that unwinds alone, to name its files.
    let workers = octocode_tools_shared::workers::workers(batches.len(), None);
    let redacted = octocode_tools_shared::workers::each_parallel(&mut batches, workers, |batch| {
        let scanned = unread[batch.clone()].iter().copied();
        let scan = || -> Vec<(usize, String)> {
            let text = scanned
                .clone()
                .map(|index| &strings[index])
                .collect::<Vec<_>>()
                .join("\n");
            if sanitize(security, &text).is_none() {
                return Vec::new();
            }
            scanned
                .clone()
                .filter_map(|index| sanitize(security, &strings[index]).map(|clean| (index, clean)))
                .collect()
        };
        octocode_engine::contain::catch(SecretScan, scan).map_err(|panic| {
            let culprit = scanned.clone().find(|&index| {
                octocode_engine::contain::catch(SecretScan, || sanitize(security, &strings[index]))
                    .is_err()
            });
            (panic, culprit)
        })
    })
    .into_iter()
    .collect::<Result<Vec<_>, _>>()
    .map_err(|(panic, culprit)| Unscanned {
        panic,
        files: culprit
            .map(|index| owners(tables, index))
            .unwrap_or_default(),
    })?
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if redacted.is_empty() {
        return Ok(());
    }
    let replaced = redacted
        .iter()
        .filter_map(|(index, _)| u32::try_from(*index).ok())
        .collect::<std::collections::HashSet<_>>();
    let touched = |ids: &[u32]| ids.iter().any(|id| replaced.contains(id));
    tables.redacted.unresolved = tables
        .unresolved
        .iter()
        .enumerate()
        .filter(|(_, site)| touched(&[site.argument, site.occurrence, site.callee, site.reason]))
        .filter_map(|(row, _)| u32::try_from(row).ok())
        .collect();
    tables.redacted.witnesses = tables
        .witnesses
        .iter()
        .enumerate()
        .filter(|(_, witness)| {
            touched(&[witness.edge.detail, witness.occurrence, witness.argument])
        })
        .filter_map(|(row, _)| u32::try_from(row).ok())
        .collect();
    tables.strings.replace(redacted);
    tables.index();
    Ok(())
}
