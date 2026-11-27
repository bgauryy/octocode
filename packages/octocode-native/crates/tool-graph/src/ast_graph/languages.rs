//! The language id every graph count and query uses, below both the linker
//! ([`super::graph`]) and the snapshot store ([`super::store`]).

/// One label per language regardless of which parser produced the facts
/// (oxc reports the extension, tree-sitter a language id): the selector of
/// the grammar the file is read with in the engine registry
/// ([`octocode_engine::signatures::languages::label`]): its extension's,
/// else, for a file a `parserGlobs` rule maps, the one `raw` names
/// ([`octocode_tools_shared::import_resolution::linker::read_grammar`]).
/// The TSX grammar keeps its name (`tsx`) and a `.h` header read as C++ or
/// CUDA keeps that dialect; `raw` when no grammar is named.
pub(crate) fn canonical_language(file: &str, raw: &str) -> String {
    use octocode_engine::signatures::languages;
    use octocode_tools_shared::import_resolution::linker::read_grammar;
    match read_grammar(file, raw) {
        Some(entry) if entry.name == "TSX" => "tsx".to_owned(),
        Some(entry) if entry.name == "C" && matches!(raw, "cpp" | "cuda") => raw.to_owned(),
        Some(entry) => languages::label(entry),
        None => raw.to_owned(),
    }
}
