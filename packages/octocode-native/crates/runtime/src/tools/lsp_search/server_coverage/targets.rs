//! Rust target gates: code a `cfg` of the compilation target (`windows`,
//! `unix`, `target_os = "…"`) leaves out of the one target rust-analyzer
//! analyzes. A file whose every use of the name sits under such a gate is
//! missing from the answer for that reason alone, whatever the package's
//! build scripts generate, and a rerun under a target where the gate holds
//! (`rustContext.target`) answers for it.
use super::*;
use std::collections::BTreeSet;

/// What a compilation target sets of the `cfg` options this reads.
#[derive(Debug, PartialEq, Eq)]
pub(in super::super) struct TargetFacts {
    /// The target triple; `None` for the host this binary was built for.
    triple: Option<&'static str>,
    os: &'static str,
    family: &'static str,
    arch: &'static str,
    pointer_width: &'static str,
    endian: &'static str,
    /// `target_env` and `target_vendor`; `None` where unknown (the host).
    env: Option<&'static str>,
    vendor: Option<&'static str>,
}

const fn listed(
    triple: &'static str,
    (arch, pointer_width): (&'static str, &'static str),
    vendor: &'static str,
    (os, family): (&'static str, &'static str),
    env: &'static str,
) -> TargetFacts {
    TargetFacts {
        triple: Some(triple),
        os,
        family,
        arch,
        pointer_width,
        endian: "little",
        env: Some(env),
        vendor: Some(vendor),
    }
}

const X86_64: (&str, &str) = ("x86_64", "64");
const AARCH64: (&str, &str) = ("aarch64", "64");
const X86: (&str, &str) = ("x86", "32");
const WINDOWS: (&str, &str) = ("windows", "windows");
const LINUX: (&str, &str) = ("linux", "unix");
const MACOS: (&str, &str) = ("macos", "unix");

/// Rust's Tier 1 targets with host tools (the rustc book's platform
/// support): the targets a toolchain is guaranteed to analyze, so the only
/// ones a rerun is offered for and the only explicit `rustContext.target`
/// values whose `cfg` is known here.
const TIER_ONE: [TargetFacts; 8] = [
    listed("aarch64-apple-darwin", AARCH64, "apple", MACOS, ""),
    listed("aarch64-pc-windows-msvc", AARCH64, "pc", WINDOWS, "msvc"),
    listed(
        "aarch64-unknown-linux-gnu",
        AARCH64,
        "unknown",
        LINUX,
        "gnu",
    ),
    listed("i686-pc-windows-msvc", X86, "pc", WINDOWS, "msvc"),
    listed("i686-unknown-linux-gnu", X86, "unknown", LINUX, "gnu"),
    listed("x86_64-pc-windows-gnu", X86_64, "pc", WINDOWS, "gnu"),
    listed("x86_64-pc-windows-msvc", X86_64, "pc", WINDOWS, "msvc"),
    listed("x86_64-unknown-linux-gnu", X86_64, "unknown", LINUX, "gnu"),
];

/// The host: rust-analyzer analyzes for it unless a target is named.
const HOST: TargetFacts = TargetFacts {
    triple: None,
    os: std::env::consts::OS,
    family: std::env::consts::FAMILY,
    arch: std::env::consts::ARCH,
    pointer_width: if usize::BITS == 64 { "64" } else { "32" },
    endian: if cfg!(target_endian = "big") {
        "big"
    } else {
        "little"
    },
    env: None,
    vendor: None,
};

impl TargetFacts {
    /// The listed target named `triple`.
    pub(in super::super) fn of_triple(triple: &str) -> Option<&'static Self> {
        TIER_ONE.iter().find(|target| target.triple == Some(triple))
    }

    /// The value of the `cfg` option `key`; `None` for an option the target
    /// does not set or this does not know.
    fn value(&self, key: &str) -> Option<&'static str> {
        match key {
            "target_os" => Some(self.os),
            "target_family" => Some(self.family),
            "target_arch" => Some(self.arch),
            "target_pointer_width" => Some(self.pointer_width),
            "target_endian" => Some(self.endian),
            "target_env" => self.env,
            "target_vendor" => self.vendor,
            _ => None,
        }
    }
}

/// The target a Rust server analyzes for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in super::super) enum AnalysisTarget {
    /// No `cargo.target`: the host.
    #[default]
    Host,
    /// A named target whose `cfg` is known.
    Listed(&'static TargetFacts),
    /// A named target whose `cfg` is not known here: no gate is judged.
    Unknown,
}

impl AnalysisTarget {
    /// The target `cargo.target` names (`None`: the host).
    pub(in super::super) fn of(triple: Option<&str>) -> Self {
        match triple {
            None => Self::Host,
            Some(triple) => TargetFacts::of_triple(triple).map_or(Self::Unknown, Self::Listed),
        }
    }

    fn facts(self) -> Option<&'static TargetFacts> {
        match self {
            Self::Host => Some(&HOST),
            Self::Listed(facts) => Some(facts),
            Self::Unknown => None,
        }
    }
}

/// A `cfg` predicate.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in super::super) enum Predicate {
    /// `name` (`windows`, `unix`, `test`).
    Flag(String),
    /// `key = "value"`.
    Pair(String, String),
    All(Vec<Self>),
    Any(Vec<Self>),
    Not(Box<Self>),
}

impl Predicate {
    /// Whether the predicate holds for `target`; `None` when an option the
    /// target does not decide (a feature, `test`, a custom cfg) leaves it
    /// open.
    pub(in super::super) fn holds(&self, target: &TargetFacts) -> Option<bool> {
        match self {
            // `windows` and `unix` are the target families under their
            // short names (the Rust reference's conditional compilation).
            Self::Flag(name) if name == "windows" || name == "unix" => Some(target.family == name),
            Self::Flag(_) => None,
            Self::Pair(key, value) => target.value(key).map(|set| set == value),
            Self::All(parts) => {
                let held = parts.iter().map(|part| part.holds(target));
                fold(held, false)
            }
            Self::Any(parts) => {
                let held = parts.iter().map(|part| part.holds(target));
                fold(held, true)
            }
            Self::Not(part) => part.holds(target).map(|held| !held),
        }
    }

    /// Whether the predicate reads an option of the target at all.
    fn names_target(&self) -> bool {
        match self {
            Self::Flag(name) => name == "windows" || name == "unix",
            Self::Pair(key, _) => key.starts_with("target_"),
            Self::All(parts) | Self::Any(parts) => parts.iter().any(Self::names_target),
            Self::Not(part) => part.names_target(),
        }
    }
}

impl std::fmt::Display for Predicate {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let list = |out: &mut std::fmt::Formatter<'_>, name: &str, parts: &[Self]| {
            write!(out, "{name}(")?;
            for (index, part) in parts.iter().enumerate() {
                if index > 0 {
                    write!(out, ", ")?;
                }
                write!(out, "{part}")?;
            }
            write!(out, ")")
        };
        match self {
            Self::Flag(name) => write!(out, "{name}"),
            Self::Pair(key, value) => write!(out, "{key} = \"{value}\""),
            Self::All(parts) => list(out, "all", parts),
            Self::Any(parts) => list(out, "any", parts),
            Self::Not(part) => write!(out, "not({part})"),
        }
    }
}

/// `all` (`decides` false) or `any` (`decides` true) over three-valued
/// parts: one part equal to `decides` settles it, an open part leaves it
/// open, and otherwise it is the opposite.
fn fold(held: impl Iterator<Item = Option<bool>>, decides: bool) -> Option<bool> {
    let mut open = false;
    for part in held {
        match part {
            Some(part) if part == decides => return Some(decides),
            Some(_) => {}
            None => open = true,
        }
    }
    (!open).then_some(!decides)
}

/// One predicate at the start of `text`, and the text after it.
fn parse(text: &str) -> Option<(Predicate, &str)> {
    let text = text.trim_start();
    let end = text
        .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .unwrap_or(text.len());
    let (name, rest) = text.split_at(end);
    if name.is_empty() {
        return None;
    }
    let rest = rest.trim_start();
    if let Some(mut rest) = rest.strip_prefix('(') {
        let mut parts = Vec::new();
        loop {
            rest = rest.trim_start();
            if let Some(after) = rest.strip_prefix(')') {
                rest = after;
                break;
            }
            let (part, after) = parse(rest)?;
            parts.push(part);
            rest = after.trim_start();
            rest = rest.strip_prefix(',').unwrap_or(rest);
        }
        let predicate = match name {
            "all" => Predicate::All(parts),
            "any" => Predicate::Any(parts),
            "not" if parts.len() == 1 => Predicate::Not(Box::new(parts.pop()?)),
            _ => return None,
        };
        return Some((predicate, rest));
    }
    if let Some(rest) = rest.strip_prefix('=') {
        let rest = rest.trim_start().strip_prefix('"')?;
        let (value, rest) = rest.split_once('"')?;
        return Some((Predicate::Pair(name.to_owned(), value.to_owned()), rest));
    }
    Some((Predicate::Flag(name.to_owned()), rest))
}

/// The text before byte `at`, which a match or a char search placed on a
/// character boundary.
fn before_of(text: &str, at: usize) -> &str {
    text.get(..at).unwrap_or_default()
}

/// The text from byte `at` on.
fn after_of(text: &str, at: usize) -> &str {
    text.get(at..).unwrap_or_default()
}

/// The predicates of the `cfg` attributes in `text` (`#[cfg(…)]`,
/// `#![cfg(…)]`); a `cfg_attr` gates an attribute, not code.
pub(in super::super) fn cfg_predicates(text: &str) -> Vec<Predicate> {
    let mut found = Vec::new();
    for (at, _) in text.match_indices("cfg") {
        let before = before_of(text, at).trim_end();
        let attribute = before.ends_with("#[") || before.ends_with("#![");
        let Some(inner) = after_of(text, at + "cfg".len())
            .trim_start()
            .strip_prefix('(')
        else {
            continue;
        };
        if attribute
            && let Some((predicate, rest)) = parse(inner)
            && rest.trim_start().starts_with(')')
        {
            found.push(predicate);
        }
    }
    found
}

/// The attribute lines that govern the item starting at zero-based `start`
/// of `lines`: the attributes and comments directly above it, and the
/// attributes it opens with (a parse may count them into the item).
fn item_attributes(lines: &[&str], start: usize) -> String {
    let annotates = |line: &str| {
        let line = line.trim_start();
        line.starts_with("#[") || line.starts_with("//")
    };
    let above = lines[..start.min(lines.len())]
        .iter()
        .rev()
        .take_while(|line| annotates(line))
        .copied();
    let opening = lines
        .iter()
        .skip(start)
        .take_while(|line| annotates(line))
        .copied();
    above.chain(opening).collect::<Vec<_>>().join("\n")
}

/// The file's own inner gates (`#![cfg(…)]`).
fn inner_gates(text: &str) -> Vec<Predicate> {
    text.lines()
        .filter(|line| line.trim_start().starts_with("#![cfg"))
        .flat_map(cfg_predicates)
        .collect()
}

/// The module name a file declares its contents under: its stem, or its
/// directory's for a `mod.rs`.
fn module_name(file: &Path) -> Option<String> {
    let stem = file.file_stem()?.to_string_lossy();
    if stem == "mod" {
        return Some(file.parent()?.file_name()?.to_string_lossy().into_owned());
    }
    Some(stem.into_owned())
}

/// The gates on the `mod` declaration of `file` in the `parent` source: the
/// attributes of the line that declares the module by its name, or by a
/// `#[path]` naming the file. `None` when no such line is found.
fn declaration_gates(parent: &str, file: &Path) -> Option<Vec<Predicate>> {
    let name = module_name(file)?;
    let file_name = file.file_name()?.to_string_lossy().into_owned();
    let lines = parent.lines().collect::<Vec<_>>();
    let declares = |line: &str| {
        line.match_indices("mod ").any(|(at, _)| {
            let boundary = before_of(line, at)
                .chars()
                .next_back()
                .is_none_or(|ch| !(ch.is_alphanumeric() || ch == '_'));
            let rest = after_of(line, at + "mod ".len()).trim_start();
            let end = rest
                .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                .unwrap_or(rest.len());
            boundary
                && !before_of(rest, end).is_empty()
                && after_of(rest, end).trim_start().starts_with([';', '{'])
        })
    };
    let names = |line: &str| {
        line.match_indices("mod ").any(|(at, _)| {
            let rest = after_of(line, at + "mod ".len()).trim_start();
            rest.strip_prefix(name.as_str())
                .is_some_and(|after| after.trim_start().starts_with([';', '{']))
        })
    };
    lines.iter().enumerate().find_map(|(index, line)| {
        if !declares(line) {
            return None;
        }
        let attributes = item_attributes(&lines, index);
        let by_path = attributes.contains("path") && attributes.contains(&file_name);
        (names(line) || by_path).then(|| cfg_predicates(&attributes))
    })
}

fn read(path: &Path) -> Option<String> {
    let bytes = crate::tools::source::read_bounded(
        path,
        octocode_engine::num::saturating_usize(MAX_FILE_BYTES),
    )
    .ok()?;
    super::super::source::admitted_text(&bytes)
        .ok()
        .map(|read| read.text)
}

/// The gates every line of `file` compiles under: its own inner gates, and
/// those of each `mod` declaration up to its crate root. A file several
/// files declare as a module compiles once per declaration, so no one chain
/// gates it: only its inner gates count then.
fn file_gates(file: &Path, text: &str, stopped: &dyn Fn() -> bool) -> Vec<Predicate> {
    let mut gates = inner_gates(text);
    let mut seen = BTreeSet::new();
    let mut current = file.to_path_buf();
    while seen.insert(current.clone()) && !stopped() {
        let Some(includes) = package_includes(&current.to_string_lossy(), stopped) else {
            break;
        };
        let [parent] = includes.declared_by.as_slice() else {
            break;
        };
        let Some(parent_text) = read(parent) else {
            break;
        };
        gates.extend(declaration_gates(&parent_text, &current).unwrap_or_default());
        gates.extend(inner_gates(&parent_text));
        current.clone_from(parent);
    }
    gates
}

/// The gates of `file` that leave every code use of `name` in it out of
/// `target`: empty when a use compiles for the target, or when the file
/// cannot be read or parsed (nothing is proven then).
fn inactive_gates(
    file: &Path,
    name: &str,
    target: &TargetFacts,
    stopped: &dyn Fn() -> bool,
) -> Vec<Predicate> {
    let Some(text) = read(file) else {
        return Vec::new();
    };
    let path = file.to_string_lossy();
    let Some(uses) = octocode_engine::lsp::resolver::code_occurrence_lines(&path, &text, name)
        .filter(|uses| !uses.is_empty())
    else {
        return Vec::new();
    };
    let inactive = |gates: Vec<Predicate>| {
        gates
            .into_iter()
            .filter(|gate| gate.holds(target) == Some(false))
            .collect::<Vec<_>>()
    };
    let whole = inactive(file_gates(file, &text, stopped));
    if !whole.is_empty() {
        return whole;
    }
    let Some(document) = crate::tools::declarations::file_document(&text, &path) else {
        return Vec::new();
    };
    let lines = text.lines().collect::<Vec<_>>();
    let mut found = BTreeSet::new();
    for line in uses {
        let around = document
            .declarations
            .iter()
            .filter(|item| {
                (item.range.start.line as usize..=item.range.end.line as usize).contains(&line)
            })
            .flat_map(|item| {
                inactive(cfg_predicates(&item_attributes(
                    &lines,
                    item.range.start.line as usize,
                )))
            })
            .collect::<Vec<_>>();
        // One use that compiles for the target is a use the server sees.
        if around.is_empty() {
            return Vec::new();
        }
        found.extend(around);
    }
    found.into_iter().collect()
}

/// A file a target gate leaves out of the analysis, and why.
#[derive(Debug)]
pub(super) struct TargetGated {
    /// Canonical path.
    pub(super) file: String,
    /// The gates that do not hold for the analyzed target.
    pub(super) gates: Vec<Predicate>,
}

/// The files under `root` (the server's workspace) that use `name` in
/// code, are missing from the answer, and whose every use a target gate
/// leaves out of the analyzed `target`. The candidates are the request's
/// own text check ([`Scope::text_only_files`]); `stopped` ends the look
/// early (the files found so far stand).
pub(super) fn target_gated_files(
    scope: &Scope,
    root: &str,
    name: &str,
    target: AnalysisTarget,
    stopped: &dyn Fn() -> bool,
) -> Vec<TargetGated> {
    let (Some(facts), Some(missed)) = (target.facts(), scope.text_only_files(name)) else {
        return Vec::new();
    };
    let root = super::super::scope::canonical(root);
    missed
        .into_iter()
        .filter(|file| Path::new(file).starts_with(&root))
        .take_while(|_| !stopped())
        .filter_map(|file| {
            let gates = inactive_gates(Path::new(&file), name, facts, stopped);
            (!gates.is_empty()).then_some(TargetGated { file, gates })
        })
        .collect()
}

/// The listed target every one of `gates` may hold for, the one nearest the
/// analyzed `target` first (same architecture, then same system): the
/// target a rerun names. `None` when no listed target satisfies them.
fn rerun_target(gates: &[Predicate], target: AnalysisTarget) -> Option<&'static str> {
    let now = target.facts();
    TIER_ONE
        .iter()
        .filter(|candidate| {
            gates
                .iter()
                .all(|gate| gate.holds(candidate) != Some(false))
        })
        .min_by_key(|candidate| {
            (
                now.is_some_and(|now| now.arch != candidate.arch),
                now.is_some_and(|now| now.os != candidate.os),
            )
        })
        .and_then(|candidate| candidate.triple)
}

/// Flag a row whose missed files (`gated`) sit behind target gates:
/// partial (`cfgGatedFiles`), with the gates and files named, and, on the
/// walk's first page, one rerun per target that compiles them in
/// (`rustContext.target`).
pub(super) fn annotate_targets(
    row: &mut Value,
    query: &LspSearchQuery,
    scope: &Scope,
    target: AnalysisTarget,
    gated: &[TargetGated],
) {
    if gated.is_empty() {
        return;
    }
    let shown = |file: &str| {
        Path::new(file).strip_prefix(&scope.root).map_or_else(
            |_| file.to_owned(),
            |path| path.to_string_lossy().into_owned(),
        )
    };
    let named = gated
        .iter()
        .map(|found| {
            let gates = found
                .gates
                .iter()
                .filter(|gate| gate.names_target())
                .map(|gate| format!("cfg({gate})"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} ({gates})", shown(&found.file))
        })
        .collect::<Vec<_>>()
        .join(", ");
    let reruns = gated
        .iter()
        .filter_map(|found| rerun_target(&found.gates, target))
        .collect::<BTreeSet<_>>();
    let repair = if reruns.is_empty() {
        "set rustContext.target to a target they hold for".to_owned()
    } else {
        format!(
            "the related rerun sets rustContext.target ({})",
            reruns.iter().copied().collect::<Vec<_>>().join(", ")
        )
    };
    let warning = format!(
        "{} file(s) that use this name compile only under target gates the analyzed target leaves out: {named}. rust-analyzer analyzes one target; {repair}.",
        gated.len()
    );
    flag_partial(row, query, CFG_REASON, Some(&warning), scope);
    if continues_a_page(query) || !lists_reason(row, CFG_REASON) {
        return;
    }
    for triple in reruns {
        let mut context = lsp_search_query::rust_context(query).map_or_else(
            || serde_json::json!({}),
            |context| serde_json::json!(context),
        );
        context["target"] = serde_json::json!(triple);
        if let Ok(rerun) =
            lsp_search_query::edited(query, &serde_json::json!({"rustContext": context}))
        {
            crate::response::result::attach_related(
                row,
                crate::response::result::Continuation::related_query(rerun).build(),
            );
        }
    }
}
