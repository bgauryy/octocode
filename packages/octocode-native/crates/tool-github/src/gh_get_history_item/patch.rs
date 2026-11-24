//! Patch views and pages: `hunks` builds one file's `matchString` view and
//! numbered diff; `page` packs a file page into the shared patch char window.
//! The window sizes are in `window`.
pub(super) mod hunks;
pub(super) mod page;
#[cfg(test)]
mod tests;
