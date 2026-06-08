import type {
  CodeSnippet,
  ExactPosition,
  LSPRange,
} from '../../../lsp/types.js';

export const LSP_GET_SEMANTIC_CONTENT_TOOL_NAME = 'lspGetSemanticContent';
export const LSP_GET_DIAGNOSTICS_TOOL_NAME = 'lspGetDiagnostics';

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

export type SemanticQueryBase = {
  id?: string;
  type: SemanticContentType;
  uri?: string;
  filePath?: string;
  workspaceRoot?: string;
  page?: number;
  itemsPerPage?: number;
  contextLines?: number;
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

export type LspEvidence = {
  confidence: 'high' | 'medium' | 'low';
  complete: boolean;
  reason?: string;
};

export type LspSemanticEnvelope = {
  type: SemanticContentType;
  uri: string;
  resolvedSymbol?: ResolvedSymbol;
  lsp: {
    serverAvailable: boolean;
    provider?: string;
    source?: string;
  };
  evidence: LspEvidence;
  summary?: unknown;
  payload:
    | { kind: 'definition'; locations: CodeSnippet[] }
    | {
        kind: 'references';
        locations: unknown[];
        byFile?: unknown[];
        totalReferences: number;
        totalFiles: number;
      }
    | {
        kind: 'calls';
        direction: 'incoming' | 'outgoing' | 'both';
        root?: unknown;
        calls: unknown[];
        totalCalls?: number;
        completeness: {
          complete: boolean;
          truncatedByDepth: boolean;
          cycleCount: number;
          failedRequestCount: number;
          dynamicCallsExcluded: true;
        };
      }
    | { kind: 'hover'; markdown?: string; text?: string; range?: LSPRange }
    | { kind: 'typeDefinition'; locations: CodeSnippet[] }
    | { kind: 'implementation'; locations: CodeSnippet[] }
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

export type LspDiagnosticsQuery = {
  id?: string;
  uri?: string;
  filePath?: string;
  workspaceRoot?: string;
  severity?: 'error' | 'warning' | 'information' | 'hint' | 'all';
  source?: string;
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
};
