import type {
  ExactPosition,
  LSPRange,
} from '@octocodeai/octocode-engine/lsp/types';

export const SEMANTIC_CONTENT_TYPES = [
  'definition',
  'references',
  'callers',
  'callees',
  'callHierarchy',
  'hover',
  'documentSymbols',
  'typeDefinition',
  'implementation',
  // LSP 3.17 additions
  'workspaceSymbol',
  'supertypes',
  'subtypes',
  'diagnostic',
] as const;

export type SemanticContentType = (typeof SEMANTIC_CONTENT_TYPES)[number];
export type SemanticOutputFormat = 'structured' | 'compact';

export type SemanticQueryBase = {
  id?: string;
  type: SemanticContentType;
  uri?: string;
  workspaceRoot?: string;
  page?: number;
  pageSize?: number;
  snapshot?: string;
  contextLines?: number;
  format?: SemanticOutputFormat;
  goal?: string;
  reasoning?: string;
};

export type SymbolAnchoredSemanticQuery = SemanticQueryBase & {
  type: Exclude<
    SemanticContentType,
    'documentSymbols' | 'workspaceSymbol' | 'diagnostic'
  >;
  symbolName: string;
  lineHint: number;
  orderHint?: number;
  depth?: number;
  includeDeclaration?: boolean;
  groupByFile?: boolean;
};

export type DocumentSymbolsSemanticQuery = SemanticQueryBase & {
  type: 'documentSymbols';
};

/** `workspace/symbol`: project-wide fuzzy symbol search. `symbolName` is the query string. */
export type WorkspaceSymbolSemanticQuery = SemanticQueryBase & {
  type: 'workspaceSymbol';
  symbolName: string;
};

/** `textDocument/diagnostic` (pull): errors/warnings for a file without a position anchor. */
export type DiagnosticSemanticQuery = SemanticQueryBase & {
  type: 'diagnostic';
};

export type LspGetSemanticsQuery =
  | SymbolAnchoredSemanticQuery
  | DocumentSymbolsSemanticQuery
  | WorkspaceSymbolSemanticQuery
  | DiagnosticSemanticQuery;

export type ResolvedSymbol = {
  name: string;
  uri: string;
  range: LSPRange;
  foundAtLine: number;
  orderHint?: number;
  position: ExactPosition;
  isAmbiguous?: boolean;
  /** Lines between the caller's lineHint and the line actually bound —
   * present only when nonzero, so a stale hint is visible in the result. */
  lineDeviation?: number;
};

export type CompactResolvedSymbol = {
  name: string;
  uri: string;
  foundAtLine: number;
  orderHint?: number;
  isAmbiguous?: boolean;
  lineDeviation?: number;
};

export function compactResolvedSymbol(
  symbol: ResolvedSymbol
): CompactResolvedSymbol {
  return {
    name: symbol.name,
    uri: symbol.uri,
    foundAtLine: symbol.foundAtLine,
    ...(symbol.orderHint !== undefined && { orderHint: symbol.orderHint }),
    ...(symbol.isAmbiguous === true && { isAmbiguous: true }),
    ...(symbol.lineDeviation !== undefined && {
      lineDeviation: symbol.lineDeviation,
    }),
  };
}

export type CompactLocation = {
  uri: string;
  content?: string;
  displayRange?: { startLine: number; endLine: number };
  isDefinition?: boolean;
};

export function compactLocation(snippet: {
  uri: string;
  content?: string;
  range?: LSPRange;
  displayRange?: { startLine: number; endLine: number };
  isDefinition?: boolean;
}): CompactLocation {
  // References/definitions carry a 0-based LSP `range` but no `displayRange`;
  // derive a 1-based displayRange so consumers can report the line instead of
  // falling back to "?". An explicit displayRange always wins.
  const displayRange =
    snippet.displayRange ??
    (snippet.range
      ? {
          startLine: snippet.range.start.line + 1,
          endLine: snippet.range.end.line + 1,
        }
      : undefined);
  return {
    uri: snippet.uri,
    ...(snippet.content !== undefined && { content: snippet.content }),
    ...(displayRange && { displayRange }),
    ...(snippet.isDefinition && { isDefinition: true }),
  };
}

export type SemanticEmptyCategory =
  | 'unsupportedOperation'
  | 'symbolNotFound'
  | 'anchorFailed'
  | 'paginationChanged'
  | 'paginationSnapshotRequired'
  | 'noLocations'
  | 'noReferences'
  | 'noHover'
  | 'noCalls'
  | 'noWorkspaceSymbols'
  | 'noTypeHierarchy'
  | 'noDiagnostics';

export type SemanticEmptyState = {
  category: SemanticEmptyCategory;
  reason: string;
};

export type ConsumerWarmupStats = {
  candidates: number;
  warmedFiles: number;
  skippedLarge: number;
  possiblyTruncated: boolean;
  incompleteReasons?: Array<'fileCap' | 'fileRead' | 'search'>;
};

export type LspSemanticEnvelope = {
  type: SemanticContentType;
  uri: string;
  workspaceRoot?: string;
  format?: SemanticOutputFormat;
  resolvedSymbol?: CompactResolvedSymbol;
  lsp: {
    serverAvailable?: boolean;
    provider?: string;
    source?: string;
  };
  summary?: unknown;
  payload:
    | { kind: 'definition'; locations: Array<CompactLocation | string> }
    | {
        kind: 'references';
        locations?: Array<CompactLocation | string>;
        byFile?: unknown[];
        totalReferences: number;
        totalFiles: number;
        definitionOnly?: boolean;
        warmup?: ConsumerWarmupStats;
        empty?: SemanticEmptyState;
      }
    | {
        kind: 'callers' | 'callees' | 'callHierarchy';
        direction: 'incoming' | 'outgoing' | 'both';
        root?: unknown;
        calls: unknown[];
        incomingCalls?: number;
        outgoingCalls?: number;
        warmup?: ConsumerWarmupStats;
        completeness: {
          complete: boolean;
          consumerWarmupIncomplete?: true;
          truncatedByDepth: boolean;
          truncatedByBudget?: boolean;
          visitedNodeCount?: number;
          requestCount?: number;
          cycleCount: number;
          failedRequestCount: number;
          dynamicCallsExcluded: true;
          stdlibCallsExcluded?: number;
        };
        empty?: SemanticEmptyState;
      }
    | { kind: 'hover'; markdown?: string; text?: string; range?: LSPRange }
    | { kind: 'typeDefinition'; locations: Array<CompactLocation | string> }
    | {
        kind: 'implementation';
        locations: Array<CompactLocation | string>;
        warmup?: ConsumerWarmupStats;
      }
    | {
        kind: 'documentSymbols';
        symbols: unknown[];
        totalSymbols?: number;
        topLevelSymbols?: number;
        empty?: SemanticEmptyState;
      }
    | {
        kind: 'workspaceSymbol';
        query: string;
        symbols: unknown[];
        totalSymbols: number;
        empty?: SemanticEmptyState;
      }
    | {
        kind: 'typeHierarchy';
        direction: 'supertypes' | 'subtypes';
        root?: unknown;
        items: unknown[];
        totalItems: number;
        empty?: SemanticEmptyState;
      }
    | {
        kind: 'diagnostic';
        diagnostics: unknown[];
        totalDiagnostics: number;
        errorCount: number;
        warningCount: number;
        empty?: SemanticEmptyState;
      }
    | {
        kind: 'empty';
        category: SemanticEmptyCategory;
        reason: string;
        warmup?: ConsumerWarmupStats;
      };
  pagination?: unknown;
  warnings?: string[];
  hints?: string[];
  terminalLimit?: boolean;
  truncated?: boolean;
  partialReasons?: Array<'warmupCap' | 'warmupIncomplete' | 'depth' | 'budget'>;
  next?: Record<
    string,
    {
      tool: string;
      query: Record<string, unknown>;
      why?: string;
      confidence?: 'exact' | 'high' | 'medium' | 'low';
    }
  >;
};
