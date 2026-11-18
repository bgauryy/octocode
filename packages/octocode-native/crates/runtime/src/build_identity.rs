//! The one identity every saved cursor, snapshot and replay plan binds to:
//! a digest of this build's runtime sources (`build.rs`). A cursor minted by
//! another build fails its snapshot check and restarts, so no hand-bumped
//! version can be forgotten when a row shape or page cut changes.

/// Differs between builds of different sources.
pub(crate) const BUILD_IDENTITY: &str = env!("OCTOCODE_BUILD_IDENTITY");

/// Every root `build.rs` hashed into [`BUILD_IDENTITY`], relative to the
/// workspace, separated by `;` (cargo env values hold no newline).
#[cfg(test)]
const HASHED_ROOTS: &str = env!("OCTOCODE_BUILD_IDENTITY_ROOTS");

/// The input list `build.rs` shares with the contract embedding.
#[cfg(test)]
#[path = "../build/inputs.rs"]
mod inputs;

#[cfg(test)]
mod tests;
