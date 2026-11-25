//! The package root above a snapshot built below it, and the widen-scope
//! lead every dependency answer carries when that scope left links unread.
use super::*;
use crate::ast_graph::build_scope::BuildScope;

/// A package root above a snapshot: its path relative to the workspace, the
/// manifest that marks it, and the graphBuild lead that builds it.
pub(super) struct PackageRoot {
    pub(super) shown: String,
    pub(super) manifest: &'static str,
    pub(super) build: Value,
}

impl PackageRoot {
    /// The root as a sentence names it: its path, or the workspace root.
    pub(super) fn named(&self) -> String {
        if self.shown == "." {
            "the workspace root".to_owned()
        } else {
            self.shown.clone()
        }
    }
}

/// The nearest directory above `root`, inside its workspace, holding a
/// package manifest while `root` holds none.
pub(super) fn package_root(
    root: &Path,
    workspace: Option<&str>,
    scan: &Value,
) -> Option<PackageRoot> {
    let workspace = Path::new(workspace?);
    let (dir, manifest) = root
        .ancestors()
        .skip(1)
        .take_while(|dir| dir.starts_with(workspace))
        .find_map(|dir| {
            let rel = root.strip_prefix(dir).ok()?;
            let crosses_hidden = rel.components().any(|part| {
                part.as_os_str()
                    .to_str()
                    .is_some_and(|name| name.starts_with('.') && name != "." && name != "..")
            });
            if crosses_hidden {
                return None;
            }
            super::super::workspace::MANIFEST_FILES
                .iter()
                .find(|file| dir.join(file).is_file())
                .map(|file| (dir, *file))
        })?;
    let shown = dir
        .strip_prefix(workspace)
        .ok()
        .map(|relative| relative.to_string_lossy().into_owned())
        .filter(|relative| !relative.is_empty())
        .unwrap_or_else(|| ".".into());
    // The wider build keeps the snapshot's scope: its walk, `exclude` and
    // `parserGlobs`, rebased onto the directory above.
    let mut row = json!({"path": shown});
    BuildScope::of_manifest(scan)
        .above(root.strip_prefix(dir).ok()?)
        .write_row(&mut row);
    Some(PackageRoot {
        shown,
        manifest,
        build: octocode_runtime_base::response::result::Continuation::related(
            octocode_runtime_base::id::ToolId::GraphBuild,
            row,
        )
        // Rows the snapshot cannot hold are read only by building the root.
        .required()
        .build(),
    })
}

/// How many imports of `manifest`'s diagnostics name code beyond the scan or
/// link nothing: the links a sub-scope may be missing.
fn unread_links(diagnostics: &Value) -> u64 {
    ["outside-root", "unresolved-internal"]
        .iter()
        .map(|code| diagnostics["byCode"][code].as_u64().unwrap_or(0))
        .sum()
}

/// The warning and the graphBuild lead of a snapshot built below its package
/// root that holds imports it could not link: `None` when it was built at
/// the root, or links everything.
pub(in crate::ast_graph::store) fn widening(manifest: &Value) -> Option<(String, Value)> {
    let unread = unread_links(&manifest["diagnostics"]);
    let root = PathBuf::from(manifest["root"].as_str().unwrap_or_default());
    let holds_manifest = super::super::workspace::MANIFEST_FILES
        .iter()
        .any(|file| root.join(file).is_file());
    if unread == 0 || holds_manifest {
        return None;
    }
    let above = package_root(&root, manifest["workspace"].as_str(), &manifest["scan"])?;
    let warning = format!(
        "The snapshot lies below its package root, {} (it holds {}), and {unread} imports name code outside the scan or link nothing, so rows may be missing; the graphBuild in related builds that root.",
        above.named(),
        above.manifest
    );
    Some((warning, above.build))
}

impl Query<'_> {
    /// A dependency answer over a snapshot below its package root that holds
    /// imports it could not link: a warning, and the graphBuild of the
    /// package root the row needs ([`package_root`] marks it).
    pub(super) fn widen_scope(&self, value: &mut Value) {
        let Some((warning, build)) = widening(&self.g.manifest) else {
            return;
        };
        octocode_runtime_base::response::paging::push_warnings(value, [warning]);
        match value.get_mut("related").and_then(Value::as_array_mut) {
            Some(leads) => leads.insert(0, build),
            None => value["related"] = Value::Array(vec![build]),
        }
    }
}
