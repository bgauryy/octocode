/**
 * LSP Schema Overlay
 *
 * Mirrors the pattern in `localSchemaOverlay.ts` for LSP tools. The Zod
 * schemas for the LSP tools ship in `@octocodeai/octocode-core`; this overlay
 * re-publishes them with the cross-cutting `verbosity` field (basic | compact
 * | ultra, default "basic") so the agent sees the cost lever in the tool's
 * input schema.
 *
 * Behaviour is wired per-tool in each handler. Omitted ≡ `"basic"` (full
 * content + full hints). Description text comes from upstream
 * `baseSchema.verbosity` — no per-tool describe.
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

// Description text lives upstream in octocode-core baseSchema.verbosity;
// LSP-specific guidance belongs in each tool's <gotchas>.
const gotoDefinitionVerbosityField = createVerbosityField();
const findReferencesVerbosityField = createVerbosityField();
const callHierarchyVerbosityField = createVerbosityField();

// ---------------------------------------------------------------------------
// lspGotoDefinition
// ---------------------------------------------------------------------------

// Field descriptions are upstream (lspGotoDefinition.ts). Overlay supplies
// only the verbosity field and context-lines range.
export const LSPGotoDefinitionQuerySchema =
  UpstreamGotoDefinitionQuerySchema.extend({
    ...lspOptionalMetaFields,
    verbosity: gotoDefinitionVerbosityField,
    contextLines: contextLinesField,
  }).strip();

export const BulkLSPGotoDefinitionQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
  LSPGotoDefinitionQuerySchema,
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// lspFindReferences
// ---------------------------------------------------------------------------

// Field descriptions are upstream (lspFindReferences.ts). Overlay supplies
// only the verbosity field, context-lines/pagination ranges, and the
// `groupByFile` boolean (which has no upstream description today).
export const LSPFindReferencesQuerySchema =
  UpstreamFindReferencesQuerySchema.extend({
    ...lspOptionalMetaFields,
    verbosity: findReferencesVerbosityField,
    contextLines: contextLinesField,
    referencesPerPage: relaxedPaginationLimitField.default(10),
    page: relaxedPageNumberField.default(1),
    groupByFile: z.boolean().optional(),
  }).strip();

export const BulkLSPFindReferencesQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
  LSPFindReferencesQuerySchema,
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// lspCallHierarchy
// ---------------------------------------------------------------------------

// Field descriptions are upstream (lspCallHierarchy.ts). Overlay supplies
// only the verbosity field and context/pagination ranges.
export const LSPCallHierarchyQuerySchema =
  UpstreamCallHierarchyQuerySchema.extend({
    ...lspOptionalMetaFields,
    verbosity: callHierarchyVerbosityField,
    contextLines: contextLinesField,
    callsPerPage: relaxedPaginationLimitField.default(10),
    page: relaxedPageNumberField.default(1),
  }).strip();

export const BulkLSPCallHierarchyQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
  LSPCallHierarchyQuerySchema,
  { maxQueries: 5 }
);
