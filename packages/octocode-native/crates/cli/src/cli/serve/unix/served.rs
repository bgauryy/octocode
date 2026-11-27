//! A runtime the CLI server keeps under its configuration identity.

/// A runtime a server keeps for one working directory and environment,
/// with the identity it was built under. Provider errors belong to calls.
pub(super) struct Served {
    pub(super) runtime: octocode_native::runtime::ToolRuntime,
    pub(super) identity: String,
}
