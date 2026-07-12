/**
 * Octocode Query Language (OQL) — canonical type definitions.
 *
 * These mirror the contract in
 * docs/octocode-language/OCTOCODE_QUERY_LANGUAGE.md. OQL is a typed research
 * query object that compiles into existing Octocode tool runners; it is not a
 * raw string DSL. Schemas/descriptions co-locate here for now (tools-core) and
 * may migrate to `@octocodeai/octocode-core/oql` once a second consumer needs
 * OQL validation without the rest of tools-core.
 *
 * This file is a barrel: the actual definitions (targets such as
 * `OqlActiveTarget`, predicates, query/batch input, diagnostics, planner,
 * result rows, record `data` contracts + typed row aliases, and the result
 * envelope) live under `./types/`, split by cohesion. Re-exporting everything
 * here keeps every existing import of `oql/types(.js)` working unchanged.
 */

export type {
  OqlActiveTarget,
  OqlReservedTarget,
  OqlTarget,
} from './types/targets.js';
export {
  ACTIVE_TARGETS,
  RESERVED_TARGETS,
  CORPUS_OPTIONAL_TARGETS,
  SEARCH_SORTS_BY_TARGET,
} from './types/targets.js';

export type {
  PredicateId,
  QuerySource,
  QueryScope,
  TextPredicate,
  RegexPredicate,
  StructuralRule,
  StructuralRuleInput,
  StructuralPredicate,
  FieldName,
  FieldOp,
  FieldPredicate,
  AllPredicate,
  AnyPredicate,
  NotPredicate,
  LeafPredicate,
  Predicate,
} from './types/predicates.js';

export type {
  MaterializePolicy,
  FetchInstructions,
  QueryControls,
  SelectField,
  QueryView,
  OqlQuery,
  OqlBatch,
  OqlCanonicalInput,
  OqlInputQuery,
  OqlInputBatch,
  OqlSearchInput,
} from './types/query.js';

export type { DiagnosticCode, OqlDiagnostic } from './types/diagnostics.js';

export type {
  PlanRoute,
  OqlPlanNode,
  OqlBackendCall,
  OqlTransformerTrace,
  OqlExplainPlan,
} from './types/planner.js';

export type {
  OqlProofGrade,
  OqlCodeResultRow,
  OqlFileResultRow,
  OqlTreeResultRow,
  OqlContentResultRow,
  OqlRecordResultRow,
} from './types/results.js';

export type {
  OqlRepositoryData,
  OqlPackageData,
  OqlPullRequestData,
  OqlCommitData,
  OqlDiffData,
  OqlSemanticsData,
  OqlMaterializedData,
  OqlResearchData,
  OqlGraphData,
} from './types/recordData.js';

export type {
  OqlRepositoryRow,
  OqlPackageRow,
  OqlPullRequestRow,
  OqlCommitRow,
  OqlDiffRow,
  OqlSemanticsRow,
  OqlMaterializedRow,
  OqlResearchRow,
  OqlGraphRow,
  OqlResultRow,
  OqlProofGradedResultRow,
} from './types/recordRows.js';

export type {
  Pagination,
  OqlProvenance,
  OqlContinuation,
  OqlContinuationHint,
  EvidenceKind,
  OqlResultEnvelope,
  OqlBatchResultEnvelope,
  OqlRunResult,
} from './types/envelope.js';

export {
  isBatchInput,
  isCanonicalBatch,
  isBatchEnvelope,
} from './types/guards.js';
