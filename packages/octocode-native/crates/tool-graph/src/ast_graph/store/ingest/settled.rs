//! What a build read, as its snapshot records it, and what changed while it
//! ran.
use super::*;
use crate::ast_graph::freshness;

/// The inputs a snapshot records: the linking configs as the build read
/// them and the files that changed while it ran.
pub(super) struct Settled {
    /// The linking configs the snapshot's directories hold, with the
    /// digests the build read.
    pub inputs: BTreeMap<String, String>,
    /// The stamp proof of `inputs`: `None` when one is refused or anything
    /// the build read changed while it ran.
    pub proof: Option<String>,
    /// How many sources and configs changed, went missing or became
    /// unreadable after the build read them. The snapshot holds what was
    /// read, so its freshness check reports each of them.
    pub changed: usize,
}

/// The snapshot's resolution inputs as the build read them, with their stamp
/// proof when nothing it read has changed since. A file edited while the
/// build ran keeps the digest of what was read: the snapshot publishes, and
/// its freshness check reports that file, as it would an edit made after
/// publication.
pub(super) fn settled_inputs(
    built: &super::super::super::types::BuiltGraph,
    tables: &format::GraphTables,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Settled, GraphOutput> {
    let files = tables
        .nodes
        .iter()
        .filter(|node| node.kind == format::NodeKind::File)
        .map(|node| tables.str(node.key));
    let dirs = freshness::scope_dirs(&built.root, files);
    let read = freshness::inputs_in(&built.resolution_inputs, &built.root, &dirs);
    // Stamped before the configs are hashed again: equal digests then prove
    // what the stamps saw.
    let proof = settled_input_stamps(paths, &built.root, tables, &read);
    let current = freshness::config_inputs(paths, &built.root, &dirs);
    let mut changed = freshness::config_changes(&read, &current).len();
    for (file, facts) in &built.facts {
        if let Err(error) = cancel.check() {
            return Err(GraphOutput::error(ErrorCode::Cancelled, error));
        }
        let unchanged = paths
            .validate_read(built.root.join(file))
            .ok()
            .and_then(|source| {
                source_matches(&source.canonical, &facts.digest, facts.stamp.as_ref())
            });
        if unchanged != Some(true) {
            changed += 1;
        }
    }
    let proof = proof.filter(|_| changed == 0 && !withholds_configs(&built.root, tables, &read));
    Ok(Settled {
        inputs: read,
        proof,
        changed,
    })
}
