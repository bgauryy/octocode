import type { ExactPosition, LSPRange } from 'octocode-lsp/types';

export const LSP_GET_SEMANTIC_CONTENT_TOOL_NAME = 'lspGetSemanticContent';

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

export type LspGetSemanticContentQuery =
  | SymbolAnchoredSemanticQuery
  | DocumentSymbolsSemanticQuery;

export type ResolvedSymbol = {
  name: string;
  uri: string;
  range: LSPRange;
  foundAtLine: number;
  orderHint?: number;
  position: ExactPosition;
};

// Envelope variant: range/position are 0-based internals derived from the
// same location as foundAtLine (1-based) — emit only the agent-facing facts.
export type CompactResolvedSymbol = {
  name: string;
  uri: string;
  foundAtLine: number;
  orderHint?: number;
};

export function compactResolvedSymbol(
  symbol: ResolvedSymbol
): CompactResolvedSymbol {
  return {
    name: symbol.name,
    uri: symbol.uri,
    foundAtLine: symbol.foundAtLine,
    ...(symbol.orderHint !== undefined && { orderHint: symbol.orderHint }),
  };
}

// Envelope variant of CodeSnippet: the 0-based `range` is dropped — content
// is line-prefixed and displayRange is 1-based, which is what agents chain on.
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

export type LspEvidence = {
  confidence: 'high' | 'medium' | 'low';
  complete: boolean;
  reason?: string;
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
  evidence: LspEvidence;
  summary?: unknown;
  payload:
    | { kind: 'definition'; locations: Array<CompactLocation | string> }
    | {
        kind: 'references';
        // groupByFile=true emits byFile INSTEAD OF the flat locations list.
        locations?: Array<CompactLocation | string>;
        byFile?: unknown[];
        totalReferences: number;
        totalFiles: number;
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
      }
    | { kind: 'hover'; markdown?: string; text?: string; range?: LSPRange }
    | { kind: 'typeDefinition'; locations: Array<CompactLocation | string> }
    | { kind: 'implementation'; locations: Array<CompactLocation | string> }
    | {
        kind: 'documentSymbols';
        symbols: unknown[];
        totalSymbols?: number;
        topLevelSymbols?: number;
      }
    | { kind: 'empty'; reason: string };
  pagination?: unknown;
  warnings?: string[];
  hints?: string[];
};
