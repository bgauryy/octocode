/**
 * Octocode Query Language (OQL) V1 — public surface.
 *
 * `octocode search` (CLI) and the OQL MCP tool both consume this module:
 *   - `runOqlSearch` executes a query/batch and returns a typed envelope.
 *   - `normalizeInput` / `planQuery` expose the normalize + explain stages.
 *   - `oqlSchemaText()` / `OQL_SCHEMA_JSON` back `octocode search --scheme`.
 *
 * Schemas/types co-locate here for V1 (tools-core). Per the implementation
 * plan's Phase 8, they may migrate to `@octocodeai/octocode-core/oql` once a
 * second consumer needs OQL validation without the rest of tools-core.
 */
export * from './types.js';
export {
  OqlQuerySchema,
  OqlBatchSchema,
  OqlCanonicalInputSchema,
  OqlInputQuerySchema,
  OqlInputBatchSchema,
  PredicateSchema,
  QuerySourceSchema,
  StructuralRuleSchema,
} from './schema.js';
export { normalizeInput, normalizeQuery } from './normalize.js';
export { planQuery, type PlanQueryResult } from './planner.js';
export { DEFAULTS, appliedDefaults } from './defaults.js';
export {
  diagnostic,
  blocksAnswer,
  hasErrors,
  OqlValidationError,
} from './diagnostics.js';
export {
  buildEnvelope,
  unsupportedEnvelope,
  backendsApproximate,
} from './envelope.js';
export { runOqlSearch, type RunOptions } from './run.js';
export {
  buildShorthandInput,
  type SearchShorthand,
  type ShorthandCorpus,
  type ShorthandResult,
} from './shorthand.js';
export { oqlSchemaText, OQL_SCHEMA_DOC } from './schemeText.js';
export {
  analyzeResearchFlow,
  type AnalyzeResearchOptions,
  type ResearchAnalysisResult,
  type ResearchDependencyIssue,
  type ResearchFileIssue,
  type ResearchFlowStep,
  type ResearchIntent,
  type ResearchManifestSummary,
  type ResearchMode,
  type ResearchSymbolRow,
  type ResearchSymbolVerdict,
} from './research/analyze.js';
