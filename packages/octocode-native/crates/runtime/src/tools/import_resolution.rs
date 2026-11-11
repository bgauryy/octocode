//! How a project's own settings place an import: slash-path arithmetic
//! ([`paths`]), JavaScript module probing and build-output mapping
//! ([`js_paths`]), the configuration context read beside the sources
//! ([`aliases`]: tsconfig `paths` and `baseUrl`, workspace packages, Python
//! roots, C include directories), and which of them a file's grammar links
//! by ([`Linker`]). The code graph links imports by it, and lspSearch places
//! an importer candidate's bare specifiers by it.
pub(crate) mod aliases;
pub(crate) mod js_paths;
mod linker;
pub(crate) mod paths;

pub(crate) use linker::Linker;
