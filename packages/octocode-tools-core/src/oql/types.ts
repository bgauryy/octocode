/**
 * Octocode Query Language (OQL) — canonical type definitions.
 *
 * These mirror the contract in
 * docs/octocode-language/OCTOCODE_QUERY_LANGUAGE.md. OQL is a typed research
 * query object that compiles into existing Octocode tool runners; it is not a
 * raw string DSL. Schemas/descriptions co-locate here for now (tools-core) and
 * may migrate to `@octocodeai/octocode-core/oql` once a second consumer needs
 * OQL validation without the rest of tools-core.
 */

// Active OQL targets.
export type OqlActiveTarget =
  | 'code'
  | 'content'
  | 'structure'
  | 'files'
  | 'semantics'
  | 'repositories'
  | 'packages'
  | 'pullRequests'
  | 'commits'
  | 'artifacts'
  | 'diff'
  | 'research'
  | 'graph'
  // Addressable materialization: clone/cache a bounded corpus and return a
  // stable local checkpoint (not a side-effect of a search).
  | 'materialize';

// Reserved capabilities need proof/dry-run engines before they become targets.
export type OqlReservedTarget = 'fixes' | 'dataflow';

export type OqlTarget = OqlActiveTarget | OqlReservedTarget;

export const ACTIVE_TARGETS: readonly OqlActiveTarget[] = [
  'code',
  'content',
  'structure',
  'files',
  'semantics',
  'repositories',
  'packages',
  'pullRequests',
  'commits',
  'artifacts',
  'diff',
  'research',
  'graph',
  'materialize',
];

export const RESERVED_TARGETS: readonly OqlReservedTarget[] = [
  'fixes',
  'dataflow',
];

/** Targets that do not need a code corpus (provider/registry discovery). */
export const CORPUS_OPTIONAL_TARGETS: readonly OqlActiveTarget[] = [
  'packages',
  'repositories',
];

/**
 * Which `controls.search.sort` values each lane can actually execute
 * (files: localFindFiles sortBy; code: code-search ranking sorts). Single
 * source for shorthand lowering and the planner's inapplicable-sort warning —
 * a value outside the target's set is IGNORED by the backend, never an error.
 */
export const SEARCH_SORTS_BY_TARGET = {
  code: ['relevance', 'matchCount', 'path', 'modified', 'accessed', 'created'],
  files: ['size', 'name', 'path', 'modified'],
} as const;

export type PredicateId = string;

export type QuerySource =
  | { kind: 'local'; path: string }
  | { kind: 'github'; repo?: string; owner?: string; ref?: string }
  | { kind: 'materialized'; localPath: string; source?: QuerySource }
  | { kind: 'npm' };

export interface QueryScope {
  path?: string | string[];
  language?: string | string[];
  include?: string[];
  exclude?: string[];
  excludeDir?: string[];
  hidden?: boolean;
  noIgnore?: boolean;
  minDepth?: number;
  maxDepth?: number;
}

export interface TextPredicate {
  id?: PredicateId;
  kind: 'text';
  value: string;
  case?: 'smart' | 'sensitive' | 'insensitive';
  wholeWord?: boolean;
}

export interface RegexPredicate {
  id?: PredicateId;
  kind: 'regex';
  value: string;
  dialect?: 'rust' | 'pcre2' | 'provider';
  case?: 'smart' | 'sensitive' | 'insensitive';
  wholeWord?: boolean;
  multiline?: boolean;
  dotAll?: boolean;
}

export interface StructuralRule {
  pattern?: string;
  kind?: string;
  inside?: StructuralRule;
  has?: StructuralRule;
  not?: StructuralRule;
  all?: StructuralRule[];
  any?: StructuralRule[];
  stopBy?: 'end';
}

export type StructuralRuleInput = StructuralRule | string;

export interface StructuralPredicate {
  id?: PredicateId;
  kind: 'structural';
  lang: string;
  pattern?: string;
  rule?: StructuralRuleInput;
}

export type FieldName =
  | 'path'
  | 'basename'
  | 'extension'
  | 'size'
  | 'modified'
  | 'accessed'
  | 'empty'
  | 'permissions'
  | 'executable'
  | 'readable'
  | 'writable'
  | 'entryType';

export type FieldOp =
  | '='
  | '!='
  | 'in'
  | 'exists'
  | 'glob'
  | 'regex'
  | '>'
  | '>='
  | '<'
  | '<='
  | 'within'
  | 'before';

export interface FieldPredicate {
  id?: PredicateId;
  kind: 'field';
  field: FieldName;
  op: FieldOp;
  value?: unknown;
}

export interface AllPredicate {
  kind: 'all';
  id?: PredicateId;
  of: Predicate[];
}
export interface AnyPredicate {
  kind: 'any';
  id?: PredicateId;
  of: Predicate[];
}
export interface NotPredicate {
  kind: 'not';
  id?: PredicateId;
  predicate: Predicate;
}

export type LeafPredicate =
  | TextPredicate
  | RegexPredicate
  | StructuralPredicate
  | FieldPredicate;

export type Predicate =
  | AllPredicate
  | AnyPredicate
  | NotPredicate
  | LeafPredicate;

export interface MaterializePolicy {
  mode: 'never' | 'auto' | 'required';
  strategy?: 'file' | 'tree' | 'subtree' | 'repo';
  allowFullRepo?: boolean;
  forceRefresh?: boolean;
}

export interface FetchInstructions {
  content?: {
    range?: { startLine?: number; endLine?: number; contextLines?: number };
    match?: { text: string; regex?: boolean; caseSensitive?: boolean };
    contentView?: 'exact' | 'compact' | 'symbols';
    charOffset?: number;
    charLength?: number;
    fullContent?: boolean;
  };
  tree?: {
    maxDepth?: number;
    pattern?: string;
    includeSizes?: boolean;
    extensions?: string[];
    filesOnly?: boolean;
    directoriesOnly?: boolean;
    sortBy?: 'name' | 'size' | 'time' | 'extension';
    reverse?: boolean;
  };
}

export interface QueryControls {
  search?: {
    countLinesPerFile?: boolean;
    countMatchesPerFile?: boolean;
    onlyMatching?: boolean;
    unique?: boolean;
    countUnique?: boolean;
    contextLines?: number;
    invertMatch?: boolean;
    matchWindow?: number;
    matchContentLength?: number;
    maxMatchesPerFile?: number;
    matchPage?: number;
    // 'size' and 'name' apply to target:"files" only (lowered to
    // localFindFiles sortBy); the rest are code-search sorts.
    sort?:
      | 'relevance'
      | 'matchCount'
      | 'path'
      | 'modified'
      | 'accessed'
      | 'created'
      | 'size'
      | 'name';
    sortReverse?: boolean;
    rankingProfile?: string;
    debugRanking?: boolean;
  };
  budget?: {
    maxFiles?: number;
    maxCandidates?: number;
    maxBytes?: number;
    maxMaterializedBytes?: number;
    maxPlanNodes?: number;
    maxBooleanExpansion?: number;
    timeoutMs?: number;
  };
}

export type SelectField = string;

export type QueryView = 'discovery' | 'paginated' | 'detailed';

export interface OqlQuery {
  schema: 'oql';
  id?: string;
  target: OqlActiveTarget;
  from?: QuerySource;
  scope?: QueryScope;
  where?: Predicate;
  materialize?: MaterializePolicy;
  fetch?: FetchInstructions;
  select?: SelectField[];
  view?: QueryView;
  controls?: QueryControls;
  limit?: number;
  page?: number;
  itemsPerPage?: number;
  /**
   * Target-specific parameter bag. The backing tool's schema remains the
   * exhaustive validator; OQL validates the documented common fields early.
   */
  params?: Record<string, unknown>;
  explain?: boolean;
}

export interface OqlBatch {
  schema: 'oql';
  id?: string;
  queries: OqlQuery[];
  combine?: 'independent' | 'merge';
  limit?: number;
  page?: number;
  itemsPerPage?: number;
  explain?: boolean;
}

export type OqlCanonicalInput = OqlQuery | OqlBatch;

export interface OqlInputQuery {
  schema?: 'oql';
  id?: string;
  target: OqlTarget;
  from?: QuerySource;
  scope?: QueryScope;
  where?: Predicate;
  materialize?: MaterializePolicy | 'never' | 'auto' | 'required';
  fetch?: FetchInstructions;
  select?: SelectField[];
  view?: QueryView;
  controls?: QueryControls;
  limit?: number;
  page?: number;
  itemsPerPage?: number;
  explain?: boolean;
  // sugar accepted only at the edge
  repo?: string;
  owner?: string;
  ref?: string;
  path?: string | string[];
  text?: string;
  regex?: string;
  pattern?: string;
  rule?: StructuralRule;
  lang?: string;
  [key: string]: unknown;
}

export interface OqlInputBatch {
  schema?: 'oql';
  id?: string;
  queries: OqlInputQuery[];
  combine?: 'independent' | 'merge';
  limit?: number;
  page?: number;
  itemsPerPage?: number;
  explain?: boolean;
}

export type OqlSearchInput = OqlInputQuery | OqlInputBatch;

/* ----------------------------- diagnostics ------------------------------ */

export type DiagnosticCode =
  | 'invalidQuery'
  | 'ambiguousSugar'
  | 'unknownField'
  | 'unsupportedTarget'
  | 'unsupportedPredicate'
  | 'unsupportedBoolean'
  | 'unsupportedScope'
  | 'negativeUniverseRequired'
  | 'residualNotExact'
  | 'fieldTypeMismatch'
  | 'requiresMaterialization'
  | 'vendorNoEquivalent'
  | 'lossyTransform'
  | 'unsupportedVendorPredicate'
  | 'responseShapeMismatch'
  | 'materializationNotAllowed'
  | 'materializationFailed'
  | 'providerUnindexed'
  | 'providerSemanticsApproximate'
  | 'partialResult'
  | 'contentTruncated'
  | 'matchTruncated'
  | 'planTruncated'
  | 'budgetExhausted'
  | 'parserFailed'
  | 'partialParse'
  | 'signatureUnsupported'
  | 'lspUnavailable'
  | 'staleCache'
  | 'sanitized'
  | 'rateLimited'
  | 'authRequired'
  | 'zeroMatches'
  | 'symbolNotFound';

export interface OqlDiagnostic {
  code: DiagnosticCode;
  severity: 'info' | 'warning' | 'error';
  queryPath?: string;
  predicateId?: string;
  backend?: string;
  message: string;
  blocksAnswer: boolean;
  repair?: {
    message: string;
    suggestedQuery?: OqlSearchInput;
  };
  continuation?: OqlContinuation;
}

/* ------------------------------ planner --------------------------------- */

export type PlanRoute = 'PUSHDOWN' | 'RESIDUAL' | 'ROUTE' | 'UNSUPPORTED';

export interface OqlPlanNode {
  predicateId: PredicateId;
  path: string;
  route: PlanRoute;
  backend?: string;
  reason: string;
}

export interface OqlBackendCall {
  backend: string;
  source?: QuerySource;
  operation: string;
  exact: boolean;
}

export interface OqlTransformerTrace {
  id: string;
  status: string;
  sourceKinds: readonly string[];
  target: string;
  backends: readonly Pick<OqlBackendCall, 'backend' | 'operation' | 'exact'>[];
}

export interface OqlExplainPlan {
  input: unknown;
  normalized: OqlCanonicalInput;
  defaults: Record<string, unknown>;
  nodes: OqlPlanNode[];
  backendCalls: OqlBackendCall[];
  transformers?: readonly OqlTransformerTrace[];
  materialization?: MaterializePolicy & { required: boolean; reason: string };
  budgets: QueryControls['budget'];
  truncated?: boolean;
  diagnostics: OqlDiagnostic[];
  next?: Record<string, OqlContinuation>;
}

/* --------------------------- result envelope ---------------------------- */

export type OqlProofGrade =
  | 'candidate'
  | 'text'
  | 'structural'
  | 'semantic'
  | 'graph'
  | 'missing';

interface OqlProofGradedRow {
  /**
   * Mandatory on rows emitted by `runOqlSearch`; adapters may omit it while the
   * runner computes the final proof grade from query semantics and row shape.
   */
  proofGrade?: OqlProofGrade;
}

export interface OqlCodeResultRow extends OqlProofGradedRow {
  kind: 'code';
  source?: QuerySource;
  path: string;
  /**
   * 1-based match line. Optional because some providers (GitHub code search)
   * return path-level matches with no line — never fabricate one.
   */
  line?: number;
  endLine?: number;
  column?: number;
  snippet?: string;
  /** File-level count payloads from local search count modes. */
  totalMatchedLines?: number;
  totalOccurrences?: number;
  /** Provider snippet offsets when available, e.g. GitHub code search indices. */
  matchIndices?: Array<{ start: number; end: number; lineOffset?: number }>;
  /** Row-level provider/context metadata that is useful but not identity. */
  metadata?: Record<string, unknown>;
  /**
   * Structural (AST) metavariable captures. `$X` → single-element list; `$$$X`
   * → node list. Keyed by bare metavar name. Present only for structural
   * matches that captured; never fabricated.
   */
  metavars?: Record<string, string[]>;
  /** Per-capture source ranges (uri+line ready for lspGetSemantics handoff). */
  metavarRanges?: Record<
    string,
    {
      text: string;
      line: number;
      column: number;
      endLine: number;
      endColumn: number;
    }[]
  >;
  /**
   * Executable follow-up continuations. Keys are dotted *domain names*
   * (`next.<domain>`, e.g. `next.fetch`, `next.semantic`, `next.charRange`),
   * NOT nested object paths — the registry in run.ts owns the key set.
   */
  next?: Record<string, OqlContinuation>;
}

export interface OqlFileResultRow extends OqlProofGradedRow {
  kind: 'file';
  source?: QuerySource;
  path: string;
  entryType: 'file' | 'directory';
  size?: number;
  modified?: string;
  next?: Record<string, OqlContinuation>;
}

export interface OqlTreeResultRow extends OqlProofGradedRow {
  kind: 'tree';
  source?: QuerySource;
  path: string;
  entryType: 'file' | 'directory';
  depth: number;
  size?: number;
  children?: OqlTreeResultRow[];
  next?: Record<string, OqlContinuation>;
}

export interface OqlContentResultRow extends OqlProofGradedRow {
  kind: 'content';
  source?: QuerySource;
  path: string;
  content: string;
  range?: {
    startLine?: number;
    endLine?: number;
    charOffset?: number;
    charLength?: number;
  };
  contentView: 'exact' | 'compact' | 'symbols';
  next?: Record<string, OqlContinuation>;
}

/**
 * Generic record row for targets whose payload is a typed research object
 * (repository, package, PR, commit, symbol/location, artifact, diff, packet).
 * `recordType` names the family; `data` is the row payload.
 */
export interface OqlRecordResultRow extends OqlProofGradedRow {
  kind: 'record';
  recordType:
    | 'semantics'
    | 'repository'
    | 'package'
    | 'pullRequest'
    | 'commit'
    | 'artifact'
    | 'diff'
    | 'research'
    | 'graph'
    | 'materialized';
  /** Stable, citeable identity (repo, name@version, #PR, SHA, path, uri). */
  id?: string;
  source?: QuerySource;
  /** Parent/query metadata preserved from the backing tool payload. */
  metadata?: Record<string, unknown>;
  data: Record<string, unknown>;
  next?: Record<string, OqlContinuation>;
}

/* --- typed `data` contracts per recordType (documented payload shapes) ----
 * The backing tool owns the exhaustive payload; these name the fields agents
 * rely on to cite + continue. All optional (backend-dependent); never fabricated.
 */

export interface OqlRepositoryData {
  fullName?: string;
  owner?: string;
  repo?: string;
  description?: string;
  stars?: number;
  forks?: number;
  language?: string;
  topics?: string[];
  pushedAt?: string;
  url?: string;
  [k: string]: unknown;
}
export interface OqlPackageData {
  name?: string;
  version?: string;
  description?: string;
  downloads?: number;
  repository?: string;
  repositoryId?: string;
  [k: string]: unknown;
}
export interface OqlPullRequestData {
  number?: number;
  title?: string;
  state?: string;
  author?: string;
  createdAt?: string;
  mergedAt?: string;
  changedFiles?: number;
  url?: string;
  [k: string]: unknown;
}
export interface OqlCommitData {
  sha?: string;
  oid?: string;
  message?: string;
  title?: string;
  author?: string;
  date?: string;
  [k: string]: unknown;
}
export interface OqlArtifactData {
  mode?: string;
  format?: string;
  /** Set when extract/decompress/unpack produced a derived local path. */
  localPath?: string;
  entries?: unknown[];
  strings?: unknown[];
  symbols?: unknown[];
  nextScanOffset?: number;
  [k: string]: unknown;
}
export interface OqlDiffData {
  path?: string;
  baseRef?: string;
  headRef?: string;
  additions?: number;
  deletions?: number;
  unchanged?: number;
  patch?: string;
  [k: string]: unknown;
}
export interface OqlSemanticsData {
  uri?: string;
  line?: number;
  startLine?: number;
  symbol?: string;
  kind?: string;
  [k: string]: unknown;
}
export interface OqlMaterializedData {
  localPath: string;
  repoRoot?: string;
  ref?: string;
  cache?: 'hit' | 'miss';
  complete?: boolean;
  [k: string]: unknown;
}
export interface OqlResearchData {
  kind?: 'researchFlow';
  goal?: string;
  intent?: string;
  facets?: readonly string[];
  mode?: 'plan' | 'analyze' | 'prove';
  summary?: Record<string, unknown>;
  flow?: readonly unknown[];
  nativeGraphSummary?: Record<string, unknown>;
  graphSummary?: Record<string, unknown>;
  packetPage?: Pagination;
  packets?: unknown[];
  /** Present only in detailed view — a windowed slice (see `manifestsPage`). */
  manifests?: unknown[];
  manifestsPage?: Pagination;
  /** Present only in detailed view — a windowed slice (see `filesPage`). */
  files?: unknown[];
  filesPage?: Pagination;
  /** Present only in detailed view — a windowed slice (see `dependenciesPage`). */
  dependencies?: unknown[];
  dependenciesPage?: Pagination;
  /** Present only in detailed view — a windowed slice (see `symbolsPage`). */
  symbols?: unknown[];
  symbolsPage?: Pagination;
  /** Present only in detailed view — a windowed slice (see `graphFactsPage`). */
  graphFacts?: unknown[];
  graphFactsPage?: Pagination;
  caveats?: string[];
  [k: string]: unknown;
}
export interface OqlGraphData {
  kind?: 'relationshipGraph';
  goal?: string;
  intent?: string;
  facets?: readonly string[];
  mode?: 'plan' | 'analyze' | 'prove';
  root?: string;
  filters?: Record<string, unknown>;
  summary?: Record<string, unknown>;
  flow?: readonly unknown[];
  nativeGraphSummary?: Record<string, unknown>;
  graphSummary?: unknown;
  packetPage?: Pagination;
  nodes?: unknown[];
  edges?: unknown[];
  facts?: unknown[];
  packets?: unknown[];
  missingProof?: unknown[];
  caveats?: string[];
  [k: string]: unknown;
}

/** Typed row aliases — a record row whose `data` matches its `recordType`. */
export type OqlRepositoryRow = OqlRecordResultRow & {
  recordType: 'repository';
  data: OqlRepositoryData;
};
export type OqlPackageRow = OqlRecordResultRow & {
  recordType: 'package';
  data: OqlPackageData;
};
export type OqlPullRequestRow = OqlRecordResultRow & {
  recordType: 'pullRequest';
  data: OqlPullRequestData;
};
export type OqlCommitRow = OqlRecordResultRow & {
  recordType: 'commit';
  data: OqlCommitData;
};
export type OqlArtifactRow = OqlRecordResultRow & {
  recordType: 'artifact';
  data: OqlArtifactData;
};
export type OqlDiffRow = OqlRecordResultRow & {
  recordType: 'diff';
  data: OqlDiffData;
};
export type OqlSemanticsRow = OqlRecordResultRow & {
  recordType: 'semantics';
  data: OqlSemanticsData;
};
export type OqlMaterializedRow = OqlRecordResultRow & {
  recordType: 'materialized';
  data: OqlMaterializedData;
};
export type OqlResearchRow = OqlRecordResultRow & {
  recordType: 'research';
  data: OqlResearchData;
};
export type OqlGraphRow = OqlRecordResultRow & {
  recordType: 'graph';
  data: OqlGraphData;
};

export type OqlResultRow =
  | OqlCodeResultRow
  | OqlFileResultRow
  | OqlTreeResultRow
  | OqlContentResultRow
  | OqlRecordResultRow;

export type OqlProofGradedResultRow = OqlResultRow & {
  proofGrade: OqlProofGrade;
};

export interface Pagination {
  currentPage?: number;
  totalPages?: number;
  nextPage?: number;
  itemsPerPage?: number;
  totalItems?: number;
  reportedTotalItems?: number;
  reachableTotalItems?: number;
  totalItemsKind?: string;
  itemUnit?: string;
  rowCount?: number;
  reportedRowCount?: number;
  totalItemsCapped?: boolean;
  uniqueFileCount?: number;
  hasMore: boolean;
  next?: OqlContinuation;
}

export interface OqlProvenance {
  backend: string;
  source?: QuerySource;
  predicateIds?: string[];
  pushed?: string[];
  residual?: string[];
  routed?: string[];
  materializedPath?: string;
  cache?: 'hit' | 'miss' | 'refresh' | 'stale';
}

export interface OqlContinuation {
  query: OqlCanonicalInput;
  baseQueryId?: string;
  queryIndex?: number;
  why?: string;
  confidence?: 'exact' | 'heuristic';
}

export interface OqlContinuationHint {
  why: string;
  confidence: 'exact' | 'heuristic';
}

export type EvidenceKind = 'proof' | 'partial' | 'candidate' | 'unsupported';

export interface OqlResultEnvelope {
  queryId?: string;
  queryIndex?: number;
  results: OqlProofGradedResultRow[];
  shared?: Record<string, unknown>;
  pagination?: Pagination;
  next?: Record<string, OqlContinuation>;
  nextHints?: Record<string, OqlContinuationHint>;
  diagnostics: OqlDiagnostic[];
  provenance: OqlProvenance[];
  evidence: {
    answerReady: boolean;
    complete: boolean;
    kind: EvidenceKind;
  };
  // present only when explain:true / --explain
  plan?: OqlExplainPlan;
}

export interface OqlBatchResultEnvelope {
  batchId?: string;
  mode: 'independent' | 'merge';
  children: Array<{
    queryId: string;
    queryIndex: number;
    envelope: OqlResultEnvelope;
  }>;
  merged?: OqlResultEnvelope;
  diagnostics: OqlDiagnostic[];
}

export type OqlRunResult = OqlResultEnvelope | OqlBatchResultEnvelope;

export function isBatchInput(input: OqlSearchInput): input is OqlInputBatch {
  return Array.isArray((input as OqlInputBatch).queries);
}

export function isCanonicalBatch(input: OqlCanonicalInput): input is OqlBatch {
  return Array.isArray((input as OqlBatch).queries);
}

export function isBatchEnvelope(
  result: OqlRunResult
): result is OqlBatchResultEnvelope {
  return Array.isArray((result as OqlBatchResultEnvelope).children);
}
