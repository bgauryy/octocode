/**
 * LSP Schema Overlay
 *
 * Final field descriptions are applied from @octocodeai/octocode-core metadata
 * via withCoreSchemaDescriptions. This file only changes shapes, bounds, and
 * public field names.
 */

import { z } from 'zod';
import {
  LSPGotoDefinitionQuerySchema as UpstreamGotoDefinitionQuerySchema,
  LSPFindReferencesQuerySchema as UpstreamFindReferencesQuerySchema,
  LSPCallHierarchyQuerySchema as UpstreamCallHierarchyQuerySchema,
} from '@octocodeai/octocode-core/schemas';
import { STATIC_TOOL_NAMES } from '../tools/toolNames.js';
import {
  createRelaxedBulkQuerySchema,
  createVerbosityFields,
  contextLinesField,
  optionalMetaFields,
  relaxedPageNumberField,
  depthField,
  requiredLineHintField,
  orderHintField,
  DEFAULT_PAGE_SIZE,
  withCoreSchemaDescriptions,
} from './localSchemaOverlay.js';

// ---------------------------------------------------------------------------
// lspGotoDefinition
// ---------------------------------------------------------------------------

export const LSPGotoDefinitionQuerySchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
  UpstreamGotoDefinitionQuerySchema.extend({
    ...optionalMetaFields,
    lineHint: requiredLineHintField,
    orderHint: orderHintField,
    ...createVerbosityFields(),
    contextLines: contextLinesField,
  })
);

export const BulkLSPGotoDefinitionQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
  LSPGotoDefinitionQuerySchema,
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// lspFindReferences
// ---------------------------------------------------------------------------

export const LSPFindReferencesQuerySchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
  UpstreamFindReferencesQuerySchema.omit({
    referencesPerPage: true,
  })
    .extend({
      ...optionalMetaFields,
      lineHint: requiredLineHintField,
      orderHint: orderHintField,
      ...createVerbosityFields(),
      contextLines: contextLinesField,
      page: relaxedPageNumberField
        .default(1)
        .describe(
          `Result page (1-based). Each page returns up to ${DEFAULT_PAGE_SIZE} references.`
        ),
      groupByFile: z.boolean().optional(),
    })
);

export const BulkLSPFindReferencesQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
  LSPFindReferencesQuerySchema,
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// lspCallHierarchy
// ---------------------------------------------------------------------------

export const LSPCallHierarchyQuerySchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
  UpstreamCallHierarchyQuerySchema.omit({
    callsPerPage: true,
  })
    .extend({
      ...optionalMetaFields,
      lineHint: requiredLineHintField,
      orderHint: orderHintField,
      ...createVerbosityFields(),
      contextLines: contextLinesField,
      page: relaxedPageNumberField
        .default(1)
        .describe(
          `Result page (1-based). Each page returns up to ${DEFAULT_PAGE_SIZE} calls.`
        ),
      depth: depthField,
    })
);

export const BulkLSPCallHierarchyQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
  LSPCallHierarchyQuerySchema,
  { maxQueries: 5 }
);
