/**
 * LSP Schema Overlay
 *
 * Mirrors the pattern in `localSchemaOverlay.ts` for LSP tools. The Zod
 * schemas for the LSP tools ship in `@octocodeai/octocode-core`; this overlay
 * re-publishes them with the cross-cutting `verbosity` field so the agent
 * sees the cost-aware mode selector in the tool's input schema.
 *
 * Behaviour is wired per-tool in each handler. Omitted `verbosity` ⇒
 * byte-identical to current behaviour (§3.1 of the RFC).
 *
 * @see `.octocode/rfc/rtk-token-techniques/RFC.md` §4.7.5–§4.7.9
 */

import { z } from 'zod/v4';
import {
  LSPGotoDefinitionQuerySchema as UpstreamGotoDefinitionQuerySchema,
  LSPFindReferencesQuerySchema as UpstreamFindReferencesQuerySchema,
  LSPCallHierarchyQuerySchema as UpstreamCallHierarchyQuerySchema,
} from '@octocodeai/octocode-core';
import { STATIC_TOOL_NAMES } from '../tools/toolNames.js';
import {
  createRelaxedBulkQuerySchema,
  createVerbosityField,
  describeShapeFields,
  contextLinesField,
  relaxedPaginationLimitField,
  relaxedPageNumberField,
} from './localSchemaOverlay.js';

const lspOptionalMetaFields = {
  id: z.string().optional().describe('Stable query identifier.'),
  mainResearchGoal: z
    .string()
    .optional()
    .describe('Overall research objective shared by related queries.'),
  researchGoal: z
    .string()
    .optional()
    .describe('Specific goal this query is trying to answer.'),
  reasoning: z
    .string()
    .optional()
    .describe('Why this query helps achieve the research goal.'),
} as const;

const gotoDefinitionVerbosityField = createVerbosityField(
  'definition locations with ranges, snippets, resolved position, and semantic/fallback mode',
  'definition count plus top path:line:column; location content is empty',
  're-call with verbosity:"compact" for snippets around the location'
);

const findReferencesVerbosityField = createVerbosityField(
  'reference locations with ranges, snippets, definition markers, pagination, and semantic/fallback mode',
  'reference counts plus path:line refs, or a per-file rollup with groupByFile; snippets are dropped',
  're-call with verbosity:"compact", groupByFile, or includePattern'
);

const callHierarchyVerbosityField = createVerbosityField(
  'target item plus caller/callee nodes, snippets, call ranges, pagination, and semantic/fallback mode',
  'edge counts and a compact A -> B edge list; node content and call arrays are dropped',
  're-call with verbosity:"compact" for full per-node context'
);

// ---------------------------------------------------------------------------
// lspGotoDefinition
// ---------------------------------------------------------------------------

export const LSPGotoDefinitionQuerySchema =
  UpstreamGotoDefinitionQuerySchema.extend({
    ...lspOptionalMetaFields,
    ...describeShapeFields(UpstreamGotoDefinitionQuerySchema.shape, {
      symbolName: 'EXACT symbol text, no parens, no partials',
      lineHint: '1-indexed line. Tool searches ±2 lines',
      orderHint: '0-indexed occurrence if multiple on line',
    }),
    verbosity: gotoDefinitionVerbosityField,
    contextLines: contextLinesField.describe('Context lines around match'),
  }).strip();

export const BulkLSPGotoDefinitionQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
  LSPGotoDefinitionQuerySchema,
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// lspFindReferences
// ---------------------------------------------------------------------------

export const LSPFindReferencesQuerySchema =
  UpstreamFindReferencesQuerySchema.extend({
    ...lspOptionalMetaFields,
    ...describeShapeFields(UpstreamFindReferencesQuerySchema.shape, {
      uri: 'File path. Example: "src/api/client.ts"',
      symbolName: 'EXACT symbol text, no parens, no partials',
      lineHint: '1-indexed line. Tool searches ±2 lines',
      orderHint: '0-indexed occurrence if multiple on line',
      includeDeclaration: 'Include definition in results',
      includePattern: 'Glob array — restrict search to these paths',
      excludePattern: 'Glob array — exclude these paths',
    }),
    verbosity: findReferencesVerbosityField,
    contextLines: contextLinesField.describe('Context lines around match'),
    referencesPerPage: relaxedPaginationLimitField
      .default(10)
      .describe('Max refs per page'),
    page: relaxedPageNumberField.default(1).describe('1-indexed page'),
    groupByFile: z
      .boolean()
      .optional()
      .describe(
        'Roll up references into per-file counts (cheaper, for impact analysis)'
      ),
  }).strip();

export const BulkLSPFindReferencesQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
  LSPFindReferencesQuerySchema,
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// lspCallHierarchy
// ---------------------------------------------------------------------------

export const LSPCallHierarchyQuerySchema =
  UpstreamCallHierarchyQuerySchema.extend({
    ...lspOptionalMetaFields,
    ...describeShapeFields(UpstreamCallHierarchyQuerySchema.shape, {
      uri: 'File path. Example: "src/api/handler.ts"',
      symbolName: 'EXACT function/method name, no parens',
      lineHint: '1-indexed line where function is defined or called',
      orderHint: '0-indexed occurrence if multiple on line',
      direction: '"incoming" (callers) | "outgoing" (callees)',
      depth: 'Recursion depth',
    }),
    verbosity: callHierarchyVerbosityField,
    contextLines: contextLinesField.describe('Context lines around match'),
    callsPerPage: relaxedPaginationLimitField
      .default(10)
      .describe('Max call sites per page'),
    page: relaxedPageNumberField.default(1).describe('1-indexed page'),
  }).strip();

export const BulkLSPCallHierarchyQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
  LSPCallHierarchyQuerySchema,
  { maxQueries: 5 }
);
