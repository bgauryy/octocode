//! How a project's own settings place an import: slash-path arithmetic
//! ([`paths`]), JavaScript module probing and build-output mapping
//! ([`js_paths`]), the configuration context read beside the sources
//! ([`aliases`]: tsconfig `paths` and `baseUrl`, workspace packages, Python
//! roots, C include directories), and which of them a file's grammar links
//! by ([`Linker`]). The code graph links imports by it, and lspSearch places
//! an importer candidate's bare specifiers by it.
pub mod aliases;
pub mod js_paths;
pub mod linker;
pub mod paths;

pub use linker::Linker;
