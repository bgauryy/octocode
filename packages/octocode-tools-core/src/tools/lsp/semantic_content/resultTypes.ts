import type {
  ConsumerWarmupStats,
  ReferenceCoverage,
  CompactLocation,
  CompactResolvedSymbol,
  LspSemanticEnvelope,
  SemanticEmptyState,
} from '../shared/semanticTypes.js';
import type { ItemPagination } from '../../../scheme/pagination.js';
import type { BulkToolOutput } from '../../../types/toolOutput.js';
import type { ReferencesByFile } from '@octocodeai/octocode-engine/lsp/types';
import type { CompactSymbol } from './semanticFileOps/documentSymbols.js';

// Public rows add presentation paths to the shared semantic evidence contract.

interface LspLocation extends CompactLocation {
  absolutePath?: string;
  path?: string;
}

interface LspResolvedSymbol extends CompactResolvedSymbol {
  absolutePath?: string;
  path?: string;
}

interface LspReferencesByFile extends ReferencesByFile {
  absolutePath?: string;
  path?: string;
}

// Row variants (LocationRow, CompactSymbolRow, …) are plain strings.
type LspSemanticPayload =
  | { kind: 'definition'; locations: Array<LspLocation | string> }
  | { kind: 'typeDefinition'; locations: Array<LspLocation | string> }
  | {
      kind: 'implementation';
      locations: Array<LspLocation | string>;
      warmup?: ConsumerWarmupStats;
    }
  | {
      kind: 'references';
      locations?: Array<LspLocation | string>;
      byFile?: Array<LspReferencesByFile | string>;
      totalReferences: number;
      coverage?: ReferenceCoverage;
      totalFiles: number;
      definitionOnly?: boolean;
      warmup?: ConsumerWarmupStats;
      empty?: SemanticEmptyState;
    }
  | Extract<
      LspSemanticEnvelope['payload'],
      { kind: 'callers' | 'callees' | 'callHierarchy' }
    >
  | {
      kind: 'documentSymbols';
      symbols: Array<CompactSymbol | string>;
      diagnostics?: Array<{ code: 'parseRecovery'; message: string }>;
      totalSymbols?: number;
      topLevelSymbols?: number;
      empty?: SemanticEmptyState;
    }
  | Extract<
      LspSemanticEnvelope['payload'],
      { kind: 'workspaceSymbol' | 'typeHierarchy' | 'diagnostic' }
    >
  | Extract<LspSemanticEnvelope['payload'], { kind: 'hover' | 'empty' }>;

export interface LspSearchData extends Omit<
  LspSemanticEnvelope,
  'lsp' | 'payload' | 'pagination' | 'summary'
> {
  absolutePath?: string;
  path?: string;
  resolvedSymbol?: LspResolvedSymbol;
  // Omitted on early-return paths (e.g. symbolNotFound) where the LSP server is
  // never engaged; present on any path that reached a provider.
  lsp?: LspSemanticEnvelope['lsp'];
  payload: LspSemanticPayload;
  pagination?: ItemPagination & { snapshot?: string };
  snapshot?: { expected?: string; actual?: string };
  summary?: Record<string, unknown>;
}

export type LspSearchOutput = BulkToolOutput<LspSearchData>;
