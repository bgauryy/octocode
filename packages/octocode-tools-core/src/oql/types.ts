/**
 * Octocode Query Language (OQL) V1 — canonical type definitions.
 *
 * These mirror the contract in
 * docs/octocode-language/OCTOCODE_QUERY_LANGUAGE.md. OQL is a typed research
 * query object that compiles into existing Octocode tool runners; it is not a
 * raw string DSL. Schemas/descriptions co-locate here for now (tools-core) and
 * may migrate to `@octocodeai/octocode-core/oql` per the plan's Phase 8.
 */

// V1 code-research targets + V2 research-surface targets (now active).
export type OqlActiveTargetV1 =
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
  // Addressable materialization: clone/cache a bounded corpus and return a
  // stable local checkpoint (not a side-effect of a search).
  | 'materialize';

// V3 (fixes/dataflow) remain reserved — they need engine proof support.
export type OqlReservedTarget = 'fixes' | 'dataflow';

export type OqlTarget = OqlActiveTargetV1 | OqlReservedTarget;

export const ACTIVE_TARGETS: readonly OqlActiveTargetV1[] = [
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
  'materialize',
];

export const RESERVED_TARGETS: readonly OqlReservedTarget[] = [
  'fixes',
  'dataflow',
];

/** Targets that do not need a code corpus (provider/registry discovery). */
export const CORPUS_OPTIONAL_TARGETS: readonly OqlActiveTargetV1[] = [
  'packages',
  'repositories',
];

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

export interface StructuralPredicate {
  id?: PredicateId;
  kind: 'structural';
  lang: string;
  pattern?: string;
  rule?: StructuralRule;
}

export type FieldName =
  | 'path'
  | 'basename'
  | 'extension'
  | 'size'
  | 'modified'
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
  | 'within';

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
    includeSizes?: boolean;
  };
}

export interface QueryControls {
  search?: {
    countLinesPerFile?: boolean;
    countMatchesPerFile?: boolean;
    onlyMatching?: boolean;
    unique?: boolean;
    countUnique?: boolean;
    matchWindow?: number;
    matchContentLength?: number;
    maxMatchesPerFile?: number;
    matchPage?: number;
    sort?:
      | 'relevance'
      | 'matchCount'
      | 'path'
      | 'modified'
      | 'accessed'
      | 'created';
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

export interface OqlQueryV1 {
  schema: 'oql/v1';
  id?: string;
  target: OqlActiveTargetV1;
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
   * Target-specific parameter bag for V2 research targets (semantics,
   * repositories, packages, pullRequests, commits, artifacts, diff). The
   * backing tool's schema validates it; the planner only routes by target.
   */
  params?: Record<string, unknown>;
  explain?: boolean;
}

export interface OqlBatchV1 {
  schema: 'oql/v1';
  id?: string;
  queries: OqlQueryV1[];
  combine?: 'independent' | 'merge';
  limit?: number;
  page?: number;
  itemsPerPage?: number;
  explain?: boolean;
}

export type OqlCanonicalInputV1 = OqlQueryV1 | OqlBatchV1;

export interface OqlInputQueryV1 {
  schema?: 'oql/v1';
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
  langType?: string;
  minify?: 'none' | 'standard' | 'symbols';
  [key: string]: unknown;
}

export interface OqlInputBatchV1 {
  schema?: 'oql/v1';
  id?: string;
  queries: OqlInputQueryV1[];
  combine?: 'independent' | 'merge';
  limit?: number;
  page?: number;
  itemsPerPage?: number;
  explain?: boolean;
}

export type OqlSearchInputV1 = OqlInputQueryV1 | OqlInputBatchV1;

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
  | 'zeroMatches';

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
    suggestedQuery?: OqlSearchInputV1;
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

export interface OqlExplainPlan {
  input: unknown;
  normalized: OqlCanonicalInputV1;
  defaults: Record<string, unknown>;
  nodes: OqlPlanNode[];
  backendCalls: OqlBackendCall[];
  materialization?: MaterializePolicy & { required: boolean; reason: string };
  budgets: QueryControls['budget'];
  truncated?: boolean;
  diagnostics: OqlDiagnostic[];
  next?: Record<string, OqlContinuation>;
}

/* --------------------------- result envelope ---------------------------- */

export interface OqlCodeResultRow {
  kind: 'code';
  source: QuerySource;
  path: string;
  /**
   * 1-based match line. Optional because some providers (GitHub code search)
   * return path-level matches with no line — never fabricate one.
   */
  line?: number;
  endLine?: number;
  column?: number;
  snippet?: string;
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

export interface OqlFileResultRow {
  kind: 'file';
  source: QuerySource;
  path: string;
  entryType: 'file' | 'directory';
  size?: number;
  modified?: string;
  next?: Record<string, OqlContinuation>;
}

export interface OqlTreeResultRow {
  kind: 'tree';
  source: QuerySource;
  path: string;
  entryType: 'file' | 'directory';
  depth: number;
  size?: number;
  children?: OqlTreeResultRow[];
  next?: Record<string, OqlContinuation>;
}

export interface OqlContentResultRow {
  kind: 'content';
  source: QuerySource;
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
 * Generic record row for V2 research targets whose payload is the backing
 * tool's typed result (repository, package, PR, commit, symbol/location,
 * artifact, diff). `recordType` names the family; `data` is the row payload.
 */
export interface OqlRecordResultRow {
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
    | 'materialized';
  /** Stable, citeable identity (repo, name@version, #PR, SHA, path, uri). */
  id?: string;
  source?: QuerySource;
  data: Record<string, unknown>;
  next?: Record<string, OqlContinuation>;
}

/* --- typed `data` contracts per recordType (documented payload shapes) ----
 * The backing tool owns the exhaustive payload; these name the fields agents
 * rely on to cite + continue. All optional (backend-dependent); never fabricated.
 * Parity: OCTOCODE_SEARCH_PARITY_CHECKLIST gap #4. */

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
  facets?: string[];
  summary?: Record<string, unknown>;
  flow?: unknown[];
  files?: unknown[];
  dependencies?: unknown[];
  symbols?: unknown[];
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

export type OqlResultRow =
  | OqlCodeResultRow
  | OqlFileResultRow
  | OqlTreeResultRow
  | OqlContentResultRow
  | OqlRecordResultRow;

export interface Pagination {
  currentPage?: number;
  totalPages?: number;
  itemsPerPage?: number;
  totalItems?: number;
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
  query: OqlCanonicalInputV1;
  baseQueryId?: string;
  queryIndex?: number;
  why: string;
  confidence: 'exact' | 'heuristic';
}

export type EvidenceKind = 'proof' | 'partial' | 'candidate' | 'unsupported';

export interface OqlResultEnvelope {
  queryId?: string;
  queryIndex?: number;
  results: OqlResultRow[];
  pagination?: Pagination;
  next?: Record<string, OqlContinuation>;
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

export function isBatchInput(
  input: OqlSearchInputV1
): input is OqlInputBatchV1 {
  return Array.isArray((input as OqlInputBatchV1).queries);
}

export function isCanonicalBatch(
  input: OqlCanonicalInputV1
): input is OqlBatchV1 {
  return Array.isArray((input as OqlBatchV1).queries);
}

export function isBatchEnvelope(
  result: OqlRunResult
): result is OqlBatchResultEnvelope {
  return Array.isArray((result as OqlBatchResultEnvelope).children);
}
