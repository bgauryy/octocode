import path from 'node:path';
import { readFile } from 'node:fs/promises';
import type { LSPClient } from '@octocodeai/octocode-engine/lsp/client';
import {
  acquirePooledClientDetailed,
  isLanguageServerAvailable,
} from '@octocodeai/octocode-engine/lsp/manager';
import { resolveImportAliasDefinitions } from '@octocodeai/octocode-engine/lsp/resolver';
import { resolveWorkspaceRootForFile } from '@octocodeai/octocode-engine/lsp/workspaceRoot';
import type {
  LspSemanticEnvelope,
  DiagnosticSemanticQuery,
  WorkspaceSymbolSemanticQuery,
} from '../../shared/semanticTypes.js';
import {
  DEFAULT_SYMBOLS_PER_PAGE,
  paginateItems,
} from '../semanticEnvelopes.js';
import { symbolKindName } from '../semanticPresentation.js';
import {
  lspErrorMessage,
  resolveWorkspaceSymbolAnchor,
  throwLspUnavailable,
} from './anchor.js';
import type { CompactSymbol } from './documentSymbols.js';

type CompactWorkspaceSymbol = CompactSymbol & { uri: string };

type DiagnosticItem = {
  severity?: number;
  message: string;
  line: number;
  endLine: number;
  character: number;
  code?: string | number;
  source?: string;
};

export async function getWorkspaceSymbols(
  query: WorkspaceSymbolSemanticQuery
): Promise<LspSemanticEnvelope | Record<string, unknown>> {
  const symbolQuery = query.symbolName ?? '';
  const workspaceRoot = path.resolve(query.workspaceRoot ?? process.cwd());

  // workspace/symbol is project-wide, but language-server selection is
  // extension-based. Use an explicit uri when provided; otherwise pick a
  // representative source file under the workspace root.
  const anchorFile = await resolveWorkspaceSymbolAnchor(query, workspaceRoot);
  const serverAvailable = await isLanguageServerAvailable(
    anchorFile,
    workspaceRoot
  );
  if (!serverAvailable) {
    throwLspUnavailable(anchorFile, 'workspaceSymbol');
  }

  const clientResult = await acquirePooledClientDetailed(
    workspaceRoot,
    anchorFile
  );
  if (clientResult.ok === false) {
    throwLspUnavailable(anchorFile, 'workspaceSymbol', clientResult);
  }
  const client = clientResult.client;

  if (!client.hasCapability('workspaceSymbolProvider')) {
    return {
      type: 'workspaceSymbol',
      uri: anchorFile,
      lsp: { serverAvailable: true, provider: 'workspaceSymbolProvider' },
      payload: {
        kind: 'empty',
        category: 'unsupportedOperation',
        reason: 'workspaceSymbolProvider unsupported',
      },
    } satisfies LspSemanticEnvelope;
  }

  let raw: unknown[];
  try {
    if (path.extname(anchorFile)) {
      await client.openDocument(anchorFile);
    }
    raw = await client.workspaceSymbol(symbolQuery);
  } catch (error) {
    return {
      type: 'workspaceSymbol',
      uri: anchorFile,
      lsp: { serverAvailable: true, provider: 'workspaceSymbolProvider' },
      payload: {
        kind: 'empty',
        category: 'unsupportedOperation',
        reason: `workspaceSymbolProvider failed: ${lspErrorMessage(error)}`,
      },
    } satisfies LspSemanticEnvelope;
  }
  const symbols = await preferDefinitionLocations(
    client,
    compactWorkspaceSymbols(raw)
  );
  const { pageItems, pagination } = paginateItems(
    symbols,
    query.page ?? 1,
    query.itemsPerPage ?? DEFAULT_SYMBOLS_PER_PAGE
  );

  return {
    type: 'workspaceSymbol',
    uri: anchorFile,
    lsp: { serverAvailable: true, provider: 'workspaceSymbolProvider' },
    summary: { query: symbolQuery, totalSymbols: symbols.length },
    payload:
      symbols.length > 0
        ? {
            kind: 'workspaceSymbol',
            query: symbolQuery,
            symbols: pageItems,
            totalSymbols: symbols.length,
          }
        : {
            kind: 'empty',
            category: 'noWorkspaceSymbols',
            reason: `workspaceSymbolProvider returned no symbols for query "${symbolQuery}"`,
          },
    pagination,
  } satisfies LspSemanticEnvelope;
}

// workspace/symbol servers often index the IMPORT BINDING (alias site) rather
// than the defining declaration — observed: querying a function returned only
// its import line in a consumer file, not the definition. For small result
// sets, chase each hit through gotoDefinition and keep the definition
// location; failures keep the server-reported location (best-effort).
const MAX_DEFINITION_RESOLVE = 10;

async function preferDefinitionLocations(
  client: LSPClient,
  symbols: CompactWorkspaceSymbol[]
): Promise<CompactWorkspaceSymbol[]> {
  if (symbols.length === 0 || symbols.length > MAX_DEFINITION_RESOLVE) {
    return symbols;
  }
  return Promise.all(
    symbols.map(async sym => {
      try {
        if (!sym.uri) return sym;
        const fsPath = sym.uri.startsWith('file://')
          ? decodeURIComponent(sym.uri.slice('file://'.length))
          : sym.uri;
        const content = await readFile(fsPath, 'utf8');
        await client.openDocument(fsPath);
        // gotoDefinition on an import specifier returns the import binding
        // itself — chase aliases to the real definition the same way the
        // anchored definition path does.
        const defs = await resolveImportAliasDefinitions({
          anchorUri: fsPath,
          symbolName: sym.name,
          locations: await client.gotoDefinition(
            fsPath,
            { line: sym.line - 1, character: sym.character },
            content
          ),
        });
        if (!Array.isArray(defs) || defs.length !== 1) return sym;
        const def = defs[0] as {
          uri?: string;
          range?: {
            start?: { line?: number; character?: number };
            end?: { line?: number };
          };
        };
        const defLine = (def.range?.start?.line ?? -1) + 1;
        if (!def.uri || defLine < 1) return sym;
        if (def.uri === sym.uri && defLine === sym.line) return sym;
        return {
          ...sym,
          uri: def.uri,
          line: defLine,
          endLine:
            (def.range?.end?.line ?? def.range?.start?.line ?? defLine - 1) + 1,
          character: def.range?.start?.character ?? sym.character,
        };
      } catch {
        return sym;
      }
    })
  );
}

export function compactWorkspaceSymbols(
  raw: unknown[]
): CompactWorkspaceSymbol[] {
  return raw.flatMap(item => {
    if (!item || typeof item !== 'object') return [];
    const sym = item as Record<string, unknown>;
    const name = typeof sym['name'] === 'string' ? sym['name'] : undefined;
    if (!name) return [];
    const kind = sym['kind'];
    // WorkspaceSymbol has `location.uri + location.range`; SymbolInformation same shape.
    const loc = sym['location'] as Record<string, unknown> | undefined;
    const range = loc?.['range'] as
      | {
          start?: { line?: number; character?: number };
          end?: { line?: number };
        }
      | undefined;
    const uri = typeof loc?.['uri'] === 'string' ? loc['uri'] : '';
    const line = (range?.start?.line ?? 0) + 1;
    const endLine = (range?.end?.line ?? range?.start?.line ?? 0) + 1;
    const containerName =
      typeof sym['containerName'] === 'string'
        ? sym['containerName']
        : undefined;
    return [
      {
        name,
        kind: symbolKindName(kind),
        line,
        character: range?.start?.character ?? 0,
        endLine,
        childCount: 0,
        ...(containerName ? { containerName } : {}),
        uri,
      },
    ];
  });
}

export async function getFileDiagnostics(
  query: DiagnosticSemanticQuery
): Promise<LspSemanticEnvelope | Record<string, unknown>> {
  const uri = query.uri ?? '';
  const workspaceRoot =
    query.workspaceRoot ??
    (uri ? await resolveWorkspaceRootForFile(uri) : process.cwd());

  const serverAvailable = await isLanguageServerAvailable(uri, workspaceRoot);
  if (!serverAvailable) {
    throwLspUnavailable(uri, 'diagnostic');
  }

  const clientResult = await acquirePooledClientDetailed(workspaceRoot, uri);
  if (clientResult.ok === false) {
    throwLspUnavailable(uri, 'diagnostic', clientResult);
  }
  const client = clientResult.client;

  if (!client.hasCapability('diagnosticProvider')) {
    return {
      type: 'diagnostic',
      uri,
      lsp: { serverAvailable: true, provider: 'diagnosticProvider' },
      payload: {
        kind: 'empty',
        category: 'unsupportedOperation',
        reason:
          'diagnosticProvider (pull) unsupported — server uses push (publishDiagnostics) instead',
      },
      warnings: [
        'This server pushes diagnostics via textDocument/publishDiagnostics. ' +
          'Pull diagnostics (type: "diagnostic") require LSP 3.17 pull support. ' +
          'Check server docs to enable it.',
      ],
    } satisfies LspSemanticEnvelope;
  }

  const raw = await client.getDiagnostics(uri);
  const diags = extractDiagnostics(raw);
  const errorCount = diags.filter(d => d.severity === 1).length;
  const warningCount = diags.filter(d => d.severity === 2).length;

  const { pageItems, pagination } = paginateItems(
    diags,
    query.page ?? 1,
    query.itemsPerPage ?? DEFAULT_SYMBOLS_PER_PAGE
  );

  return {
    type: 'diagnostic',
    uri,
    lsp: { serverAvailable: true, provider: 'diagnosticProvider' },
    summary: {
      totalDiagnostics: diags.length,
      errorCount,
      warningCount,
    },
    payload:
      diags.length > 0
        ? {
            kind: 'diagnostic',
            diagnostics: pageItems,
            totalDiagnostics: diags.length,
            errorCount,
            warningCount,
          }
        : {
            kind: 'empty',
            category: 'noDiagnostics',
            reason: 'No diagnostics — file has no errors or warnings',
          },
    pagination,
  } satisfies LspSemanticEnvelope;
}

export function extractDiagnostics(raw: unknown): DiagnosticItem[] {
  // Pull response shape: { kind: "full", items: Diagnostic[] }
  if (raw && typeof raw === 'object') {
    const report = raw as Record<string, unknown>;
    const items = Array.isArray(report['items']) ? report['items'] : [];
    return items.flatMap(item => parseDiagnostic(item));
  }
  return [];
}

export function parseDiagnostic(item: unknown): DiagnosticItem[] {
  if (!item || typeof item !== 'object') return [];
  const d = item as Record<string, unknown>;
  const range = d['range'] as
    | { start?: { line?: number; character?: number }; end?: { line?: number } }
    | undefined;
  const message = typeof d['message'] === 'string' ? d['message'] : '';
  if (!message) return [];
  return [
    {
      severity: typeof d['severity'] === 'number' ? d['severity'] : undefined,
      message,
      line: (range?.start?.line ?? 0) + 1,
      endLine: (range?.end?.line ?? range?.start?.line ?? 0) + 1,
      character: range?.start?.character ?? 0,
      ...(d['code'] !== undefined
        ? { code: d['code'] as string | number }
        : {}),
      ...(typeof d['source'] === 'string' ? { source: d['source'] } : {}),
    },
  ];
}
