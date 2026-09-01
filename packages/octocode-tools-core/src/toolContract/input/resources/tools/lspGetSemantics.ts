import { z } from 'zod';

import type { ToolSpec } from '../../types/index.js';
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
  instructions: `Use a real search/read line anchor for semantic proof. references finds usages; callers incoming calls; callees outgoing calls; callHierarchy both. Fall back to references if call hierarchy is unavailable.
documentSymbols/diagnostic need uri; workspaceSymbol needs symbolName; other operations need uri+symbolName+lineHint. orderHint resolves same-line names. format:"compact" saves tokens. Empty/unavailable results call for a new anchor or text search.`,
  schema: {
    pageSize: 'References or symbols returned per page.',
    page: 'Reference/symbol result page.',
    type: 'Identity, usage, hierarchy, hover, symbol, or diagnostic operation.',
    uri: 'Target file path/URI; required for every type except workspaceSymbol.',
    symbolName: 'Bare anchored identifier, or fuzzy workspace symbol query.',
    lineHint: 'Observed line containing symbolName.',
    orderHint: 'Disambiguate repeated symbols on one line.',
    depth: 'Call/type hierarchy depth; 0 is direct only.',
    includeDeclaration:
      'references: include the declaration; disable before unused analysis.',
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
  pageSize: intRange(1, MAX_LSP_ITEMS_PER_PAGE).optional(),
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
        message: 'Set symbolName for workspaceSymbol.',
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
      message: 'Set uri for file-scoped operations.',
    });
  }
  if (query.type === 'documentSymbols' || query.type === 'diagnostic') return;

  // All remaining types need both symbolName and lineHint.
  if (!query.symbolName) {
    ctx.addIssue({
      code: 'custom',
      path: ['symbolName'],
      message: 'Set symbolName for anchored operations.',
    });
  }
  if (!Number.isInteger(query.lineHint)) {
    ctx.addIssue({
      code: 'custom',
      path: ['lineHint'],
      message: 'Set lineHint for anchored operations.',
    });
  }
});
