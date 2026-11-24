//! Architecture guards the `tools/` layer test does not cover: the crate edges
//! of the workspace, and tool knowledge in the shared `response` stage
//! (`octocode-runtime-base`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

include!("../../runtime-base/src/test_support/production_source.rs");

/// Each crate and the workspace crates it may depend on in `[dependencies]`.
/// The graph is one order, lowest first, so no two crates import each other:
/// a new edge is a decision made here, not by a stray `Cargo.toml` line.
const CRATE_EDGES: &[(&str, &[&str])] = &[
    ("io", &[]),
    ("contract-base", &[]),
    ("contract-types", &["contract-base"]),
    ("engine", &["io"]),
    ("github", &["io", "contract-base"]),
    (
        "runtime-base",
        &["io", "contract-types", "engine", "github"],
    ),
    (
        "tools-shared",
        &["io", "contract-types", "engine", "github", "runtime-base"],
    ),
    (
        "tool-ast",
        &["contract-types", "engine", "runtime-base", "tools-shared"],
    ),
    (
        "tool-clasify",
        &[
            "io",
            "contract-types",
            "engine",
            "runtime-base",
            "tools-shared",
        ],
    ),
    (
        "tool-graph",
        &[
            "io",
            "contract-types",
            "engine",
            "runtime-base",
            "tools-shared",
        ],
    ),
    (
        "tool-local",
        &["contract-types", "engine", "runtime-base", "tools-shared"],
    ),
    (
        "tool-lsp",
        &[
            "io",
            "contract-types",
            "engine",
            "runtime-base",
            "tools-shared",
        ],
    ),
    (
        "tool-packages",
        &[
            "io",
            "contract-types",
            "engine",
            "github",
            "runtime-base",
            "tools-shared",
        ],
    ),
    ("test-support", &["engine"]),
    (
        "tool-github",
        &[
            "io",
            "contract-types",
            "engine",
            "github",
            "runtime-base",
            "tools-shared",
            "test-support",
        ],
    ),
    (
        "runtime",
        &[
            "io",
            "contract-types",
            "engine",
            "github",
            "runtime-base",
            "tools-shared",
            "tool-ast",
            "tool-clasify",
            "tool-github",
            "tool-graph",
            "tool-local",
            "tool-lsp",
            "tool-packages",
        ],
    ),
    (
        "cli",
        &[
            "io",
            "contract-types",
            "engine",
            "github",
            "runtime-base",
            "runtime",
        ],
    ),
];
/// Where a package name maps to its directory under `crates/`.
fn crate_dir(package: &str) -> &str {
    match package {
        "octocode-native" => "runtime",
        other => other.strip_prefix("octocode-").unwrap_or(other),
    }
}

fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The workspace crates `krate` lists under `[dependencies]` (and target
/// tables), by directory name.
fn workspace_dependencies(krate: &str) -> BTreeSet<String> {
    let text = std::fs::read_to_string(crates_dir().join(krate).join("Cargo.toml"))
        .unwrap_or_else(|error| panic!("read {krate}/Cargo.toml: {error}"));
    let manifest: toml::Value = toml::from_str(&text).expect("manifest parses");
    let mut tables = vec![manifest.get("dependencies")];
    if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
        tables.extend(targets.values().map(|target| target.get("dependencies")));
    }
    tables
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_table)
        .flat_map(|table| table.iter())
        .filter(|(_, spec)| spec.get("path").is_some())
        .map(|(name, _)| crate_dir(name).to_owned())
        .collect()
}

#[test]
fn workspace_crate_edges_are_the_declared_ones() {
    for (krate, allowed) in CRATE_EDGES {
        let found = workspace_dependencies(krate);
        let allowed = allowed.iter().map(|name| (*name).to_owned()).collect();
        assert_eq!(
            found, allowed,
            "`{krate}` depends on other workspace crates"
        );
    }
    let members = std::fs::read_dir(crates_dir())
        .expect("read crates/")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("Cargo.toml").is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<BTreeSet<_>>();
    let declared = CRATE_EDGES
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(members, declared, "a crate is missing from CRATE_EDGES");
}

/// `ARCHITECTURE.md` states the crate edges in one sentence (`a` → `b`, `c`;
/// leaves named without an arrow); it lists exactly `CRATE_EDGES`.
#[test]
fn architecture_doc_lists_the_declared_crate_edges() {
    let doc = std::fs::read_to_string(crates_dir().join("../ARCHITECTURE.md")).expect("read doc");
    let line = doc
        .lines()
        .find(|line| line.contains("`CRATE_EDGES`):"))
        .expect("the crate-edge sentence");
    let names = |text: &str| -> BTreeSet<String> {
        text.split('`')
            .skip(1)
            .step_by(2)
            .map(str::to_owned)
            .collect()
    };
    let mut documented = BTreeMap::new();
    for part in line.split_once("):").expect("edge list").1.split(';') {
        match part.split_once('→') {
            Some((krate, edges)) => {
                let krate = names(krate).into_iter().next().expect("crate name");
                documented.insert(krate, names(edges));
            }
            None => documented.extend(names(part).into_iter().map(|leaf| (leaf, BTreeSet::new()))),
        }
    }
    let declared = CRATE_EDGES
        .iter()
        .map(|(krate, edges)| {
            let edges = edges.iter().map(|name| (*name).to_owned()).collect();
            ((*krate).to_owned(), edges)
        })
        .collect::<BTreeMap<_, BTreeSet<_>>>();
    assert_eq!(
        documented, declared,
        "ARCHITECTURE.md crate edges differ from CRATE_EDGES"
    );
}

#[test]
fn workspace_crate_edges_form_no_cycle() {
    let position: BTreeMap<&str, usize> = CRATE_EDGES
        .iter()
        .enumerate()
        .map(|(at, (name, _))| (*name, at))
        .collect();
    for (krate, allowed) in CRATE_EDGES {
        for dependency in *allowed {
            assert!(
                position[dependency] < position[krate],
                "`{krate}` -> `{dependency}` points up the declared order"
            );
        }
    }
}

/// Files of `response/` that may name a `ToolId` variant in production code,
/// each with its reason. The stage is shared: any other tool-specific branch
/// belongs to the tool and reaches the stage through its output.
const TOOL_AWARE_RESPONSE_FILES: &[(&str, &str)] = &[(
    "result/error.rs",
    "missing_path_lead: the one structureSearch tree lead every local tool gives a missing path",
)];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn response_names_no_tool_outside_its_allowlist() {
    let response = Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime-base/src/response");
    let pattern = regex::Regex::new(r"\bToolId::[A-Z][a-z]\w*").expect("static pattern");
    let mut files = Vec::new();
    rust_files(&response, &mut files);
    let mut offenders = Vec::new();
    for file in files {
        let shown = file
            .strip_prefix(&response)
            .expect("under response")
            .to_string_lossy()
            .replace('\\', "/");
        if shown.contains("test")
            || TOOL_AWARE_RESPONSE_FILES
                .iter()
                .any(|(name, _)| *name == shown)
        {
            continue;
        }
        let source = std::fs::read_to_string(&file).expect("read source");
        let code = code_only(&production_text(&source));
        if let Some(hit) = pattern.find(&code) {
            offenders.push(format!("{shown}: {}", hit.as_str()));
        }
    }
    assert!(
        offenders.is_empty(),
        "tool knowledge in response/: {offenders:?}"
    );
}

#[test]
fn every_allowlisted_response_file_still_names_a_tool() {
    let response = Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime-base/src/response");
    let pattern = regex::Regex::new(r"\bToolId::[A-Z][a-z]\w*").expect("static pattern");
    for (name, _) in TOOL_AWARE_RESPONSE_FILES {
        let source = std::fs::read_to_string(response.join(name)).expect("allowlisted file exists");
        let code = code_only(&production_text(&source));
        assert!(
            pattern.is_match(&code),
            "{name} no longer needs its allowlist entry"
        );
    }
}
