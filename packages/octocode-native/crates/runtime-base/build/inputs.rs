//! The contract files `@octocodeai/config` generates, which this crate's build
//! embeds ([`embed`]) and renders ([`config_gen`]). The runtime crate's build
//! identity mounts this file to hash the same list, so a file the build reads is
//! never missing from the identity.

use std::path::{Path, PathBuf};

/// Files of the generated tool contract, relative to its `contract/` directory.
pub(super) const CONTRACT_FILES: [&str; 6] = [
    "tool-contract.json",
    "contract-fixtures.json",
    "provenance.json",
    "tool_types.rs",
    "tool_types_base.rs",
    "cli-presentation.json",
];

/// The generated configuration contract, relative to the config package.
pub(super) const CONFIG_CONTRACT: &str = "config-contract.json";

/// A file of the `@octocodeai/config` package.
pub(super) fn config_path(manifest_dir: &Path, name: &str) -> PathBuf {
    manifest_dir.join(format!("../../../../packages/octocode-config/{name}"))
}
