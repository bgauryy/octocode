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
  createBulkQuerySchema,
} from '@octocodeai/octocode-core';
import { STATIC_TOOL_NAMES } from '../tools/toolNames.js';
import { createVerbosityField, verbosityField } from './localSchemaOverlay.js';

export { verbosityField };

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
    verbosity: gotoDefinitionVerbosityField,
  });

export type LSPGotoDefinitionQuery = z.infer<
  typeof LSPGotoDefinitionQuerySchema
>;

export const BulkLSPGotoDefinitionQuerySchema = createBulkQuerySchema(
  STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
  LSPGotoDefinitionQuerySchema,
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// lspFindReferences
// ---------------------------------------------------------------------------

export const LSPFindReferencesQuerySchema =
  UpstreamFindReferencesQuerySchema.extend({
    verbosity: findReferencesVerbosityField,
    groupByFile: z
      .boolean()
      .optional()
      .describe(
        'Roll up references into per-file counts ({ "src/foo.ts": 7 }) ' +
          'instead of returning individual locations. Less tokens than the ' +
          'full locations list — use for impact-analysis ("is this used widely?"). ' +
          'Drill-back: re-query with `includePattern` scoped to the top file(s).'
      ),
  });

export type LSPFindReferencesQuery = z.infer<
  typeof LSPFindReferencesQuerySchema
>;

export const BulkLSPFindReferencesQuerySchema = createBulkQuerySchema(
  STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
  LSPFindReferencesQuerySchema,
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// lspCallHierarchy
// ---------------------------------------------------------------------------

export const LSPCallHierarchyQuerySchema =
  UpstreamCallHierarchyQuerySchema.extend({
    verbosity: callHierarchyVerbosityField,
  });

export type LSPCallHierarchyQuery = z.infer<typeof LSPCallHierarchyQuerySchema>;

export const BulkLSPCallHierarchyQuerySchema = createBulkQuerySchema(
  STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
  LSPCallHierarchyQuerySchema,
  { maxQueries: 5 }
);
