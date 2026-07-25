import { z } from 'zod';
import { LspGetSemanticsQuerySchema as CoreLspGetSemanticsQuerySchema } from '@octocodeai/octocode-core/schemas';
import { LOCAL_MAX_DEPTH } from '../../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../../scheme/coreSchemas.js';
import { SEMANTIC_CONTENT_TYPES } from '../shared/semanticTypes.js';
import type {
  ItemPagination,
  ToolContinuation,
} from '../../../scheme/pagination.js';
import type { BulkToolOutput } from '../../../types/toolOutput.js';

const requiredLineHintField = clampedInt(1, 1_000_000_000).describe(
  '1-based source line for symbol-anchored semantic operations. Get it from search/localSearchCode, structural AST captures, or documentSymbols; never guess.'
);
const orderHintField = clampedInt(0, 100_000).optional();

const SEMANTIC_OUTPUT_FORMATS = ['structured', 'compact'] as const;

const queryOverrides = {
  type: z
    .enum(SEMANTIC_CONTENT_TYPES)
    .default('definition')
    .describe(
      'Semantic operation for local code intelligence. Use after text or structural AST search when you need identity, references, call flow, type relations, hover, symbols, or diagnostics.'
    ),
  symbolName: z
    .string()
    .min(1)
    .max(1024)
    .optional()
    .describe(
      'Exact bare identifier at the lineHint anchor for symbol operations; workspaceSymbol uses this as the fuzzy project-wide symbol query.'
    ),
  lineHint: requiredLineHintField.optional(),
  orderHint: orderHintField,
  depth: clampedInt(0, LOCAL_MAX_DEPTH).optional(),
  includeDeclaration: z.boolean().optional().default(true),
  page: relaxedPageNumberField,
  itemsPerPage: clampedInt(1, 100).optional(),
  contextLines: clampedInt(0, 100).optional(),
  format: z.enum(SEMANTIC_OUTPUT_FORMATS).optional().default('structured'),
} as const;

const SemanticContentQueryShape = createQueryShapeSchema(
  CoreLspGetSemanticsQuerySchema,
  queryOverrides
);

export const LspGetSemanticsQueryDisplaySchema = describeQuerySchema(
  CoreLspGetSemanticsQuerySchema,
  queryOverrides
);

// Public-surface alias: octocode-mcp's public.ts re-exports this name.
// Do NOT remove as a "duplicate export" — external consumers import it.
export const LspGetSemanticsQuerySchema = LspGetSemanticsQueryDisplaySchema;

export const BulkLspGetSemanticsQuerySchema = createRelaxedBulkQuerySchema(
  SemanticContentQueryShape,
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// Output TYPES — describes what lspGetSemantics returns per query result row.
// No zod: the MCP server registers no outputSchema, so the output is a plain
// type. Shared envelope lives in types/toolOutput.ts.
// ---------------------------------------------------------------------------

interface LspLocation {
  uri: string;
  absolutePath?: string;
  path?: string;
  content?: string;
  displayRange?: { startLine: number; endLine: number };
  isDefinition?: boolean;
}

interface LspRange {
  start: { line: number; character: number };
  end: { line: number; character: number };
}

interface LspResolvedSymbol {
  name: string;
  uri: string;
  absolutePath?: string;
  path?: string;
  foundAtLine: number;
  orderHint?: number;
}

interface LspInfo {
  serverAvailable?: boolean;
  provider?: string;
  source?: string;
}

type LspEmptyCategory =
  | 'unsupportedOperation'
  | 'symbolNotFound'
  | 'anchorFailed'
  | 'noLocations'
  | 'noReferences'
  | 'noHover'
  | 'noCalls'
  | 'noWorkspaceSymbols'
  | 'noTypeHierarchy'
  | 'noDiagnostics';

interface LspEmptyState {
  category: LspEmptyCategory;
  reason: string;
}

interface LspCompactCallTarget {
  name: string;
  kind: string;
  uri: string;
  line: number;
  endLine: number;
  selectionLine?: number;
}

interface LspCompactCall {
  direction: 'incoming' | 'outgoing';
  item: LspCompactCallTarget;
  ranges: Array<{ line: number; character: number }>;
  rangeCount: number;
  rangeSampleCount: number;
  contentPreview?: string;
}

interface LspCompleteness {
  complete: boolean;
  truncatedByDepth: boolean;
  truncatedByBudget?: boolean;
  visitedNodeCount?: number;
  requestCount?: number;
  cycleCount: number;
  failedRequestCount: number;
  dynamicCallsExcluded: true;
  stdlibCallsExcluded?: number;
}

interface LspCompactSymbol {
  name: string;
  kind: string;
  line: number;
  character: number;
  endLine: number;
  childCount: number;
  containerName?: string;
}

interface LspReferencesByFile {
  uri: string;
  absolutePath?: string;
  path?: string;
  count: number;
  firstLine: number;
  firstCharacter: number;
  lines: number[];
  hasDefinition?: boolean;
}

// Row variants (LocationRow, CompactSymbolRow, …) are plain strings.
type LspSemanticPayload =
  | { kind: 'definition'; locations: Array<LspLocation | string> }
  | { kind: 'typeDefinition'; locations: Array<LspLocation | string> }
  | { kind: 'implementation'; locations: Array<LspLocation | string> }
  | {
      kind: 'references';
      locations?: Array<LspLocation | string>;
      byFile?: Array<LspReferencesByFile | string>;
      totalReferences: number;
      totalFiles: number;
      empty?: LspEmptyState;
    }
  | {
      kind: 'callers' | 'callees' | 'callHierarchy';
      root?: LspCompactCallTarget | string;
      direction: 'incoming' | 'outgoing' | 'both';
      calls: Array<LspCompactCall | string>;
      incomingCalls: number;
      outgoingCalls: number;
      completeness: LspCompleteness;
      empty?: LspEmptyState;
    }
  | { kind: 'hover'; markdown?: string; text?: string; range?: LspRange }
  | {
      kind: 'documentSymbols';
      symbols: Array<LspCompactSymbol | string>;
      totalSymbols?: number;
      topLevelSymbols?: number;
      empty?: LspEmptyState;
    }
  | {
      kind: 'workspaceSymbol';
      query: string;
      symbols: unknown[];
      totalSymbols: number;
      empty?: LspEmptyState;
    }
  | {
      kind: 'typeHierarchy';
      direction: 'supertypes' | 'subtypes';
      root?: unknown;
      items: unknown[];
      totalItems: number;
      empty?: LspEmptyState;
    }
  | {
      kind: 'diagnostic';
      diagnostics: unknown[];
      totalDiagnostics: number;
      errorCount: number;
      warningCount: number;
      empty?: LspEmptyState;
    }
  | { kind: 'empty'; category: LspEmptyCategory; reason: string };

export interface LspGetSemanticsData {
  type: string;
  uri: string;
  absolutePath?: string;
  path?: string;
  format?: 'structured' | 'compact';
  resolvedSymbol?: LspResolvedSymbol;
  // Omitted on early-return paths (e.g. symbolNotFound) where the LSP server is
  // never engaged; present on any path that reached a provider.
  lsp?: LspInfo;
  payload: LspSemanticPayload;
  pagination?: ItemPagination;
  summary?: Record<string, unknown>;
  warnings?: string[];
  // Ready-to-run follow-ups (e.g. next.readSite).
  next?: Record<string, ToolContinuation>;
  hints?: string[];
}

export type LspGetSemanticsOutput = BulkToolOutput<LspGetSemanticsData>;
