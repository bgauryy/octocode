import { z } from 'zod';

import type { ToolSpec } from '../../types.js';
import {
  buildObject,
  contextLines,
  defineTool,
  intRange,
  MAX_LINE_NUMBER,
  MAX_LSP_DEPTH,
  MAX_LSP_ITEMS_PER_PAGE,
  MAX_ORDER_HINT,
  metaFields,
  pageNumber,
} from './_toolkit.js';

export const lspGetSemantics: ToolSpec = defineTool({
  name: 'lspGetSemantics',
  type: 'Local',
  shortDescription:
    'Run LSP semantic queries — definitions, references, call hierarchy, symbols, type hierarchy, diagnostics.',
  instructions: `Use after search/read gives a real file+line anchor. documentSymbols/diagnostic need uri only; workspaceSymbol needs symbolName; others need uri+symbolName+lineHint.
lineHint must come from a prior anchor, never guessed; orderHint disambiguates same-line symbols; references take groupByFile/includeDeclaration; callers/callees/callHierarchy take depth/contextLines; format:"compact" saves tokens. Empty/serverUnavailable → re-anchor or fall back to search.`,
  schema: {
    itemsPerPage: 'References/symbols returned per page (with page).',
    page: 'Result page for paginated reference/symbol lists (advance while pagination.hasMore).',
    type: 'Semantic operation, grouped by concept: identity — definition (where declared), typeDefinition (its type, not the value), implementation (concrete impls of an interface/abstract member); usage — references (all call sites), callers/callees (one hop), callHierarchy (full tree); type hierarchy — supertypes/subtypes; other — hover (signature+doc), documentSymbols (file outline, uri only), workspaceSymbol (fuzzy project-wide name search, no uri needed), diagnostic (compiler/linter errors, uri only).',
    uri: 'Target file path/URI; required for every type except workspaceSymbol.',
    symbolName:
      'Exact bare identifier at the lineHint anchor for symbol operations; workspaceSymbol uses this as the fuzzy project-wide symbol query.',
    lineHint: 'Real line containing symbolName; never guess.',
    orderHint: 'Disambiguate repeated symbols on one line.',
    depth:
      'Traversal depth for call-hierarchy / type-hierarchy queries (0 = direct only).',
    includeDeclaration:
      'references only; defaults true, which counts the declaration itself as one hit — set false (or expect a floor of 1) before treating a low count as unused.',
    groupByFile: 'references summary mode.',
    contextLines:
      'Lines of surrounding source shown around each result location.',
    format: '"compact" saves tokens; "structured" has typed locations.',
    workspaceRoot: 'Use when auto-root is wrong.',
  },
});

export const LspGetSemanticsQuerySchema = buildObject(lspGetSemantics.schema, {
  ...metaFields,
  uri: z.string().optional(),
  type: z
    .enum([
      'definition',
      'references',
      'callers',
      'callees',
      'callHierarchy',
      'hover',
      'documentSymbols',
      'typeDefinition',
      'implementation',
      'workspaceSymbol',
      'supertypes',
      'subtypes',
      'diagnostic',
    ])
    .default('definition'),
  symbolName: z.string().min(1).optional(),
  lineHint: intRange(1, MAX_LINE_NUMBER).optional(),
  orderHint: intRange(0, MAX_ORDER_HINT).default(0),
  depth: intRange(0, MAX_LSP_DEPTH).optional(),
  includeDeclaration: z.boolean().default(true),
  groupByFile: z.boolean().optional(),
  page: pageNumber(),
  itemsPerPage: intRange(1, MAX_LSP_ITEMS_PER_PAGE).optional(),
  contextLines: contextLines(),
  format: z.enum(['structured', 'compact']).default('structured'),
  workspaceRoot: z.string().optional(),
}).superRefine((query, ctx) => {
  // workspaceSymbol: needs a query string (symbolName) but not a position.
  if (query.type === 'workspaceSymbol') {
    if (!query.symbolName) {
      ctx.addIssue({
        code: 'custom',
        path: ['symbolName'],
        message:
          'symbolName (the search query) is required for workspaceSymbol',
      });
    }
    return;
  }

  // File-scoped types need a URI. documentSymbols/diagnostic do not need a
  // symbol position, but still need a file to inspect.
  if (!query.uri) {
    ctx.addIssue({
      code: 'custom',
      path: ['uri'],
      message: 'uri is required unless type is workspaceSymbol',
    });
  }
  if (query.type === 'documentSymbols' || query.type === 'diagnostic') return;

  // All remaining types need both symbolName and lineHint.
  if (!query.symbolName) {
    ctx.addIssue({
      code: 'custom',
      path: ['symbolName'],
      message:
        'symbolName is required unless type is documentSymbols, workspaceSymbol, or diagnostic',
    });
  }
  if (!Number.isInteger(query.lineHint)) {
    ctx.addIssue({
      code: 'custom',
      path: ['lineHint'],
      message:
        'lineHint is required unless type is documentSymbols, workspaceSymbol, or diagnostic',
    });
  }
});
