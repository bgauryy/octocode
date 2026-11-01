//! The text pre-filter of importer candidates: whether a file's module
//! references can reach the anchor's module at all.
//!
//! An importer of a module export names the module it imports from: a
//! relative specifier that resolves to the anchor file or to a module that
//! relays it (a barrel), or a bare or aliased specifier (a package name, a
//! `paths` alias) that only the project's resolution settings could place,
//! when it binds the name or a namespace whose member the file spells. A
//! file whose every module reference resolves elsewhere cannot use the
//! anchor's export, so it is checked without opening it in the server.
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
}

/// Graph facts of a parsed file (CommonJS loads included; parsed once per
/// source version); `None` when the file cannot be parsed or its parse
/// recovered from errors (facts may miss an import).
fn clean_facts(content: &str, file: &str) -> Option<Arc<GraphFactsDocument>> {
    let facts = crate::tools::declarations_cache::graph_facts(content, file)?;
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
            })
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
                // What binds the name: the load's own line destructuring it,
                // or (for a candidate) any `.name` the file reads.
                let start = load.get(0).map_or(0, |whole| whole.start());
                let line = content[start..].lines().next().unwrap_or_default();
                let line = format!(
                    "{}{line}",
                    content[..start].rsplit('\n').next().unwrap_or_default()
                );
                let binds =
                    spells(&line, "", &self.symbol) || (reading == Reading::Candidate && member);
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

    /// Whether one module reference of `file` may reach the anchor: a
    /// relative one that resolves to it (any binding: a default import may
    /// be the symbol under another name), or that `binds` the name from a
    /// module that may relay it; or a bare one that `binds` the name (its
    /// resolution is the project's).
    fn names(
        &self,
        specifier: &str,
        file: &str,
        binds: bool,
        relays: &mut HashMap<PathBuf, bool>,
    ) -> bool {
        if !is_relative(specifier) {
            return binds;
        }
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
        let Some(content) =
            crate::tools::source::read_bounded(target, octocode_engine::signatures::MAX_PARSE_SIZE)
                .ok()
                .and_then(|bytes| super::super::source::admitted_text(&bytes).ok())
        else {
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

/// The bounded, policy-readable text of `file`; `None` when it cannot be
/// read or decoded.
pub(super) fn read_checked(policy: &PathPolicy, file: &str) -> Option<String> {
    let valid = policy.validate_read(file).ok()?;
    let bytes = crate::tools::source::read_bounded(
        &valid.canonical,
        octocode_engine::signatures::MAX_PARSE_SIZE,
    )
    .ok()?;
    super::super::source::admitted_text(&bytes).ok()
}

#[cfg(test)]
mod tests;
