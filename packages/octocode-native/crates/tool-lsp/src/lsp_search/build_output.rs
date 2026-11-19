//! TypeScript build output: the declaration files a package's tsconfig emits
//! to its `outDir` (or `declarationDir`), and the sources under its `rootDir`
//! they were compiled from.
//!
//! A workspace package imported by name resolves, through its `exports` or
//! `types`, to that output, so the server binds an importer's name to the
//! emitted declaration: a different declaration from the anchor's source.
//! Without a `declarationMap` the server never maps it back, so this does:
//! `<outDir>/<rel>.d.ts` was compiled from `<rootDir>/<rel>.ts` (or `.tsx`;
//! `.d.mts` and `.d.cts` from `.mts` and `.cts`).
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::security::ContentSecurity;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Config files that govern a TypeScript or JavaScript project.
const CONFIG_NAMES: [&str; 2] = ["tsconfig.json", "jsconfig.json"];
/// Directories walked up from a file to its governing config.
const MAX_ANCESTORS: usize = 16;
/// `extends` links followed from one config.
const MAX_EXTENDS: usize = 4;

/// One package's declaration output and the source directory it mirrors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BuildOutput {
    /// Canonical directory the declarations are emitted to.
    out_dir: PathBuf,
    /// Canonical directory the emitted tree mirrors.
    root_dir: PathBuf,
}

/// The emit options of one config, after its `extends` chain.
#[derive(Default)]
struct EmitOptions {
    out_dir: Option<PathBuf>,
    root_dir: Option<PathBuf>,
    /// The literal leading directories of `include` and `files`: where the
    /// compiler's own common source directory lies when `rootDir` is unset.
    include_roots: Vec<PathBuf>,
}

impl BuildOutput {
    /// The output of the package compiling source `file` (canonical): the
    /// nearest config above it that emits to an `outDir` and whose `rootDir`
    /// holds the file. The walk stops at the package root (the first
    /// directory holding a `package.json`).
    pub(super) fn of_source(file: &Path, policy: &PathPolicy) -> Option<Self> {
        if is_declaration_file(file) {
            return None;
        }
        for directory in file.ancestors().skip(1).take(MAX_ANCESTORS) {
            if let Some(output) = Self::in_directory(directory, policy)
                && file.starts_with(&output.root_dir)
                && !file.starts_with(&output.out_dir)
            {
                return Some(output);
            }
            if directory.join("package.json").is_file() {
                return None;
            }
        }
        None
    }

    /// The output declaration `file` (canonical) belongs to: the nearest
    /// config above it whose `outDir` holds it.
    pub(super) fn containing(file: &Path, policy: &PathPolicy) -> Option<Self> {
        if !is_declaration_file(file) {
            return None;
        }
        file.ancestors()
            .skip(1)
            .take(MAX_ANCESTORS)
            .find_map(|directory| {
                Self::in_directory(directory, policy).filter(|output| output.holds(file))
            })
    }

    /// The output the config in `directory` emits, when it names an `outDir`.
    fn in_directory(directory: &Path, policy: &PathPolicy) -> Option<Self> {
        let config = CONFIG_NAMES
            .iter()
            .map(|name| directory.join(name))
            .find(|path| path.is_file())?;
        let options = read_options(&config, policy, 0)?;
        let out_dir = options.out_dir?;
        let root_dir = options
            .root_dir
            .or_else(|| common_root(&options.include_roots))
            .unwrap_or_else(|| directory.to_path_buf());
        let canonical = |path: PathBuf| std::fs::canonicalize(&path).unwrap_or(path);
        let (out_dir, root_dir) = (canonical(out_dir), canonical(root_dir));
        (out_dir != root_dir).then_some(Self { out_dir, root_dir })
    }

    /// Whether `file` (canonical) is a declaration file of this output.
    pub(super) fn holds(&self, file: &Path) -> bool {
        is_declaration_file(file) && file.starts_with(&self.out_dir)
    }

    /// The existing source (canonical) a declaration `file` of this output
    /// was compiled from; `None` for a file outside the output or a source
    /// that no longer exists.
    pub(super) fn source_of(&self, file: &Path) -> Option<PathBuf> {
        if !self.holds(file) {
            return None;
        }
        let relative = file.strip_prefix(&self.out_dir).ok()?.to_str()?;
        let (stem, sources): (&str, &[&str]) = if let Some(stem) = relative.strip_suffix(".d.mts") {
            (stem, &["mts"])
        } else if let Some(stem) = relative.strip_suffix(".d.cts") {
            (stem, &["cts"])
        } else {
            (relative.strip_suffix(".d.ts")?, &["ts", "tsx"])
        };
        sources
            .iter()
            .map(|extension| self.root_dir.join(format!("{stem}.{extension}")))
            .find(|source| source.is_file())
            .map(|source| std::fs::canonicalize(&source).unwrap_or(source))
    }
}

/// The identifier that starts at zero-based `line` and UTF-16 `character` of
/// `text`, as a server's location names it.
pub(super) fn identifier_at(text: &str, line: u32, character: u32) -> Option<String> {
    let text = text.lines().nth(line as usize)?;
    let mut units = 0u32;
    let start = text.char_indices().find_map(|(at, c)| {
        let here = (units == character).then_some(at);
        units += u32::try_from(c.len_utf16()).unwrap_or(1);
        here
    })?;
    let name = text
        .get(start..)?
        .chars()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '$'))
        .collect::<String>();
    (!name.is_empty()).then_some(name)
}

/// The one-based first and last lines of the declaration of `name` in
/// `content`, the source of `file`: a module-scope one first (what a
/// package exports), else the first of that name.
pub(super) fn declaration_lines(content: &str, file: &str, name: &str) -> Option<(u64, u64)> {
    let facts = octocode_tools_shared::declarations::graph_facts(content, file)?;
    let named = || facts.declarations.iter().filter(|found| found.name == name);
    let found = named()
        .find(|found| found.parent.is_none())
        .or_else(|| named().next())?;
    Some((
        u64::from(found.range.start.line) + 1,
        u64::from(found.range.end.line.max(found.range.start.line)) + 1,
    ))
}

/// `.d.ts`, `.d.mts` or `.d.cts`.
pub(super) fn is_declaration_file(file: &Path) -> bool {
    file.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            [".d.ts", ".d.mts", ".d.cts"]
                .iter()
                .any(|suffix| name.ends_with(suffix))
        })
}

/// The emit options of `config`, its own over those its `extends` chain
/// sets; paths resolve beside the config that names them. `None` when the
/// config cannot be read as JSON with comments.
fn read_options(config: &Path, policy: &PathPolicy, depth: usize) -> Option<EmitOptions> {
    let text = octocode_tools_shared::import_resolution::aliases::read_config_text(
        policy,
        &ContentSecurity::new(),
        config,
    )?;
    let value = serde_json::from_str::<Value>(&strip_jsonc(&text)).ok()?;
    let directory = config.parent()?;
    let mut options = match value.get("extends").and_then(Value::as_str) {
        Some(base)
            if depth < MAX_EXTENDS && (base.starts_with("./") || base.starts_with("../")) =>
        {
            let base = octocode_engine::text::lexical(&directory.join(base));
            let base = if base
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                base
            } else {
                PathBuf::from(format!("{}.json", base.display()))
            };
            read_options(&base, policy, depth + 1).unwrap_or_default()
        }
        _ => EmitOptions::default(),
    };
    let compiler = value.get("compilerOptions");
    let option = |key: &str| {
        compiler
            .and_then(|options| options.get(key))
            .and_then(Value::as_str)
            .map(|path| octocode_engine::text::lexical(&directory.join(path)))
    };
    if let Some(out_dir) = option("declarationDir").or_else(|| option("outDir")) {
        options.out_dir = Some(out_dir);
    }
    if let Some(root_dir) = option("rootDir") {
        options.root_dir = Some(root_dir);
    }
    let included = ["include", "files"]
        .iter()
        .filter_map(|key| value.get(*key).and_then(Value::as_array))
        .flatten()
        .filter_map(Value::as_str)
        .map(|pattern| literal_root(directory, pattern))
        .collect::<Vec<_>>();
    if !included.is_empty() {
        options.include_roots = included;
    }
    Some(options)
}

/// The literal directories `pattern` starts with (`src/**/*` → `src`).
fn literal_root(directory: &Path, pattern: &str) -> PathBuf {
    let mut root = directory.to_path_buf();
    let parts = pattern
        .trim_start_matches("./")
        .split('/')
        .collect::<Vec<_>>();
    for (index, part) in parts.iter().enumerate() {
        // The last part names files, unless it is a directory itself.
        let last = index + 1 == parts.len();
        if part.contains(['*', '?', '{', '[']) || (last && part.contains('.')) {
            break;
        }
        root.push(part);
    }
    octocode_engine::text::lexical(&root)
}

/// The deepest directory every one of `roots` lies under.
fn common_root(roots: &[PathBuf]) -> Option<PathBuf> {
    let (first, rest) = roots.split_first()?;
    let mut common = first.clone();
    for root in rest {
        while !root.starts_with(&common) {
            if !common.pop() {
                return None;
            }
        }
    }
    Some(common)
}

/// tsconfig's JSON dialect as plain JSON: comments and trailing commas out.
fn strip_jsonc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => out.extend(chars.next()),
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                while chars.peek().is_some_and(|next| *next != '\n') {
                    chars.next();
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = '\0';
                for next in chars.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
                out.push(' ');
            }
            '}' | ']' => {
                // A comma before the closing bracket is trailing.
                let mut blank = String::new();
                while let Some(last) = out.pop() {
                    if !last.is_whitespace() {
                        if last != ',' {
                            out.push(last);
                        }
                        break;
                    }
                    blank.insert(0, last);
                }
                out.push_str(&blank);
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests;
