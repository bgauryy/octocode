//! clangd's compilation-database lookup for one server configuration, the one
//! model every consumer uses: the coverage flag (`lspSearch`'s
//! `noCompileDatabase`) and the reload watch ([`super::project_config`]).
//!
//! clangd (llvmorg-21.1.0) searches, in order: the `compilationDatabasePath`
//! initialization option (`ClangdLSPServer.cpp`), else the
//! `--compile-commands-dir` flag when its value is non-empty and exists, as a
//! file or directory (`ClangdMain.cpp`;
//! relative to clangd's working directory, which Octocode sets to the
//! workspace root), else every ancestor of the file. In each searched
//! directory it reads `compile_commands.json`, `build/compile_commands.json`
//! and `compile_flags.txt` (`GlobalCompilationDatabase.cpp`).
//!
//! clangd writes its background index beside the database it reads, with no
//! setting to move it. So when a project has a `compile_commands.json`, Octocode
//! points clangd at a copy under its own home ([`mirror_database`]): the index
//! goes there, the repository stays untouched, and clangd knows every file.
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

/// Names the project database a mirror copies, in the server's environment: the
/// pool key and every reader of [`database_search`] see the project's file.
const MIRRORED_DATABASE: &str = "CLANGD_MIRRORED_DATABASE";

/// The `compile_commands.json` clangd would read for `config`, if any. A flags
/// file lists no files, so no background index can come of it.
fn project_database(config: &LspLanguageServerConfig) -> Option<PathBuf> {
    let directory = match database_search(config) {
        DatabaseSearch::Fixed(directory) => directory,
        DatabaseSearch::Ancestors => PathBuf::from(&config.workspace_root),
    };
    ["compile_commands.json", "build/compile_commands.json"]
        .iter()
        .map(|name| directory.join(name))
        .find(|path| path.is_file())
}

fn is_background_index_flag(arg: &str) -> bool {
    arg.strip_prefix("--")
        .or_else(|| arg.strip_prefix('-'))
        .is_some_and(|flag| flag.split('=').next() == Some("background-index"))
}

/// A file's state when a server started: what its index holds. A tool that keeps
/// modification times (`cp -p`, `rsync -t`) still changes this.
pub type FileState = Option<(std::time::SystemTime, u64)>;

pub fn file_state(file: &str) -> FileState {
    let meta = std::fs::metadata(file).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// The files the database behind `config` lists and their state now, with each
/// entry's real `directory` and its `file` spelled as written: a file reached
/// through a link below the project is the index's only under that spelling, so
/// its real path does not read as covered.
pub fn listed_files(
    config: &LspLanguageServerConfig,
) -> std::collections::HashMap<String, FileState> {
    let database = mirror_directory(config)
        .map(|mirror| mirror.join("compile_commands.json"))
        .or_else(|| project_database(config));
    let entries = database
        .and_then(|database| std::fs::read(database).ok())
        .and_then(|bytes| serde_json::from_slice::<Vec<serde_json::Value>>(&bytes).ok())
        .unwrap_or_default();
    let mut real = std::collections::HashMap::<String, PathBuf>::new();
    entries
        .iter()
        .filter_map(|entry| {
            let directory = entry["directory"].as_str()?;
            let directory = real.entry(directory.to_owned()).or_insert_with(|| {
                Path::new(directory)
                    .canonicalize()
                    .unwrap_or_else(|_| PathBuf::from(directory))
            });
            let file = crate::text::lexical(&directory.join(entry["file"].as_str()?))
                .to_string_lossy()
                .into_owned();
            let state = file_state(&file);
            Some((file, state))
        })
        .collect()
}

/// The folder of the mirror `config` reads, when it reads one.
fn mirror_directory(config: &LspLanguageServerConfig) -> Option<PathBuf> {
    compile_commands_dir(config.args.as_deref().unwrap_or_default())
        .filter(|_| {
            config
                .env
                .as_ref()
                .is_some_and(|env| env.contains_key(MIRRORED_DATABASE))
        })
        .map(PathBuf::from)
}

/// The folder clangd keeps its index shards in for `config`.
fn index_directory(config: &LspLanguageServerConfig) -> Option<PathBuf> {
    mirror_directory(config)
        .or_else(|| {
            project_database(config).and_then(|database| database.parent().map(Path::to_path_buf))
        })
        .map(|directory| directory.join(".cache/clangd/index"))
}

/// The file names clangd wrote an index shard for (`lib.c.<hash>.idx`). A listed
/// file with none did not index: its flags failed.
pub fn indexed_names(config: &LspLanguageServerConfig) -> std::collections::HashSet<String> {
    index_directory(config)
        .and_then(|directory| std::fs::read_dir(directory).ok())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|shard| {
            let shard = shard.file_name().to_string_lossy().into_owned();
            let (name, _) = shard.strip_suffix(".idx")?.rsplit_once('.')?;
            Some(name.to_owned())
        })
        .collect()
}

/// Whether clangd has written any index shard for `config`. A database that lists
/// nothing, one whose entries were all left out, and `.clangd` with
/// `Index: Background: Skip` write none, and leave a server with no index however
/// quiet it is.
pub fn has_index_shards(config: &LspLanguageServerConfig) -> bool {
    index_directory(config).is_some_and(|directory| {
        std::fs::read_dir(directory).is_ok_and(|mut shards| shards.next().is_some())
    })
}

/// Whether clangd under `config` holds a background index of a project database:
/// mirrored here, or turned on by the user.
pub fn indexes_project(config: &LspLanguageServerConfig) -> bool {
    let args = config.args.as_deref().unwrap_or_default();
    args.iter().any(|arg| {
        is_background_index_flag(arg) && !arg.ends_with("=false") && !arg.ends_with("=0")
    }) && project_database(config).is_some()
}

/// Whether clangd reads a compile command for a file in `directory`.
pub fn reads_compile_command(config: &LspLanguageServerConfig, directory: &Path) -> bool {
    !clangd_config_sources(directory, &database_search(config)).is_empty()
}

/// Whether clangd reads a `compile_commands.json` for a file in `directory`, the
/// only configuration that lists files, so the only one an index can come of.
pub fn reads_compile_database(config: &LspLanguageServerConfig, directory: &Path) -> bool {
    clangd_config_sources(directory, &database_search(config))
        .iter()
        .any(|source| source.ends_with("compile_commands.json"))
}

/// Whether clangd would read a compile command or project config from `dir`.
pub fn is_project_config_dir(dir: &Path) -> bool {
    dir.join(PROJECT_CONFIG).is_file() || DATABASE_FILES.iter().any(|name| dir.join(name).is_file())
}

/// Where clangd looks for a compilation database.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatabaseSearch {
    /// Only this path (the init option, or an existing flag value).
    Fixed(PathBuf),
    /// The file's directory and every ancestor (clangd's default).
    Ancestors,
}

/// clangd's lookup for `config`: the init option, then the flag (if its value
/// is non-empty and exists), else ancestors.
pub fn database_search(config: &LspLanguageServerConfig) -> DatabaseSearch {
    // A mirrored project is described by the database it copies.
    if let Some(parent) = config
        .env
        .as_ref()
        .and_then(|env| env.get(MIRRORED_DATABASE))
        .and_then(|database| Path::new(database).parent())
    {
        return DatabaseSearch::Fixed(parent.to_path_buf());
    }
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
        .filter(|dir| dir.exists())
        .map_or(DatabaseSearch::Ancestors, DatabaseSearch::Fixed)
}

/// What is wrong with the database `config` names: clangd ignores a flag whose
/// directory does not exist, and reads only the named directory when it does.
/// `None` when nothing is named or what is named holds a database.
pub fn configured_database_problem(config: &LspLanguageServerConfig) -> Option<String> {
    let flag = compile_commands_dir(config.args.as_deref().unwrap_or_default());
    match database_search(config) {
        DatabaseSearch::Fixed(dir) => DATABASE_FILES
            .iter()
            .all(|name| !dir.join(name).is_file())
            .then(|| {
                format!(
                    "clangd is configured to read its compile commands only from {}, which holds no compile_commands.json or compile_flags.txt",
                    dir.display()
                )
            }),
        DatabaseSearch::Ancestors => flag.map(|dir| {
            format!(
                "clangd ignores --compile-commands-dir={dir}: no such directory under {}",
                config.workspace_root
            )
        }),
    }
}

/// The last `--compile-commands-dir` value in LLVM `cl` spellings: one or two
/// dashes, with `=DIR` or DIR as the next argument. An empty value means the
/// flag is unset (`ClangdMain.cpp` tests `!CompileCommandsDir.empty()`).
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
    found.filter(|dir| !dir.is_empty())
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

mod mirror;
pub(crate) use mirror::{mirror_database, prepare_mirror};

#[cfg(test)]
mod tests;
