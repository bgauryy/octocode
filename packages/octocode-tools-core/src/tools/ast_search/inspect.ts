import { createHash } from 'node:crypto';
import { open, stat } from 'node:fs/promises';
import { resolve } from 'node:path';
import type { GraphFacts } from '@octocodeai/octocode-engine';
import { contextUtils } from '../../utils/contextUtils.js';
import type { AstSearchQuery } from '@octocodeai/octocode-core/schema';
import {
  decodeGraphFactsJson,
  decodeGraphScanResult,
} from '../../graph/scanContract.js';

type SyntaxQuery = Extract<AstSearchQuery, { treeKind: 'syntax' }>;
type SymbolsQuery = Extract<AstSearchQuery, { operation: 'symbols' }>;
const MAX_SOURCE_BYTES = 1_000_000;

async function readBounded(path: string): Promise<string | undefined> {
  const handle = await open(path, 'r');
  try {
    const buffer = Buffer.alloc(MAX_SOURCE_BYTES + 1);
    let offset = 0;
    while (offset < buffer.length) {
      const { bytesRead } = await handle.read(
        buffer,
        offset,
        buffer.length - offset,
        null
      );
      if (!bytesRead) break;
      offset += bytesRead;
    }
    return offset > MAX_SOURCE_BYTES
      ? undefined
      : buffer.subarray(0, offset).toString('utf8');
  } finally {
    await handle.close();
  }
}

function digest(value: unknown): string {
  return createHash('sha256').update(JSON.stringify(value)).digest('hex');
}

function continuation(query: object) {
  return { tool: 'astSearch', query };
}

function restart(query: SyntaxQuery | SymbolsQuery, snapshot: string) {
  const { snapshot: _snapshot, ...scope } = query;
  return {
    status: 'error' as const,
    errorCode: 'ast.snapshot.changed',
    error:
      'The source or query changed, or this continuation omitted its snapshot. Discard earlier pages and restart.',
    snapshot,
    complete: false,
    next: {
      restart: continuation({
        ...scope,
        ...(query.operation === 'symbols' ? { page: 1 } : { nodeOffset: 0 }),
      }),
    },
  };
}

function sourceLimit(path: string) {
  return {
    status: 'error' as const,
    path,
    errorCode: 'ast.source.limit',
    error: 'Source exceeds the native parser byte limit.',
    complete: false,
    terminalLimit: true,
  };
}

export async function inspectSyntax(query: SyntaxQuery) {
  const content = await readBounded(query.path);
  if (content === undefined) return sourceLimit(query.path);
  const snapshot = digest([query.path, content, query.namedOnly]);
  if (query.nodeOffset > 0 && query.snapshot !== snapshot)
    return restart(query, snapshot);
  const result = await contextUtils.inspectSyntaxTree(
    content,
    query.path,
    query
  );
  const hasMore = result.nextOffset !== undefined;
  const complete = result.status === 'ok' && !hasMore;
  return {
    ...result,
    operation: 'tree',
    treeKind: 'syntax',
    path: query.path,
    ...(result.status === 'ok'
      ? { status: undefined }
      : { status: 'error' as const, errorCode: `ast.syntax.${result.status}` }),
    snapshot,
    complete,
    isPartial: !complete,
    ...(hasMore
      ? {
          next: {
            nextPage: continuation({
              ...query,
              snapshot,
              nodeOffset: result.nextOffset,
            }),
          },
        }
      : {}),
    ...(!complete && !hasMore ? { terminalLimit: true } : {}),
  };
}

export async function inspectSymbols(query: SymbolsQuery) {
  const isFile = (await stat(query.path)).isFile();
  let entries: Array<{ path: string; facts: GraphFacts }>;
  let truncated = false;
  let filesSkipped = 0;
  let scanDiagnostics: Array<{ path: string; message: string }> = [];
  if (isFile) {
    const content = await readBounded(query.path);
    if (content === undefined) return sourceLimit(query.path);
    const raw = contextUtils.extractGraphFacts(content, query.path);
    if (raw === null)
      return {
        status: 'error' as const,
        errorCode: 'ast.symbols.unsupported',
        path: query.path,
        complete: false,
        error:
          'No native declaration extractor supports this source. Inspect its syntax tree or exact content.',
      };
    const decoded = decodeGraphFactsJson<GraphFacts>(raw);
    if (decoded.ok === true) {
      entries = [{ path: query.path, facts: decoded.parsed }];
    } else {
      entries = [];
      filesSkipped = 1;
      scanDiagnostics = [
        { path: query.path, message: `${decoded.code}: ${decoded.message}` },
      ];
    }
  } else {
    const result = await contextUtils.scanGraphFacts({
      path: query.path,
      maxFiles: query.maxFiles,
      maxFileBytes: MAX_SOURCE_BYTES,
      excludeDir: query.excludeDir,
    });
    const decoded = decodeGraphScanResult(result);
    entries = decoded.entries.map(entry => ({
      path: resolve(query.path, entry.relativePath),
      facts: entry.parsed as GraphFacts,
    }));
    truncated = result.truncated;
    filesSkipped = decoded.filesSkipped;
    scanDiagnostics = decoded.diagnostics.map(diagnostic => ({
      path: resolve(query.path, diagnostic.file),
      message:
        diagnostic.code === 'scan-skip'
          ? diagnostic.message
          : `${diagnostic.code}: ${diagnostic.message}`,
    }));
  }
  entries.sort((a, b) => a.path.localeCompare(b.path));
  const diagnostics = [
    ...scanDiagnostics,
    ...entries.flatMap(entry =>
      entry.facts.diagnostics.map(message => ({ path: entry.path, message }))
    ),
  ];
  const declarations = entries
    .flatMap(entry =>
      entry.facts.declarations.map(declaration => ({
        ...declaration,
        path: entry.path,
      }))
    )
    .filter(
      declaration =>
        (query.name === undefined || declaration.name.includes(query.name)) &&
        (query.kinds === undefined || query.kinds.includes(declaration.kind))
    );
  const snapshot = digest([
    query.path,
    query.name,
    query.kinds,
    query.excludeDir,
    query.maxFiles,
    declarations,
    diagnostics,
    truncated,
    filesSkipped,
  ]);
  if (query.page > 1 && query.snapshot !== snapshot)
    return restart(query, snapshot);
  const offset = (query.page - 1) * query.pageSize;
  const hasMore = offset + query.pageSize < declarations.length;
  const incompleteScan =
    truncated ||
    filesSkipped > 0 ||
    diagnostics.some(
      diagnostic =>
        !diagnostic.message.startsWith(
          'tree-sitter graph facts are syntax-only;'
        )
    );
  return {
    operation: 'symbols',
    path: query.path,
    snapshot,
    declarations: declarations.slice(offset, offset + query.pageSize),
    totalDeclarations: declarations.length,
    filesScanned: entries.length,
    filesSkipped,
    diagnostics,
    complete: !hasMore && !incompleteScan,
    isPartial: hasMore || incompleteScan,
    ...(declarations.length === 0 && !incompleteScan
      ? { status: 'empty' as const }
      : {}),
    ...(incompleteScan ? { terminalLimit: true } : {}),
    pagination: {
      currentPage: query.page,
      totalPages: Math.max(1, Math.ceil(declarations.length / query.pageSize)),
      hasMore,
    },
    ...(hasMore
      ? {
          next: {
            nextPage: continuation({
              ...query,
              snapshot,
              page: query.page + 1,
            }),
          },
        }
      : {}),
  };
}
