//! Parsed `Cargo.toml` files, kept while unchanged.
use super::CARGO_MANIFEST;
use crate::stamp::Proof;
use octocode_io::memo::Memo;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A parsed `Cargo.toml` and the stamp of the file it came from.
struct KeptManifest {
    inputs: Proof,
    table: Option<Arc<toml::Table>>,
}

/// Manifests kept this process, by path, against the one process budget, each
/// until [`crate::memory::IDLE`] after its last read.
static MANIFESTS: Memo<PathBuf, KeptManifest> = Memo::new(&crate::memory::MEMORY);

/// The parsed `Cargo.toml` of `directory`; `None` when it is missing or does not
/// parse. Every reader of a manifest (root discovery, includes, a request's
/// reach) reads it here, parsed once while the file is unchanged; a manifest
/// written within the settle window is parsed again on its next read.
pub fn cargo_manifest(directory: &Path) -> Option<Arc<toml::Table>> {
    let path = directory.join(CARGO_MANIFEST);
    if let Some(kept) = MANIFESTS
        .get(&path)
        .filter(|kept| kept.value.inputs.holds())
    {
        MANIFESTS.touch(&path, crate::memory::IDLE);
        return kept.value.table.clone();
    }
    let started = crate::stamp::settle_clock();
    let mut inputs = Proof::default();
    inputs.follow(path.clone());
    let text = std::fs::read_to_string(&path).ok();
    let weight = inputs.weight() + text.as_ref().map_or(0, String::len);
    let table = text.and_then(|text| text.parse().ok()).map(Arc::new);
    if inputs.settled_before(started) {
        MANIFESTS.insert(
            path,
            KeptManifest {
                inputs,
                table: table.clone(),
            },
            weight,
            Some(crate::memory::IDLE),
        );
    }
    table
}
