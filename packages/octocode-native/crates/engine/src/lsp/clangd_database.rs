//! clangd's compilation-database lookup for one server configuration, the one
//! model every consumer uses: the coverage flag (`lspSearch`'s
//! `noCompileDatabase`) and the reload watch ([`super::project_config`]).
//!
//! clangd (llvmorg-21.1.0) searches, in order: the `compilationDatabasePath`
//! initialization option (`ClangdLSPServer.cpp`), else the
//! `--compile-commands-dir` flag when its directory exists (`ClangdMain.cpp`;
//! relative to clangd's working directory, which Octocode sets to the
//! workspace root), else every ancestor of the file. In each searched
//! directory it reads `compile_commands.json`, `build/compile_commands.json`
//! and `compile_flags.txt` (`GlobalCompilationDatabase.cpp`).
//!
//! Not modeled: a `CompileFlags.CompilationDatabase` value in `.clangd` or the
//! user's `config.yaml` (no YAML parser is in the build). A `.clangd` file
//! still counts as configuration found, so it raises no false flag. File
//! presence never proves the compile command clangd used.
use crate::lsp::types::LspLanguageServerConfig;
use std::path::{Path, PathBuf};

/// The database files clangd reads in each directory it searches.
const DATABASE_FILES: &[&str] = &[
    "compile_commands.json",
    "build/compile_commands.json",
    "compile_flags.txt",
];

/// Project config clangd reads from the file's directory and every ancestor,
/// whatever the database search.
const PROJECT_CONFIG: &str = ".clangd";

/// Where clangd looks for a compilation database.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatabaseSearch {
    /// Only this directory (the init option, or an existing flag directory).
    Fixed(PathBuf),
    /// The file's directory and every ancestor (clangd's default).
    Ancestors,
}

/// clangd's lookup for `config`: the init option, then the flag (if its
/// directory exists), else ancestors.
pub fn database_search(config: &LspLanguageServerConfig) -> DatabaseSearch {
    let root = Path::new(&config.workspace_root);
    let resolve = |dir: &str| {
        let path = Path::new(dir);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        }
    };
    if let Some(dir) = config
        .initialization_options
        .as_ref()
        .and_then(|options| options.get("compilationDatabasePath"))
        .and_then(serde_json::Value::as_str)
        .filter(|dir| !dir.is_empty())
    {
        return DatabaseSearch::Fixed(resolve(dir));
    }
    compile_commands_dir(config.args.as_deref().unwrap_or_default())
        .map(|dir| resolve(&dir))
        .filter(|dir| dir.is_dir())
        .map_or(DatabaseSearch::Ancestors, DatabaseSearch::Fixed)
}

/// The last `--compile-commands-dir` value in LLVM `cl` spellings: one or two
/// dashes, with `=DIR` or DIR as the next argument.
fn compile_commands_dir(args: &[String]) -> Option<String> {
    let mut found = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let Some(flag) = arg
            .strip_prefix("--")
            .or_else(|| arg.strip_prefix('-'))
            .and_then(|rest| rest.strip_prefix("compile-commands-dir"))
        else {
            continue;
        };
        if let Some(value) = flag.strip_prefix('=') {
            found = Some(value.to_owned());
        } else if flag.is_empty() {
            found = args.next().cloned().or(found);
        }
    }
    found
}

/// The clangd configuration files that apply to a file in `dir` under
/// `search`, nearest first: the database files clangd reads, plus every
/// ancestor `.clangd` (it supplies flags whatever the database search).
pub fn clangd_config_sources(dir: &Path, search: &DatabaseSearch) -> Vec<PathBuf> {
    let databases: Vec<PathBuf> = match search {
        DatabaseSearch::Fixed(fixed) => {
            DATABASE_FILES.iter().map(|name| fixed.join(name)).collect()
        }
        DatabaseSearch::Ancestors => dir
            .ancestors()
            .flat_map(|dir| DATABASE_FILES.iter().map(move |name| dir.join(name)))
            .collect(),
    };
    databases
        .into_iter()
        .chain(dir.ancestors().map(|dir| dir.join(PROJECT_CONFIG)))
        .filter(|path| path.is_file())
        .collect()
}

/// Absolute files outside the per-directory names that clangd reads for
/// `config`: a fixed directory's database files, which a reload watch stamps
/// as well as each document's ancestors.
pub fn fixed_database_files(config: &LspLanguageServerConfig) -> Vec<PathBuf> {
    match database_search(config) {
        DatabaseSearch::Fixed(dir) => DATABASE_FILES.iter().map(|name| dir.join(name)).collect(),
        DatabaseSearch::Ancestors => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
