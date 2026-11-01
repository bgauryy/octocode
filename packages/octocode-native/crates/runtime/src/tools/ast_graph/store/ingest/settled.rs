//! The inputs a build read must be unchanged when it publishes.
use super::*;

/// The snapshot's resolution inputs, refused when a config or source the
/// build read changed while it ran.
pub(super) fn settled_inputs(
    built: &super::super::super::types::BuiltGraph,
    tables: &format::GraphTables,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<BTreeMap<String, String>, GraphOutput> {
    let inputs = snapshot_inputs(paths, &built.root, tables);
    if inputs != built.resolution_inputs {
        return Err(GraphOutput::error(
            ErrorCode::ExecutionFailed,
            "project configuration changed during graph analysis; rebuild from settled inputs",
        ));
    }
    for (file, facts) in &built.facts {
        if let Err(error) = cancel.check() {
            return Err(GraphOutput::error(ErrorCode::Cancelled, error));
        }
        let source = match paths.validate_read(built.root.join(file)) {
            Ok(source) => source.canonical,
            Err(_) => {
                return Err(GraphOutput::error(
                    ErrorCode::ExecutionFailed,
                    "source access changed during graph analysis; rebuild from accessible inputs",
                ));
            }
        };
        if source_matches(&source, &facts.digest, facts.stamp.as_ref()) != Some(true) {
            return Err(GraphOutput::error(
                ErrorCode::ExecutionFailed,
                format!(
                    "source {file:?} changed during graph analysis; rebuild from settled inputs"
                ),
            ));
        }
    }
    Ok(inputs)
}
