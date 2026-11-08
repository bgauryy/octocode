//! Secret redaction of a projected snapshot before it reaches the disk.
use super::*;

#[cfg(test)]
thread_local! {
    /// Redaction passes this test thread ran (ingest tests read it).
    pub(in crate::tools::ast_graph::store) static PASSES: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

/// How an injected sanitizer fault behaves (tests only).
#[cfg(test)]
#[derive(Clone, Copy)]
pub(in crate::tools::ast_graph::store) enum Fault {
    /// The sanitizer's own guard recovers and replaces the text.
    Recovered,
    /// The scan unwinds.
    Unwind,
}

#[cfg(test)]
static FAULTS: std::sync::Mutex<Vec<(String, Fault)>> = std::sync::Mutex::new(Vec::new());

/// Makes the sanitizer fail on every text containing `marker`.
#[cfg(test)]
pub(in crate::tools::ast_graph::store) fn inject_redact_fault(marker: &str, fault: Fault) {
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
pub(in crate::tools::ast_graph::store) struct Unscanned {
    pub(in crate::tools::ast_graph::store) panic: octocode_engine::contain::Contained,
    pub(in crate::tools::ast_graph::store) files: Vec<String>,
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
        .filter_map(|node| tables.strings.get(node.key as usize).cloned())
        .collect::<Vec<_>>();
    named.sort_unstable();
    named.dedup();
    named
}

/// Bytes of joined strings one secret scan reads. The scanner takes bounded
/// text, and most snapshots hold no secret, so a clean batch is cleared in
/// one scan.
const SECRET_SCAN_BATCH: usize = 1 << 20;

/// Redacts every string of `tables` the secret scanner flags (a token used as
/// an import specifier is a package name; a diagnostic can quote source), so
/// no secret of the source reaches the disk, then re-sorts the lookups the
/// new strings reorder. Digest strings are hashes the staleness check
/// compares, never source text, so they are kept as they are. `Err` when a
/// scan unwound: the tables may hold unscanned strings and must not be
/// published.
pub(in crate::tools::ast_graph::store) fn redact_secrets(
    tables: &mut format::GraphTables,
    security: &ContentSecurity,
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
    // Batches are independent: scan them on every core. A panic the
    // sanitizer's own guard recovered already replaced its string alone; a
    // batch whose scan unwound fails the redaction, so nothing unscanned
    // reaches the disk, after a rescan of its strings one by one finds the
    // first that unwinds alone, to name its files.
    let workers = crate::tools::workers::workers(batches.len(), None);
    let redacted = crate::tools::workers::each_parallel(&mut batches, workers, |batch| {
        let scanned = batch.clone().filter(|index| !digests.contains(index));
        let scan = || -> Vec<(usize, String)> {
            let text = scanned
                .clone()
                .map(|index| strings[index].as_str())
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
    for (index, content) in redacted {
        tables.strings[index] = content;
    }
    tables.index();
    Ok(())
}
