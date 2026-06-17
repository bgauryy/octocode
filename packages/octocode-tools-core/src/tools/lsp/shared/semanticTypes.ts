import type { ExactPosition, LSPRange } from 'octocode-lsp/types';

export const LSP_GET_SEMANTIC_CONTENT_TOOL_NAME = 'lspGetSemantics';

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
] as const;

export type SemanticContentType = (typeof SEMANTIC_CONTENT_TYPES)[number];
export type SemanticOutputFormat = 'structured' | 'compact';

export type SemanticQueryBase = {
  id?: string;
  type: SemanticContentType;
  uri?: string;
  workspaceRoot?: string;
  page?: number;
  itemsPerPage?: number;
  contextLines?: number;
  format?: SemanticOutputFormat;
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
};

export type SymbolAnchoredSemanticQuery = SemanticQueryBase & {
  type: Exclude<SemanticContentType, 'documentSymbols'>;
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

export type LspGetSemanticsQuery =
  | SymbolAnchoredSemanticQuery
  | DocumentSymbolsSemanticQuery;

export type ResolvedSymbol = {
  name: string;
  uri: string;
  range: LSPRange;
  foundAtLine: number;
  orderHint?: number;
  position: ExactPosition;
  isAmbiguous?: boolean;
};

export type CompactResolvedSymbol = {
  name: string;
  uri: string;
  foundAtLine: number;
  orderHint?: number;
  isAmbiguous?: boolean;
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
  displayRange?: { startLine: number; endLine: number };
  isDefinition?: boolean;
}): CompactLocation {
  return {
    uri: snippet.uri,
    ...(snippet.content !== undefined && { content: snippet.content }),
    ...(snippet.displayRange && { displayRange: snippet.displayRange }),
    ...(snippet.isDefinition && { isDefinition: true }),
  };
}

export type SemanticEmptyCategory =
  | 'serverUnavailable'
  | 'unsupportedOperation'
  | 'symbolNotFound'
  | 'anchorFailed'
  | 'noLocations'
  | 'noReferences'
  | 'noHover'
  | 'noCalls';

export type SemanticEmptyState = {
  category: SemanticEmptyCategory;
  reason: string;
};

export type LspSemanticEnvelope = {
  type: SemanticContentType;
  uri: string;
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
        empty?: SemanticEmptyState;
      }
    | {
        kind: 'callers' | 'callees' | 'callHierarchy';
        direction: 'incoming' | 'outgoing' | 'both';
        root?: unknown;
        calls: unknown[];
        incomingCalls?: number;
        outgoingCalls?: number;
        completeness: {
          complete: boolean;
          truncatedByDepth: boolean;
          cycleCount: number;
          failedRequestCount: number;
          dynamicCallsExcluded: true;
          stdlibCallsExcluded?: number;
        };
        empty?: SemanticEmptyState;
      }
    | { kind: 'hover'; markdown?: string; text?: string; range?: LSPRange }
    | { kind: 'typeDefinition'; locations: Array<CompactLocation | string> }
    | { kind: 'implementation'; locations: Array<CompactLocation | string> }
    | {
        kind: 'documentSymbols';
        symbols: unknown[];
        totalSymbols?: number;
        topLevelSymbols?: number;
        empty?: SemanticEmptyState;
      }
    | { kind: 'empty'; category: SemanticEmptyCategory; reason: string };
  pagination?: unknown;
  warnings?: string[];
  hints?: string[];
};
