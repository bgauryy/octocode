// The saved graph model this package owns: a section's data.json (one chunk's graph snapshot), the stitched
// data.json over every chunk, the event stream the viewer and events.jsonl carry, and the run options.
// Native tool rows (graphBuild, graphQuery and the research tools) come from the generated contract
// (../contract.generated.d.ts); these types describe what this package derives and saves from them.
import type { Load } from '../loads.ts';
import type { BytesEvidence, BytesMethod } from './package-bytes.ts';
import type { Relation } from '../relations.ts';

/** Graph binding confidence, as graphQuery states it. */
export type Confidence = 'high' | 'medium' | 'low';
/** Who backs a call: the static graph, the language server, or both. */
export type CallVia = 'graph' | 'lsp' | 'both';

/** Where a query row points: a root-relative path (absolute: `section` + path, ids.ts absOf) and lines. */
export interface Loc {
  path: string;
  line?: number | null;
  endLine?: number;
  column?: number;
  lines?: number[];
}

export interface FileMetrics {
  pagerank: number;
  betweenness: number;
  inWeight: number;
  outWeight: number;
  depth: number;
  core: number;
  community: number;
  cycle: number | null;
}

export type FileCheckStatus =
  'ok' | 'stale' | 'lines' | 'missing' | 'unverified';

export interface FileCheck {
  status: FileCheckStatus;
  lines?: number;
  bytes?: number;
  problems: string[];
}

/** A file node (or a workspace package node, `kind: 'package'`). Ids are root-relative paths or `pkg:<name>`. */
export interface FileNode {
  id: string;
  label: string;
  group: string;
  language?: string;
  roles?: string[];
  fanIn: number;
  fanOut: number;
  kind?: 'package';
  chunk?: string;
  manifest?: string;
  metrics?: FileMetrics;
  check?: FileCheck;
}

/** One folded edge per (source, target): every witness line, occurrences and the strongest confidence. */
export interface GraphEdge {
  source: string;
  target: string;
  line: number | null;
  lines: number[];
  count: number;
  confidence: Confidence;
  resolution: string[];
  /** Every witness is test code. Absent on an edge only the language server reported from production code. */
  test?: boolean;
  crossChunk?: boolean;
  /** Imports: the lines of each kind (`{ "type-import": [3], "dynamic-import": [40] }`), so an answer cites the import
   * that loads; absent in builds before it. */
  kindLines?: Record<string, number[]>;
  /** Imports: the load the engine states for each kind (`{ "type-import": "type" }`); loads.ts reads it before its
   * kind table. Absent in builds before the engine stated loads. */
  loads?: Record<string, Load>;
}

export type ImportEdge = GraphEdge;

export interface CallEdge extends GraphEdge {
  via: CallVia;
}

/** A declaration row of a section's snapshot; ids are `file#name` (with an `@line` suffix when names repeat). */
export interface Declaration {
  id: string;
  file: string;
  name: string;
  kind: string;
  line?: number;
  endLine?: number;
  exported: boolean;
  /** The parameter list and return type as the declaration writes them (graphQuery `params`, `returns`). */
  params?: string;
  returns?: string;
}

/** A symbol in the call graph: a declaration or a call endpoint, with its outgoing call count. */
export interface SymbolNode extends Declaration {
  calls: number;
  /** The language server answered this function's callees probe in full: a refresh carries the answer over while
   * neither its file nor a file it calls into changed (lsp.ts carriedProbes). */
  probed?: true;
}

export interface ExternalPackage {
  id: string;
  count: number;
  files: string[];
  testFiles?: string[];
  /** Per importing file: the kinds its imports of the package record (loads.ts reads when it loads). */
  kinds?: Record<string, string[]>;
  /** Per importing file: the lines of its imports of the package. */
  lines?: Record<string, number[]>;
  /** The load the engine states for each kind of these imports; read before the kind table. */
  loads?: Record<string, Load>;
  /** What the package costs as a build measured it (package-bytes.ts `PackageSize`); null when it could not. */
  bytes?: number | null;
  bytesMethod?: BytesMethod | null;
  bytesEvidence?: BytesEvidence;
}

/**
 * What a file takes from a file it imports: the declaration a name resolves to (`file#name`; a file for a namespace
 * import) and how: `import` (declared in the imported file), `reexport` (through a re-export of it), `namespace`,
 * `path` (a Rust value path), `reexported-name` (a name the source re-exports).
 */
export interface UseEdge {
  source: string;
  target: string;
  line: number | null;
  detail: string;
  test?: boolean;
}

/** An import the section's snapshot could not link inside its root. */
export interface OutsideImport {
  file: string;
  line?: number;
  test?: boolean;
  specifier: string | null;
  message: string;
  /** The import's kind (static-import, type-import, dynamic-import, …) when the engine states it. */
  importKind?: string;
  /** When that import takes effect, as the engine states it. */
  load?: Load;
}

/** A call site the section left unbound (graphQuery `unresolved`), kept for stitching between chunks. */
export interface UnresolvedCall {
  from: string;
  file: string;
  targetName: string;
  line: number;
  reason: string;
  /** The receiver's type path as the source spells it (the engine's `receiverType`), for a call on a typed receiver. */
  receiverType?: string;
}

export interface Partial {
  isPartial: boolean;
  partialReasons: string[];
}

/** One package of the enclosing workspace the build imported from outside its root. */
export interface WorkspaceExternal {
  /** The graph's package node, `pkg:<name>`, when the engine made one. */
  id?: string;
  /** The name as the graph's package node spells it, else the member's own name. */
  name: string;
  /** The member directories it names, relative to the workspace root. */
  paths: string[];
  /** Imports of the build into it: package-node import edges plus unresolved out-of-root import lines. */
  imports: number;
  /** Files of the build importing it. */
  files: number;
}

/** A workspace enclosing the build root whose packages the build imported as externals. */
export interface WorkspaceExternals {
  root: string;
  /** `package`: `root` is the build's own package, holding the build root; its imports into the rest of it. */
  scope?: 'package';
  /** What declares the workspace: `package.json workspaces`, `pnpm-workspace.yaml`, `Cargo.toml [workspace]`, `go.work`. */
  manifests: string[];
  packages: WorkspaceExternal[];
  /** The build of the workspace root that stitches them. */
  command: string;
}

export interface Coverage extends Partial {
  files: number;
  imports: number;
  calls: number;
  isolated?: number;
  callsOutside?: number;
  outsideRoot?: number;
  declarations?: number;
  symbolsInCalls?: number;
  chunks?: number;
  built?: number;
  notBuilt?: string[];
  partialChunks?: { id: string; path: string; partialReasons: string[] }[];
  crossChunkImports?: number;
  crossChunkCalls?: number;
  note?: string;
  /** Workspaces enclosing the build root whose packages the build imported as externals (workspace-scope.ts). */
  workspaceExternals?: WorkspaceExternals[];
}

export interface Snapshot {
  isPartial: boolean;
  /** The engine's reasons; absent when it states none. */
  partialReasons?: string[];
  id: string;
  root: string;
  queryRoot?: string;
  counts?: {
    nodes?: number;
    edges?: number;
    nodesByKind?: Record<string, number>;
    edgesByKind?: Record<string, number>;
  };
  linkage?: number | null;
}

export interface CycleRow {
  size: number;
  runtime?: boolean;
  nodes: string[];
  chunk?: string;
}

export interface Insights {
  hubs: { id: string; fanIn: number; fanOut: number }[];
  health: Record<string, unknown>;
  findings: Record<string, number>;
  issues: Record<string, unknown>[];
  cycles: {
    all: CycleRow[];
    total: number;
    runtime: number;
    largestRuntime: number;
    top: { size: number; runtime?: boolean; nodes: string[] }[];
  };
  groups: { from: string; to: string; count: number }[];
  mutualGroups: [string, string][];
}

/** One chunk of the plan: a package directory or the loose files of a directory. */
export interface Chunk {
  id: string;
  path: string;
  files: number;
  loose?: boolean;
  exclude?: string[];
  package?: string;
  error?: string;
}

/** A chunk as data.json records it. */
export interface ChunkRow extends Chunk {
  built: boolean;
}

/** A section's data.json: one graph snapshot (a chunk, or the whole root). */
export interface SectionData {
  section: string;
  evidence: string;
  groupWalks?: Record<
    string,
    { groups: number; grouped: boolean; rows: number; total: number }
  >;
  coverage: Coverage;
  frozen: boolean;
  ms?: { ingest: number; total: number };
  snapshot: Snapshot;
  files: FileNode[];
  imports: ImportEdge[];
  symbols: SymbolNode[];
  calls: CallEdge[];
  fnFiles: string[];
  insights: Insights;
  externals: ExternalPackage[];
  outsideRoot: OutsideImport[];
  declarations: Declaration[];
  unresolvedCalls: UnresolvedCall[];
  /** Names each file takes from the files it imports; absent in builds before it. */
  uses?: UseEdge[];
}

/** Weighted file edges (graph-model.ts `fileEdges`), as data.json stores them. */
export interface WeightedEdge {
  source: string;
  target: string;
  weight: number;
  imports: number;
  calls: number;
  test: boolean;
  /** An import or a full-evidence call backs it; absent in exports written before strong edges (read as strong). */
  strong?: boolean;
  /** When the pair's strongest import takes effect (loads.ts); null when only calls join it, absent in older exports. */
  load?: Load | null;
}

export interface CycleSet {
  id: number;
  size: number;
  weight: number;
  languages: string[];
  chunks: string[];
  members: string[];
}

export interface Community {
  id: number;
  label: string;
  size: number;
  internal: number;
  external: number;
  chunk: string;
  purity: number;
  members: string[];
}

export interface PackageCoupling {
  chunk: string;
  files: number;
  ca: number;
  ce: number;
  instability: number | null;
  inWeight: number;
  outWeight: number;
}

export interface GraphMetrics {
  weightRule: string;
  stats: {
    nodes: number;
    edges: number;
    weight: number;
    modularity: number;
    cycles: number;
    largestCycle: number;
    maxDepth: number;
    maxCore: number;
    communities: number;
    isolated: number;
    pagerankIterations: number;
  };
  cycles: CycleSet[];
  communities: Community[];
  packages: PackageCoupling[];
  timings: Record<string, number>;
}

export interface UnansweredProbe {
  id: string;
  file: string;
  symbol: string;
  line?: number;
  reasons: string[];
  ambiguousDefinitions?: {
    file: string;
    name: string;
    line: number;
    sites: unknown[];
  }[];
}

/** How calls were confirmed: `lsp` is the phase state (pending, off, ok, partial, …). */
export interface CallStats {
  /** `both` once the LSP pass finished; absent while it runs or when it had nothing to probe. */
  mode?: 'graph' | 'both';
  lsp: string;
  probed?: number;
  resolved?: number;
  partialWithMatches?: number;
  /** Answered probes whose answer omitted locations the read policy withheld (never graph files). */
  withheld?: number;
  both?: number;
  graphOnly?: number;
  lspOnly?: number;
  /** Sources selected only for a receiver call no rule bound (lsp.ts receiverCallSources), counted in `total` too. */
  receiverSources?: number;
  /** Sources a refresh did not probe again: their previous answers were carried over (lsp.ts carriedProbes). */
  carried?: number;
  /** Sources whose answers came from the shared LSP answer cache (graph-cache.ts). */
  cached?: number;
  total?: number;
  scope?: string;
  unanswered?: UnansweredProbe[];
}

export interface Verification {
  checked: number;
  ok: number;
  stale: number;
  lines: number;
  missing: number;
  unverified: number;
  staleCheck: string;
}

/** Provider relations attached by `octocode-graph enrich` (enrich.ts). */
export interface Enrichment {
  revision: string;
  baseSha256: string;
  kinds: string[];
  providers: Record<string, unknown>[];
  relations: Relation[];
}

/** The stitched data.json (and stitched.json, its checkpoint before LSP, checks and final metrics). */
export interface GraphData extends Omit<
  SectionData,
  'declarations' | 'unresolvedCalls' | 'outsideRoot'
> {
  mode?: 'whole';
  hubFiles?: string[];
  chunks?: ChunkRow[];
  uncoveredFiles?: string[];
  legends?: Record<string, string[]>;
  callStats?: CallStats;
  tokens?: { sourceBytes: number; sourceFiles: number };
  fileEdges?: WeightedEdge[];
  /** Docs → files and symbols (links-docs.ts); never part of the code metrics. */
  links?: Link[];
  /** References that named no single target, with why. */
  unresolvedLinks?: UnresolvedLink[];
  metrics?: GraphMetrics;
  verification?: Verification;
  enrichment?: Enrichment;
  model?: unknown;
  outsideRoot?: OutsideImport[];
}

/** A GraphData after `locate` and `computeMetrics`: every node located and measured. */
export type MeasuredGraph = GraphData & {
  fileEdges: WeightedEdge[];
  metrics: GraphMetrics;
};

/** Build options: concurrency, LSP scope and the section selection. */
export type LspMode = 'smart' | 'all' | 'off';

export interface RunOptions {
  jobs: number;
  ingestJobs: number;
  lsp: LspMode;
  whole: boolean;
  exclude: string[];
  /** A single source file, relative to the root, when the input was a file. */
  file?: string;
}

/**
 * One event of a run. Every event the pipeline, the server and the viewer exchange has a `type`; the log
 * adds `seq` and `t` (ms since the run started). Payloads are typed where they are produced.
 */
export interface RunEvent {
  type: string;
  seq?: number;
  t?: number;
  [field: string]: unknown;
}

export type Emit = (event: RunEvent) => void;

/** How a link was found (links-docs.ts for the Markdown rules). */
export type LinkRule =
  | 'md-link'
  | 'md-path'
  | 'md-cite'
  | 'md-symbol'
  | 'manifest'
  | 'tsconfig'
  | 'contract'
  | 'md-contract'
  | 'agent';

/** An edge with a non-code endpoint, witnessed by one line of its source file. */
export interface Link {
  /** The citing file and the line that witnesses the link. */
  source: string;
  line: number;
  /** A file, a directory (ending in `/`), or a symbol id (`file#name`). */
  target: string;
  /** A citation's cited lines. */
  lines?: [number, number];
  /** `references` and `cites` from docs; `documents`, `configures` and `relates` from agents (links-proposed.ts). */
  relation: 'references' | 'cites' | 'documents' | 'configures' | 'relates';
  rule: LinkRule;
  /** `stale` when the cited span changed while the citing line did not (a refresh compares the hashes). */
  check: 'ok' | 'stale';
  /** Hash of the cited span (citations only). */
  hash?: string;
  /** Hash of the citing line. */
  sourceHash?: string;
  /** An agent's link: its id, how sure the checks are, who proposed it, what it claims, and a missing file. */
  id?: string;
  confidence?: 'candidate' | 'likely' | 'confirmed';
  proposedBy?: string;
  claim?: string;
  missing?: boolean;
}

/** A reference that named no single target: why, and the candidates it could mean. */
export interface UnresolvedLink {
  source: string;
  line: number;
  text: string;
  rule: LinkRule;
  /** `unconfirmed`: a code-shaped name the doc does not place (its section names no declaring file); `build-output`: a manifest path into a build output directory that is not on disk (candidates: sources of its name). */
  reason:
    | 'no-such-file'
    | 'no-such-anchor'
    | 'line-out-of-range'
    | 'ambiguous'
    | 'unconfirmed'
    | 'build-output';
  candidates?: string[];
}

/** A refresh's `refreshed` event: the chunks it rebuilt, the files changed, added and removed, and its time. */
export interface RefreshedPayload {
  ms: number;
  chunks: string[];
  changed?: string[];
  added?: string[];
  removed?: string[];
}
