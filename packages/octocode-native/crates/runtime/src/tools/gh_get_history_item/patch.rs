//! Patch views and pages: `hunks` builds one file's `matchString` view and
//! numbered diff; `page` packs a file page into the shared patch char window;
//! `locate` finds a patch's hunks in another version of the file.
//! The window sizes are in `window`.
pub(super) mod hunks;
pub(super) mod locate;
pub(super) mod page;
#[cfg(test)]
mod tests;
