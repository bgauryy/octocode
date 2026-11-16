//! The text pre-filter of importer candidates: whether a file's module
//! references can reach the anchor's module at all.
//!
//! An importer of a module export names the module it imports from: a
//! specifier that resolves to the anchor file or to a module that relays it
//! (a barrel). A relative one resolves beside its file; a bare or aliased
//! one (a package name, a `paths` alias) as the project's settings place it
//! over the scope's listed files (`project`), and one they name as project
//! code without a listed file stays a candidate when it binds the name or a
//! namespace whose member the file spells. A file whose every module
//! reference resolves elsewhere (another file, a builtin, an installed
//! package) cannot use the anchor's export, so it is checked without
//! opening it in the server.
//!
//! The filter applies only when the anchor exports `symbol` itself (a
//! script's global, a CommonJS export or a class member is reached without
//! an import of its name), and every undecidable file stays a candidate:
//! unreadable, unparsable, recovered from syntax errors, or loading a
//! module by a computed or dynamic specifier.
use super::super::import_stop::module_candidates;
use super::super::words::spells_word;
use super::*;
use octocode_engine::graph::{GraphFactCode, GraphFactsDocument};
use octocode_engine::text::lexical;
use std::path::PathBuf;

mod project;
use project::{Placed, Project};

/// A dynamic `import(…)`: the literal specifier (single or double
/// quoted), or no group for a computed one.
static DYNAMIC_IMPORT: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    #[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
    regex::Regex::new(r#"\bimport\s*\(\s*(?:'([^']*)'|"([^"]*)")?"#)
        .expect("static dynamic-import regex")
});

/// Candidates one request pre-filters, in sorted order: the list (and so
/// every importer window) is the same on every request, and later ones stay
/// candidates for the server to verify.
const MAX_FILTERED_FILES: usize = 2048;

/// How a file's module references are read.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reading {
    /// An importer candidate: any reference that may reach the anchor.
    Candidate,
    /// A module another one imports the name from: it relays the name only
    /// through static imports and re-exports.
    Relay,
    /// A file that imports the name: the reference binding it must reach
    /// the anchor.
    Binding,
}

/// The anchor's module, when it exports the symbol by its own declaration.
#[derive(Debug)]
pub(super) struct AnchorModule {
    /// Canonical anchor file.
    path: PathBuf,
    symbol: String,
    /// The project's placement of bare specifiers ([`Self::within`]);
    /// without it a bare specifier that binds the name stays a candidate.
    /// A relay of this module's export ([`Self::relay`]) shares it.
    project: Option<Arc<Project>>,
}

/// Graph facts of a parsed file (CommonJS loads included; parsed once per
/// source version); `None` when the file cannot be parsed or its parse
/// recovered from errors (facts may miss an import).
fn clean_facts(content: &str, file: &str) -> Option<Arc<GraphFactsDocument>> {
    let facts = crate::tools::declarations::graph_facts(content, file)?;
    facts_are_clean(&facts).then_some(facts)
}

fn facts_are_clean(facts: &GraphFactsDocument) -> bool {
    !facts.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.code,
            GraphFactCode::ParseError
                | GraphFactCode::RecoveredParse
                | GraphFactCode::ParseDeadline
        )
    })
}

/// Whether `content` may reference a module at all: it spells `import` or
/// `require` (`createRequire` too), or a backslash that may escape one. A
/// file that spells none references no module, which no parse is needed to
/// say.
pub(super) fn may_reference_modules(content: &str) -> bool {
    ["import", "equire", "\\"]
        .iter()
        .any(|spelling| content.contains(spelling))
}

/// Whether `content` spells `word` as a whole identifier after `prefix`
/// (`.` for any member, `binding.` for one binding's member, empty for the
/// bare name).
fn spells(content: &str, prefix: &str, word: &str) -> bool {
    spells_word(
        content.as_bytes(),
        format!("{prefix}{word}").as_bytes(),
        true,
        true,
    )
}

/// The statement of `content` holding the dynamic load at `start..end`: back
/// to the `;` or the enclosing `{` before it and on to the `;` or the
/// enclosing `}` after it, through the brackets the load sits in
/// (`Promise.all([…])`, `.then(({ name }) => …)`). Brackets inside strings
/// and comments count too, which can only cut the statement short.
fn load_statement(content: &str, start: usize, end: usize) -> &str {
    let bytes = content.as_bytes();
    let mut depth = 0usize;
    let mut from = 0;
    for index in (0..start).rev() {
        match bytes[index] {
            b')' | b']' | b'}' => depth += 1,
            b'(' | b'[' => depth = depth.saturating_sub(1),
            b'{' if depth > 0 => depth -= 1,
            b'{' | b';' if depth == 0 => {
                from = index + 1;
                break;
            }
            _ => {}
        }
    }
    let mut depth = 0usize;
    let mut to = bytes.len();
    for (index, byte) in bytes.iter().enumerate().skip(end) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' => depth = depth.saturating_sub(1),
            b'}' if depth > 0 => depth -= 1,
            b'}' | b';' if depth == 0 => {
                to = index;
                break;
            }
            _ => {}
        }
    }
    &content[from..to.max(from)]
}

fn is_relative(specifier: &str) -> bool {
    specifier == "."
        || specifier == ".."
        || specifier.starts_with("./")
        || specifier.starts_with("../")
}

impl AnchorModule {
    /// The anchor `path` (canonical) when its `content` exports `symbol` by
    /// its own declaration (`export function symbol`, `export { symbol }`,
    /// `export default function symbol`); `None` otherwise, which keeps
    /// every candidate.
    pub(super) fn of(path: &str, content: &str, symbol: &str) -> Option<Self> {
        let facts = clean_facts(content, path)?;
        facts
            .exports
            .iter()
            .any(|export| {
                export.source.is_none()
                    && (export.name == symbol || export.local_name.as_deref() == Some(symbol))
            })
            .then(|| Self {
                path: PathBuf::from(canonical(path)),
                symbol: symbol.to_owned(),
                project: None,
            })
    }

    /// This module in the project of the scope `root` (canonical) whose
    /// text scan `listed` these files ([`Scope::listed_files`]): a bare
    /// specifier is then placed as the project resolves it.
    pub(super) fn within(mut self, root: &Path, listed: &[PathBuf], policy: &PathPolicy) -> Self {
        self.project = Some(Arc::new(Project::new(root, listed, &self.path, policy)));
        self
    }

    /// [`Self::within`] the `scope`'s root and the files its text scan of
    /// `symbol` listed; unchanged when that scan did not run or failed.
    pub(super) fn in_scope(self, scope: &Scope, symbol: &str, policy: &PathPolicy) -> Self {
        match scope.listed_files(symbol) {
            Some(listed) => self.within(Path::new(&scope.root), &listed, policy),
            None => self,
        }
    }

    /// The module `relay` (canonical file) as it exports this module's
    /// symbol under `alias` ([`Self::renamed_by`]): what an importer of the
    /// alias must reach, read with the same rules and in the same project.
    fn relay(&self, relay: &str, alias: String) -> Self {
        Self {
            path: PathBuf::from(relay),
            symbol: alias,
            project: self.project.clone(),
        }
    }

    /// The file and the name of the export this module stands for.
    pub(super) fn export(&self) -> (&Path, &str) {
        (&self.path, &self.symbol)
    }

    /// The other names `file` (canonical) exports this module's symbol
    /// under, sorted: `export { symbol as alias } from '…'` whose source
    /// may be this module, or `export { local as alias }` (`export { local
    /// }`) of a binding that imports the symbol from it. An importer of the
    /// alias never spells the symbol, so the alias is the name to look for
    /// among the files that reach `file`. A default export names nothing an
    /// importer must spell, a namespace (`export * as ns`) is read as
    /// `ns.symbol`, and a file that cannot be parsed cleanly names none.
    fn renamed_by(
        &self,
        file: &str,
        content: &str,
        relays: &mut HashMap<PathBuf, bool>,
    ) -> BTreeSet<String> {
        let mut aliases = BTreeSet::new();
        let Some(facts) = clean_facts(content, file) else {
            return aliases;
        };
        let locals = facts
            .imports
            .iter()
            .filter(|import| import.imported_name.as_deref() == Some(self.symbol.as_str()))
            .filter(|import| self.names(&import.specifier, file, true, relays))
            .filter_map(|import| import.local_name.as_deref())
            .collect::<Vec<_>>();
        for export in &facts.exports {
            if export.name == self.symbol || matches!(export.name.as_str(), "default" | "*") {
                continue;
            }
            let local = export.local_name.as_deref().unwrap_or(&export.name);
            let renames = match export.source.as_deref() {
                Some(source) => local == self.symbol && self.names(source, file, true, relays),
                None => locals.contains(&local),
            };
            if renames {
                aliases.insert(export.name.clone());
            }
        }
        aliases
    }

    /// Whether `file` (canonical) may reach the anchor's export through its
    /// module references; `false` only when every reference provably names
    /// another module. `relays` memoizes, per resolved module, whether it
    /// can relay the export.
    pub(super) fn reached_by(
        &self,
        file: &str,
        content: &str,
        relays: &mut HashMap<PathBuf, bool>,
    ) -> bool {
        self.reaches(file, content, relays, Reading::Candidate)
    }

    /// Whether `file` (canonical) binds the name from a module that may be
    /// the anchor's: the reference that binds it must reach the anchor (a
    /// file importing the name from another module and loading something
    /// else dynamically does not).
    pub(super) fn bound_by(
        &self,
        file: &str,
        content: &str,
        facts: &GraphFactsDocument,
        relays: &mut HashMap<PathBuf, bool>,
    ) -> bool {
        !facts_are_clean(facts)
            || self.reaches_facts(file, content, facts, relays, Reading::Binding)
    }

    /// The module references of `file` that may reach the anchor, read as
    /// `reading` says.
    fn reaches(
        &self,
        file: &str,
        content: &str,
        relays: &mut HashMap<PathBuf, bool>,
        reading: Reading,
    ) -> bool {
        let Some(facts) = clean_facts(content, file) else {
            return true;
        };
        self.reaches_facts(file, content, &facts, relays, reading)
    }

    fn reaches_facts(
        &self,
        file: &str,
        content: &str,
        facts: &GraphFactsDocument,
        relays: &mut HashMap<PathBuf, bool>,
        reading: Reading,
    ) -> bool {
        let member = spells(content, ".", &self.symbol);
        // A dynamic `import()` (or an `import('…')` type) is not in the
        // facts: a literal one binds what the file reads off it, a computed
        // one may name any module. A relay re-exports none of them.
        if reading != Reading::Relay {
            for load in DYNAMIC_IMPORT.captures_iter(content) {
                // What binds the name: the statement holding the load
                // destructuring it, on whichever line (`const [a, { name }] =
                // await Promise.all([\n import('x'), …`), or (for a
                // candidate) any `.name` the file reads.
                let (start, end) = load
                    .get(0)
                    .map_or((0, 0), |whole| (whole.start(), whole.end()));
                let binds = spells(load_statement(content, start, end), "", &self.symbol)
                    || (reading == Reading::Candidate && member);
                let reached = match load.get(1).or_else(|| load.get(2)) {
                    Some(specifier) => {
                        self.reference(reading, specifier.as_str(), file, binds, relays)
                    }
                    None => reading == Reading::Candidate || binds,
                };
                if reached {
                    return true;
                }
            }
        }
        for import in &facts.imports {
            // A namespace or default binding reaches the name only as
            // `binding.name`; a side-effect import binds nothing.
            let qualified = || {
                import
                    .local_name
                    .as_deref()
                    .is_some_and(|local| spells(content, &format!("{local}."), &self.symbol))
            };
            let binds = import.local_name.as_deref() == Some(self.symbol.as_str())
                || match import.imported_name.as_deref() {
                    Some(name) => {
                        name == self.symbol || (matches!(name, "*" | "default") && qualified())
                    }
                    None => qualified(),
                };
            if self.reference(reading, &import.specifier, file, binds, relays) {
                return true;
            }
        }
        for export in &facts.exports {
            let Some(source) = export.source.as_deref() else {
                continue;
            };
            let binds = export.name == self.symbol
                || export.local_name.as_deref() == Some(self.symbol.as_str())
                || export.name == "*";
            if self.reference(reading, source, file, binds, relays) {
                return true;
            }
        }
        facts
            .common_js
            .iter()
            .any(|load| match load.specifier.as_deref() {
                // A computed specifier can name any module.
                None => match reading {
                    Reading::Candidate => true,
                    Reading::Relay => false,
                    Reading::Binding => member,
                },
                Some(specifier) => {
                    let line = content
                        .lines()
                        .nth(load.line.saturating_sub(1) as usize)
                        .unwrap_or_default();
                    let binds = member || spells(line, "", &self.symbol);
                    self.reference(reading, specifier, file, binds, relays)
                }
            })
    }

    /// [`Self::names`], when `reading` lets a reference that does not bind
    /// the name count.
    fn reference(
        &self,
        reading: Reading,
        specifier: &str,
        file: &str,
        binds: bool,
        relays: &mut HashMap<PathBuf, bool>,
    ) -> bool {
        (binds || reading != Reading::Binding) && self.names(specifier, file, binds, relays)
    }

    /// Whether one module reference of `file` may reach the anchor: one
    /// that resolves to it (any binding: a default import may be the symbol
    /// under another name), or that `binds` the name from a module that may
    /// relay it. A relative specifier resolves beside `file`; a bare one as
    /// the project places it ([`Project::place`]: an alias, a workspace
    /// package, a `baseUrl` module), so a builtin or an installed package
    /// reaches nothing. A bare one the project cannot place (project code
    /// with no listed file, or no project at all) may reach the anchor when
    /// it `binds` the name.
    pub(super) fn names(
        &self,
        specifier: &str,
        file: &str,
        binds: bool,
        relays: &mut HashMap<PathBuf, bool>,
    ) -> bool {
        let target = if is_relative(specifier) {
            let Some(directory) = Path::new(file).parent() else {
                return true;
            };
            let candidates = module_candidates(&lexical(&directory.join(specifier)));
            if candidates.contains(&self.path) {
                return true;
            }
            // The module the specifier resolves to; none binds nothing.
            let Some(target) = candidates.into_iter().find(|candidate| candidate.is_file()) else {
                return false;
            };
            target
        } else {
            let placed = self
                .project
                .as_ref()
                .map_or(Placed::Unknown, |project| project.place(specifier, file));
            match placed {
                Placed::At(target) => target,
                Placed::Elsewhere => return false,
                Placed::Unknown => return binds,
            }
        };
        let target = std::fs::canonicalize(&target).unwrap_or(target);
        if target == self.path {
            return true;
        }
        if !binds {
            return false;
        }
        if let Some(known) = relays.get(&target) {
            return *known;
        }
        relays.insert(target.clone(), true);
        let relayed = self.relayed_by(&target, relays);
        relays.insert(target, relayed);
        relayed
    }

    /// Whether the module at `target` may relay the anchor's export: its
    /// own imports or re-exports may reach the anchor (it may export what
    /// it imports), or it cannot be read or parsed (a package manifest's
    /// entry point included). A style, data or asset file relays nothing,
    /// nor does a module that declares its own same-named symbol, re-exports
    /// only other names, or loads modules only dynamically. A cycle reads as
    /// a relay until decided.
    fn relayed_by(&self, target: &Path, relays: &mut HashMap<PathBuf, bool>) -> bool {
        let file = target.to_string_lossy();
        if target
            .file_name()
            .is_some_and(|name| name == "package.json")
        {
            return true;
        }
        // Styles, data and assets bind no name.
        if !octocode_engine::lsp::family::is_typescript_family(
            octocode_engine::lsp::config::detect_language_id(&file).as_deref(),
        ) {
            return false;
        }
        let Some(content) = crate::tools::source::read_bounded(
            target,
            crate::security::SourceLimits::REAL.whole_source,
        )
        .ok()
        .and_then(|bytes| super::super::source::admitted_text(&bytes).ok())
        .map(|read| read.text) else {
            return true;
        };
        self.reaches(&file, &content, relays, Reading::Relay)
    }
}

/// Split sorted `candidates` into those that may reach the anchor's export
/// and those whose imports name only other modules ([`AnchorModule`]); the
/// first [`MAX_FILTERED_FILES`] are checked, the rest stay candidates. With
/// no `anchor` module every candidate stays. One blocking worker reads the
/// files in turn and keeps none.
pub(super) async fn split_reachable(
    candidates: Vec<String>,
    anchor: Option<Arc<AnchorModule>>,
    policy: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<(Vec<String>, Vec<String>), LspFailure> {
    let Some(anchor) = anchor else {
        return Ok((candidates, Vec::new()));
    };
    let policy = policy.clone();
    let all = candidates.clone();
    let split = super::super::blocking_cancellable(cancel, move |stopped| {
        let mut relays = HashMap::new();
        let (mut kept, mut unreached) = (Vec::new(), Vec::new());
        for (index, file) in candidates.into_iter().enumerate() {
            let reached = index >= MAX_FILTERED_FILES
                || stopped()
                || read_checked(&policy, &file)
                    .is_none_or(|content| anchor.reached_by(&file, &content, &mut relays));
            if reached {
                kept.push(file);
            } else {
                unreached.push(file);
            }
        }
        (kept, unreached)
    })
    .await?;
    // A failed worker decided nothing: every candidate stays.
    Ok(split.unwrap_or((all, Vec::new())))
}

/// The relays among `files` (canonical) that export `module`'s symbol under
/// another name ([`AnchorModule::renamed_by`]), one module per relay and
/// alias, in the order of `files`. One blocking worker reads the files in
/// turn and keeps none; a file it cannot read names no alias.
pub(super) async fn renamed_exports(
    files: Vec<String>,
    module: Arc<AnchorModule>,
    policy: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Vec<Arc<AnchorModule>>, LspFailure> {
    let policy = policy.clone();
    let renamed = super::super::blocking_cancellable(cancel, move |stopped| {
        let mut relays = HashMap::new();
        let mut found = Vec::new();
        for file in files {
            if stopped() {
                break;
            }
            let Some(content) = read_checked(&policy, &file) else {
                continue;
            };
            for alias in module.renamed_by(&file, &content, &mut relays) {
                found.push(Arc::new(module.relay(&file, alias)));
            }
        }
        found
    })
    .await?;
    // A read the request stopped is the request's end, not a shorter list.
    cancel.check().map_err(LspFailure::cancelled)?;
    Ok(renamed.unwrap_or_default())
}

/// Split `files` (candidates the server proved nothing of) into those
/// holding a reference that binds the name from a module that may be the
/// anchor's ([`AnchorModule::bound_by`]) and those whose every such
/// reference names another module: a builtin, an installed package, another
/// file. The second kind cannot use the anchor's export, whatever the server
/// left undecided. A file that cannot be read or parsed stays in the first
/// kind, and so does every file when no `anchor` module is known. A file
/// that is a candidate for an alias (`relays`: the modules it may import the
/// symbol from under another name) is judged against each of those too: it
/// binds the alias, not the anchor's name. One blocking worker reads the
/// files in turn and keeps none.
pub(super) async fn split_bound(
    files: Vec<String>,
    anchor: Option<Arc<AnchorModule>>,
    relays: Arc<HashMap<String, Vec<Arc<AnchorModule>>>>,
    policy: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<(Vec<String>, Vec<String>), LspFailure> {
    let Some(anchor) = anchor.filter(|_| !files.is_empty()) else {
        return Ok((files, Vec::new()));
    };
    let policy = policy.clone();
    let all = files.clone();
    let split = super::super::blocking_cancellable(cancel, move |stopped| {
        let mut memo = HashMap::<(PathBuf, String), HashMap<PathBuf, bool>>::new();
        files.into_iter().partition(|file| {
            stopped()
                || read_checked(&policy, file).is_none_or(|content| {
                    crate::tools::declarations::graph_facts(&content, file).is_none_or(|facts| {
                        std::iter::once(&anchor)
                            .chain(relays.get(file).into_iter().flatten())
                            .any(|module| {
                                // What relays a name depends on the name.
                                let key = (module.path.clone(), module.symbol.clone());
                                let relays = memo.entry(key).or_default();
                                module.bound_by(file, &content, &facts, relays)
                            })
                    })
                })
        })
    })
    .await?;
    // A failed worker decided nothing: every file stays.
    Ok(split.unwrap_or((all, Vec::new())))
}

/// The bounded, policy-readable text of `file`; `None` when it cannot be
/// read or decoded.
pub(super) fn read_checked(policy: &PathPolicy, file: &str) -> Option<String> {
    let valid = policy.validate_read(file).ok()?;
    let bytes = crate::tools::source::read_bounded(
        &valid.canonical,
        crate::security::SourceLimits::REAL.whole_source,
    )
    .ok()?;
    super::super::source::admitted_text(&bytes)
        .ok()
        .map(|read| read.text)
}

#[cfg(test)]
mod tests;
