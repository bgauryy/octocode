/**
 * LSP Schema Overlay
 *
 * Mirrors the pattern in `localSchemaOverlay.ts` for LSP tools. The Zod
 * schemas for the LSP tools ship in `@octocodeai/octocode-core`; this overlay
 * re-publishes them with the cross-cutting `verbosity` field (basic | compact
 * | concise, default "basic") so the agent sees the cost lever in the tool's
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
} from '@octocodeai/octocode-core/schemas';
import { STATIC_TOOL_NAMES } from '../tools/toolNames.js';
import {
  createRelaxedBulkQuerySchema,
  createVerbosityField,
  contextLinesField,
  optionalMetaFields,
  relaxedPaginationLimitField,
  relaxedPageNumberField,
  depthField,
  describeField,
  requiredLineHintField,
  orderHintField,
  charOffsetField,
  localCharLengthField,
} from './localSchemaOverlay.js';

// Description text lives upstream in octocode-core baseSchema.verbosity;
// LSP-specific guidance belongs in each tool's <gotchas>.

// ---------------------------------------------------------------------------
// lspGotoDefinition
// ---------------------------------------------------------------------------

// Field descriptions are upstream (lspGotoDefinition.ts). Overlay supplies
// only the verbosity field and context-lines range.
export const LSPGotoDefinitionQuerySchema =
  UpstreamGotoDefinitionQuerySchema.extend({
    ...optionalMetaFields,
    uri: describeField(
      UpstreamGotoDefinitionQuerySchema.shape.uri,
      'File URI or path containing the symbol usage; absolute paths and workspace-relative paths are accepted.'
    ),
    lineHint: describeField(
      requiredLineHintField,
      '1-based line number near the symbol. Get it from localSearchCode before calling LSP tools.'
    ),
    orderHint: describeField(
      orderHintField,
      'Optional 0-based occurrence index on the hinted line when a symbol appears multiple times.'
    ),
    verbosity: createVerbosityField(),
    contextLines: contextLinesField,
    charOffset: charOffsetField,
    charLength: localCharLengthField,
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
    ...optionalMetaFields,
    uri: describeField(
      UpstreamFindReferencesQuerySchema.shape.uri,
      'File URI or path containing the symbol definition or usage to resolve.'
    ),
    lineHint: describeField(
      requiredLineHintField,
      '1-based line number near the symbol. Get it from localSearchCode before calling LSP tools.'
    ),
    orderHint: orderHintField,
    charOffset: charOffsetField,
    charLength: localCharLengthField,
    includePattern: describeField(
      UpstreamFindReferencesQuerySchema.shape.includePattern,
      'Optional glob(s) limiting reference results to matching file paths, useful in monorepos.'
    ),
    excludePattern: describeField(
      UpstreamFindReferencesQuerySchema.shape.excludePattern,
      'Optional glob(s) excluding noisy generated, fixture, or vendor paths from reference results.'
    ),
    verbosity: createVerbosityField(),
    contextLines: contextLinesField,
    referencesPerPage: relaxedPaginationLimitField.default(10),
    page: relaxedPageNumberField.default(1),
    groupByFile: z
      .boolean()
      .optional()
      .describe(
        'Return a per-file reference count rollup instead of individual snippets. Best for blast-radius probes.'
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

// Field descriptions are upstream (lspCallHierarchy.ts). Overlay supplies
// only the verbosity field and context/pagination ranges.
export const LSPCallHierarchyQuerySchema =
  UpstreamCallHierarchyQuerySchema.extend({
    ...optionalMetaFields,
    uri: describeField(
      UpstreamCallHierarchyQuerySchema.shape.uri,
      'File URI or path containing the function or method whose call graph should be traced.'
    ),
    lineHint: describeField(
      requiredLineHintField,
      '1-based line number near the callable symbol. Get it from localSearchCode before calling LSP tools.'
    ),
    orderHint: orderHintField,
    direction: describeField(
      UpstreamCallHierarchyQuerySchema.shape.direction,
      'Call graph direction: incoming shows callers; outgoing shows callees.'
    ),
    verbosity: createVerbosityField(),
    contextLines: contextLinesField,
    charOffset: charOffsetField,
    charLength: localCharLengthField,
    callsPerPage: relaxedPaginationLimitField.default(10),
    page: relaxedPageNumberField.default(1),
    depth: depthField,
  }).strip();

export const BulkLSPCallHierarchyQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY,
  LSPCallHierarchyQuerySchema,
  { maxQueries: 5 }
);
