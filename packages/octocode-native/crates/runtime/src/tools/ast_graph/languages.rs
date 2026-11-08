//! The language id every graph count and query uses, below both the linker
//! ([`super::graph`]) and the snapshot store ([`super::store`]).

/// One label per language regardless of which parser produced the facts
/// (oxc reports the extension, tree-sitter a language id): the selector of
/// the file's grammar in the engine registry
/// ([`octocode_engine::signatures::languages::label`]), except that the TSX
/// grammar keeps its name (`tsx`) and a `.h` header read as C++ or CUDA
/// keeps that dialect; `raw` for an extension no grammar claims.
pub(crate) fn canonical_language(file: &str, raw: &str) -> String {
    use octocode_engine::signatures::languages;
    let extension = octocode_engine::text::extension_of(file, true, "");
    match languages::find_entry(&extension) {
        Some(entry) if entry.name == "TSX" => "tsx".to_owned(),
        Some(entry) if entry.name == "C" && matches!(raw, "cpp" | "cuda") => raw.to_owned(),
        Some(entry) => languages::label(entry),
        None => raw.to_owned(),
    }
}
