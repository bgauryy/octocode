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
  octocode graph query stale               files changed since ingest -> re-ingest

NODE IDS
  src/a.ts               file (root-relative; absolute paths are accepted too)
  src/a.ts#Class.method  symbol (an @<line> suffix appears only when a name repeats)
  pkg:react              external package
  A bare name or Class.method also resolves; ambiguity exits 2 and lists candidates.

EVIDENCE
  Syntax-derived. Imports are module-resolved (tsconfig paths, workspaces, cargo, go.mod, python
  roots). Calls carry via (local|import|namespace|unique-name) and confidence (high|medium|low).
  Call answers carry coverage.callInternalRecall (below 0.9 they add a warning: a lower bound).
  issues findings carry a tier (100/90/60); the default view hides tier 60 (text-referenced).
  Treat results as leads: confirm identity with lspSearch before deleting or renaming.

OUTPUT
  One JSON object on stdout (--pretty for humans). Lists carry total and results; a cut adds
  truncated plus next, a paste-ready command. Exit: 0 ok, 1 empty, 2 bad input or ambiguous,
  3 no graph or node, 5 error, 6 more pages (run next).

STORAGE
  <workspace>/.octocode/graph/<UTC time>-<scope>/{graph.bin, manifest.json}; latest names the
  newest. The workspace is the nearest .git ancestor of the current directory (or --workspace).
  Files over 1 MB stay as unparsed nodes; minified bundles keep their imports but no symbols.";

#[derive(Subcommand)]
pub(super) enum GraphCommand {
    /// Parse a directory and publish a snapshot (graph.bin + manifest.json).
    ///
    /// Writes <workspace>/.octocode/graph/<UTC time>-<scope>/ and updates `latest`; keeps the
    /// newest 3 snapshots per scope (--keep). Honors .gitignore; prunes node_modules, dist,
    /// build, out, coverage, target, .next, .cache, venv, __pycache__ and hidden directories.
    /// Prints a receipt: counts, call-link rate, scan gaps, and a `next` command.
    #[command(verbatim_doc_comment)]
    Ingest(Box<IngestArgs>),
    /// Answer one bounded question from a snapshot (default: the workspace's latest).
    ///
    /// LOOKUP     stats · find <text> · node <ref> · symbols <file|symbol>
    /// NEIGHBORS  deps <ref> · dependents <ref> · callers <ref> · callees <ref>
    /// TRAVERSE   path <from> <to> · walk <ref> [--direction out|in|both] [--edge ..] [--depth n]
    /// STRUCTURE  cycles · hubs [--direction in|out] · diagnostics [code|path] · stale
    /// ANALYSIS   issues [--detector a,b] [--min-score x] [--baseline <snapshot>]
    ///            impact <ref> | --changed a,b | --since <git-rev>
    ///
    /// deps/dependents follow imports for files and calls for symbols; override with --edge
    /// (contains, imports, uses, calls, inherits). `issues` findings are ranked hypotheses with
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
    /// node_modules, dist, build, out, coverage, .git, target, .next, .cache.
    #[arg(long, value_delimiter = ',')]
    exclude: Vec<String>,
    /// Maximum files to parse (default 50000).
    #[arg(long)]
    max_files: Option<u32>,
    /// Snapshots of the same scope to keep, newest first (default 3).
    #[arg(long)]
    keep: Option<usize>,
    /// Rebuild even when the latest snapshot of this scope is current (by default an
    /// unchanged tree reuses it and prints `reused: true`).
    #[arg(long)]
    force: bool,
    /// Emit indented JSON.
    #[arg(long)]
    pretty: bool,
}

#[derive(Args)]
pub(super) struct QueryArgs {
    #[arg(value_parser = clap::builder::PossibleValuesParser::new(OPS))]
    op: String,
    /// Node reference or search text.
    target: Option<String>,
    /// Destination reference for `path`.
    to: Option<String>,
    /// Snapshot directory, graph.bin path, snapshot id, or id substring.
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
    /// `issues`: detectors to run (comma-separated; default all).
    #[arg(long, value_delimiter = ',')]
    detector: Vec<String>,
    /// `issues`: snapshot (id, substring, or dir) to diff findings against.
    #[arg(long)]
    baseline: Option<String>,
    /// `issues`: drop findings scoring below this (0–1).
    #[arg(long)]
    min_score: Option<f64>,
    /// `issues`: lowest certainty tier to show: 100 (nothing references it), 90 (no graph or
    /// text reference; the default), 60 (referenced only by text, e.g. a string path).
    #[arg(long)]
    min_tier: Option<u8>,
    /// `impact`: more changed references (files or symbols, comma-separated).
    #[arg(long, value_delimiter = ',')]
    changed: Vec<String>,
    /// `impact`: git revision; files changed since it (plus untracked) are the change set.
    #[arg(long)]
    since: Option<String>,
    /// Emit indented JSON.
    #[arg(long)]
    pretty: bool,
}

pub(super) fn graph(runtime: &ToolRuntime, command: GraphCommand) -> u8 {
    let (output, pretty) = match command {
        GraphCommand::Ingest(args) => {
            let IngestArgs {
                path,
                workspace,
                exclude,
                max_files,
                keep,
                force,
                pretty,
            } = *args;
            let options = IngestOptions {
                path: std::path::absolute(&path).unwrap_or(path),
                workspace,
                exclude_dir: exclude,
                max_files,
                keep,
                force,
            };
            (runtime.graph_ingest(&options), pretty)
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
                baseline,
                min_score,
                changed,
                since,
                min_tier,
                pretty,
            } = *args;
            let options = QueryOptions {
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
            };
            (runtime.graph_query(&options), pretty)
        }
    };
    match super::write_json(&output.value, !pretty) {
        0 => output.exit,
        failed => failed,
    }
}
