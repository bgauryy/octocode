//! `octocode graph`: build a code-graph snapshot once (`ingest`), then answer
//! bounded questions from it (`query`) without re-parsing the repository.
use clap::{Args, Subcommand};
use octocode_native::runtime::ToolRuntime;
use octocode_native::tools::ast_graph::store::{IngestOptions, OPS, QueryOptions};
use std::path::PathBuf;

/// `octocode graph --help`: the whole agent workflow on one screen.
pub(super) const GRAPH_HELP: &str = "\
Persisted code graph: parse a repository once, then answer structural questions in milliseconds.

WORKFLOW
  octocode graph ingest .                  build a snapshot (about 1k-3k files/s; unchanged tree = reused)
  octocode graph query stats               size, languages, most-imported files, gaps
  octocode graph query find <name>         locate a node id
  octocode graph query callers <id>        who calls a symbol (dependents: who imports a file)
  octocode graph query impact <id>         blast radius: affected files by depth, tests to run
  octocode graph query impact --since main blast radius of a branch diff
  octocode graph query issues              ranked possible problems (cycles, dead code, ...)
  octocode graph query drift --baseline <id>  structural diff from another snapshot
  octocode graph query stale               sources or configs changed/added since ingest -> re-ingest

NODE IDS
  src/a.ts               file (root-relative; absolute paths are accepted too)
  src/a.ts#Class.method  symbol (an @<line> suffix appears only when a name repeats)
  pkg:react              external package
  A bare name or Class.method also resolves; ambiguity exits 2 and lists candidates.

EVIDENCE
  Syntax-derived. Imports are module-resolved (tsconfig paths, workspaces, cargo, go.mod, python
  roots). Calls carry via (local|import|namespace|unique-name) and confidence (high|medium|low).
  Call answers carry coverage.callInternalRecall (below 0.9 they add a warning: a lower bound).
  issues findings carry a tier (100/90/60, see --min-tier); the default view hides tier 60.
  Treat results as leads: confirm identity with lspSearch before deleting or renaming.

OUTPUT
  Readable text on a terminal; one JSON object on a pipe or with --json. Errors follow the
  output (JSON on stdout, else text on stderr). Lists carry total and results; a cut adds
  truncated plus next, a paste-ready command. Exit: 0 ok, 1 empty, 2 bad input or ambiguous,
  3 no graph or node, 5 error. A cut result is still 0: run next for more.

STORAGE
  <workspace>/.octocode/graph/<UTC time>-<scope>-<root tag>/{graph.bin, manifest.json}; latest
  names the newest. --graph takes an id, a scope, or a substring of exactly one id (several:
  exit 2). The workspace is --workspace, else the nearest .git ancestor of the current directory
  inside the allowed roots (WORKSPACE_ROOT, else the current directory).
  Files over 1 MB stay as unparsed nodes; minified bundles keep their imports but no symbols.";

#[derive(Subcommand)]
pub(super) enum GraphCommand {
    /// Parse a directory and publish a snapshot (graph.bin + manifest.json).
    ///
    /// Writes <workspace>/.octocode/graph/<UTC time>-<scope>-<root tag>/ and updates `latest`;
    /// keeps the newest 3 snapshots per scanned root (--keep). Honors .gitignore; prunes hidden
    /// directories and dependency, build-output, and environment directories (node_modules,
    /// target, venv, ...).
    /// Prints a receipt: counts, call-link rate, scan gaps, and a `next` command.
    #[command(verbatim_doc_comment)]
    Ingest(Box<IngestArgs>),
    /// Answer one bounded question from a snapshot (default: the workspace's latest).
    ///
    /// LOOKUP     stats · find <text> · node <ref> · symbols <file|symbol>
    /// NEIGHBORS  deps <ref> · dependents <ref> · callers <ref> · callees <ref>
    /// TRAVERSE   path <from> <to> · walk <ref> [--direction out|in|both] [--edge ..] [--depth n]
    /// STRUCTURE  cycles · hubs [--direction in|out] · diagnostics [code|path] · stale
    /// ANALYSIS   issues [--detector a,b] [--min-score x] [--baseline <snapshot>] [--entry a,b]
    ///            impact <ref> | --changed a,b | --since <git-rev>
    ///            drift --baseline <snapshot>
    ///
    /// deps/dependents follow imports for files and calls for symbols (a file's dependents also
    /// use its exports through a re-exporting module); override with --edge
    /// (contains, imports, uses, calls, inherits). A file's dependents list bare-path imports
    /// of it that no tsconfig or workspace links in summary.unlinkedPathImports. `issues` findings are ranked hypotheses with
    /// evidence and verify commands; `impact` rows carry depth, risk, confidence and typesOnly,
    /// and the summary lists affected entrypoints and tests to run.
    #[command(verbatim_doc_comment)]
    Query(Box<QueryArgs>),
}

#[derive(Args)]
pub(super) struct IngestArgs {
    /// Directory to scan (default: current directory).
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Workspace whose .octocode/graph receives the snapshot.
    #[arg(long)]
    workspace: Option<PathBuf>,
    /// Extra directory names to prune (repeatable or comma-separated), on top of
    /// the default pruned directories.
    #[arg(long, value_delimiter = ',')]
    exclude: Vec<String>,
    /// Maximum files to parse (default 50000).
    #[arg(long)]
    max_files: Option<u32>,
    /// Snapshots of the same root to keep, newest first (default 3).
    #[arg(long)]
    keep: Option<usize>,
    /// Rebuild even when the latest snapshot of this root is current (by default an
    /// unchanged tree reuses it and prints `reused: true`).
    #[arg(long)]
    force: bool,
    /// AST parser for files matching a root-relative glob, as LANG=GLOB (repeatable),
    /// e.g. cpp='include/**/*.h'.
    #[arg(long = "language-glob", value_name = "LANG=GLOB", value_parser = language_glob)]
    language_glob: Vec<(String, String)>,
}

fn language_glob(value: &str) -> Result<(String, String), String> {
    match value.split_once('=') {
        Some((language, glob)) if !language.is_empty() && !glob.is_empty() => {
            Ok((language.to_owned(), glob.to_owned()))
        }
        _ => Err(format!("expected LANG=GLOB, got {value:?}")),
    }
}

#[derive(Args)]
pub(super) struct QueryArgs {
    #[arg(value_parser = clap::builder::PossibleValuesParser::new(OPS))]
    op: String,
    /// Node reference or search text.
    target: Option<String>,
    /// Destination reference for `path`.
    to: Option<String>,
    /// Snapshot directory, graph.bin path, snapshot id, scope, or a substring of exactly one id
    /// (a scope or substring naming several snapshots exits 2 and lists them).
    #[arg(long)]
    graph: Option<String>,
    /// Workspace whose .octocode/graph holds the snapshots.
    #[arg(long)]
    workspace: Option<PathBuf>,
    /// Edge kinds to follow: contains, imports, uses, calls, inherits (comma-separated).
    #[arg(long, value_delimiter = ',')]
    edge: Vec<String>,
    /// Node kind filter: file, symbol, or package.
    #[arg(long)]
    kind: Option<String>,
    /// Traversal direction: out, in, or both.
    #[arg(long)]
    direction: Option<String>,
    /// Traversal depth (1-20).
    #[arg(long)]
    depth: Option<u32>,
    /// Weakest call-edge confidence to follow: high, medium, or low (default low).
    #[arg(long)]
    confidence: Option<String>,
    /// Rows per page (default 50, max 1000).
    #[arg(long)]
    limit: Option<usize>,
    /// Rows to skip (copy from `next`).
    #[arg(long)]
    offset: Option<usize>,
    /// Page one evidence array from a graph summary (copy from `nextLists`).
    #[arg(long)]
    list: Option<String>,
    /// `issues`: detectors to run (comma-separated; default all).
    #[arg(long, value_delimiter = ',')]
    detector: Vec<String>,
    /// `issues`: entrypoints in place of the inferred ones (comma-separated files).
    #[arg(long, value_delimiter = ',')]
    entry: Vec<String>,
    /// `issues`: snapshot (id, substring, or dir) to diff findings against;
    /// `drift`: the snapshot to diff the graph against.
    #[arg(long)]
    baseline: Option<String>,
    /// `issues`: drop findings scoring below this (0–1).
    #[arg(long)]
    min_score: Option<f64>,
    /// `issues`: lowest certainty tier to show: 100 (nothing references it), 90 (no graph or
    /// code reference; the default; bare prose words are listed in evidence.textMentions), 60
    /// (the file path, or every unused export, is referenced in code or qualified text such as
    /// `mod.run`, listed in evidence.mentionedIn, where some exports alone keep 90; or no mention
    /// scan ran).
    #[arg(long)]
    min_tier: Option<u8>,
    /// `impact`: more changed references (files or symbols, comma-separated).
    #[arg(long, value_delimiter = ',')]
    changed: Vec<String>,
    /// `impact`: git revision; files changed since it (plus untracked) are the change set.
    #[arg(long)]
    since: Option<String>,
}

pub(super) fn graph(runtime: &ToolRuntime, command: GraphCommand, mode: super::OutputMode) -> u8 {
    let output = match command {
        GraphCommand::Ingest(args) => {
            let IngestArgs {
                path,
                workspace,
                exclude,
                max_files,
                keep,
                force,
                language_glob,
            } = *args;
            let mut language_globs = std::collections::BTreeMap::<String, Vec<String>>::new();
            for (language, glob) in language_glob {
                language_globs.entry(language).or_default().push(glob);
            }
            let options = IngestOptions {
                path: std::path::absolute(&path).unwrap_or(path),
                workspace,
                exclude_dir: exclude,
                max_files,
                keep,
                force,
                language_globs,
            };
            runtime.graph_ingest(&options)
        }
        GraphCommand::Query(args) => {
            let QueryArgs {
                op,
                target,
                to,
                graph,
                workspace,
                edge,
                kind,
                direction,
                depth,
                confidence,
                limit,
                offset,
                detector,
                entry,
                baseline,
                min_score,
                changed,
                since,
                min_tier,
                list,
            } = *args;
            let options = QueryOptions {
                check_freshness: false,
                op,
                target,
                to,
                graph,
                workspace,
                edges: edge,
                kind,
                direction,
                depth,
                confidence,
                limit,
                offset,
                detectors: detector,
                baseline,
                min_score,
                changed,
                since,
                min_tier,
                list,
                entries: entry,
            };
            runtime.graph_query(&options)
        }
    };
    let error = output.value["error"].as_str().map(|error| {
        let code = output.value["errorCode"].as_str().unwrap_or_default();
        format!("{error} (errorCode: {code})")
    });
    // A graph answer prints as JSON in both modes; a text-mode error
    // prints its message and choices on stderr instead.
    let written = match error {
        Some(error) if !mode.json => {
            eprintln!("{error}");
            eprint!("{}", error_choices(&output.value));
            0
        }
        error => {
            if let Some(error) = error.filter(|_| mode.tells_stderr()) {
                eprintln!("{error}");
            }
            super::write_json(&output.value)
        }
    };
    match written {
        0 => super::exit_code(output.class),
        failed => failed,
    }
}

/// The choices a text-mode error offers, one per line under the message:
/// ambiguous candidates (a node id with its kind and line, or a snapshot
/// id), near-miss suggestions, and the command listing the rest.
fn error_choices(value: &serde_json::Value) -> String {
    let mut out = String::new();
    let rows = ["candidates", "suggestions"]
        .iter()
        .filter_map(|field| value[*field].as_array())
        .flatten();
    for row in rows {
        let line = match row {
            serde_json::Value::String(id) => id.clone(),
            row => {
                let id = row["id"].as_str().unwrap_or_default();
                match (row["symbolKind"].as_str(), row["line"].as_u64()) {
                    (Some(kind), Some(line)) => format!("{id}  ({kind}, line {line})"),
                    _ => format!("{id}  ({})", row["kind"].as_str().unwrap_or_default()),
                }
            }
        };
        out.push_str(&format!("  {line}\n"));
    }
    if let (Some(total), Some(next)) = (value["total"].as_u64(), value["next"].as_str()) {
        out.push_str(&format!("  ... {total} in all: {next}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::error_choices;
    use serde_json::json;

    #[test]
    fn text_errors_list_their_candidates_and_the_rest() {
        let ambiguous = json!({
            "error": "3 nodes match \"new\"; pass one of the candidate ids",
            "candidates": [
                {"id": "src/a.rs#new", "kind": "symbol", "symbolKind": "function", "line": 4},
                {"id": "src/b.rs", "kind": "file"}
            ],
            "total": 3,
            "truncated": true,
            "next": "octocode graph query find new --graph g1"
        });
        assert_eq!(
            error_choices(&ambiguous),
            "  src/a.rs#new  (function, line 4)\n  src/b.rs  (file)\n  ... 3 in all: octocode graph query find new --graph g1\n"
        );
        let graphs = json!({"error": "2 graphs match", "candidates": ["g1", "g2"]});
        assert_eq!(error_choices(&graphs), "  g1\n  g2\n");
        assert_eq!(error_choices(&json!({"error": "bad"})), "");
    }
}
