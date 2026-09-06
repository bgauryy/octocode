import { open, stat } from 'node:fs/promises';
import {
  SymbolResolver,
  SymbolResolutionError,
} from '@octocodeai/octocode-engine/lsp/resolver';
import { toUri } from '@octocodeai/octocode-engine/lsp/uri';
import type {
  ExactPosition,
  LSPRange,
} from '@octocodeai/octocode-engine/lsp/types';
import { validateToolPath } from '../../../utils/file/toolHelpers.js';
import { LSP_ERROR_CODES } from '@octocodeai/octocode-engine/lsp/lspErrorCodes';
import type {
  DocumentSymbolsSemanticQuery,
  SymbolAnchoredSemanticQuery,
  ResolvedSymbol,
} from './semanticTypes.js';

export type FileAnchor = {
  uri: string;
  absolutePath: string;
  content: string;
};

export type SymbolAnchor = FileAnchor & {
  resolvedSymbol: ResolvedSymbol;
};

export type AnchorResolutionResult<T> =
  { ok: true; value: T } | { ok: false; error: Record<string, unknown> };

const MAX_SEMANTIC_SOURCE_BYTES = 1_000_000;
const SOURCE_LIMIT_MESSAGE =
  '[lspSourceTooLarge] Semantic source exceeds 1000000 bytes; choose a smaller source file.';

async function readBoundedSource(path: string): Promise<string> {
  const file = await open(path, 'r');
  try {
    const info = await file.stat();
    if (!info.isFile())
      throw new Error('Semantic source must be a regular file.');
    if (info.size > MAX_SEMANTIC_SOURCE_BYTES)
      throw new Error(SOURCE_LIMIT_MESSAGE);
    // Read at most one sentinel byte beyond the bound, including when a file
    // grows between stat and read.
    const buffer = Buffer.alloc(MAX_SEMANTIC_SOURCE_BYTES + 1);
    let offset = 0;
    while (offset < buffer.length) {
      const { bytesRead } = await file.read(
        buffer,
        offset,
        buffer.length - offset,
        null
      );
      if (bytesRead === 0) break;
      offset += bytesRead;
    }
    if (offset > MAX_SEMANTIC_SOURCE_BYTES)
      throw new Error(SOURCE_LIMIT_MESSAGE);
    return buffer.toString('utf8', 0, offset);
  } finally {
    await file.close();
  }
}

export async function resolveFileAnchor(
  query: { uri?: string },
  toolName: string
): Promise<AnchorResolutionResult<FileAnchor>> {
  const uri = query.uri;
  const pathValidation = validateToolPath({ ...query, path: uri }, toolName);
  if (!pathValidation.isValid) {
    return {
      ok: false,
      error: pathValidation.errorResult as Record<string, unknown>,
    };
  }

  const absolutePath = pathValidation.sanitizedPath;
  // Stat first so a missing path or a directory produces an actionable message
  // instead of a raw, confusing "EISDIR: illegal operation on a directory" or
  // "ENOENT" surfaced verbatim. LSP semantics operate on a single file.
  try {
    const stats = await stat(absolutePath);
    if (!stats.isFile()) {
      return {
        ok: false,
        error: {
          status: 'error',
          error: `Path is not a regular source file: ${absolutePath}.`,
          errorType: 'not_a_file',
          errorCode: LSP_ERROR_CODES.LSP_REQUEST_FAILED,
          hints: [
            'Choose a source file, or use workspaceSymbol with symbolName.',
          ],
        },
      };
    }
    if (stats.size > MAX_SEMANTIC_SOURCE_BYTES) {
      return {
        ok: false,
        error: {
          status: 'error',
          error: SOURCE_LIMIT_MESSAGE,
          errorType: 'source_limit',
          errorCode: LSP_ERROR_CODES.LSP_REQUEST_FAILED,
        },
      };
    }
  } catch {
    return {
      ok: false,
      error: {
        status: 'error',
        error: `File not found: ${absolutePath}.`,
        errorType: 'file_not_found',
        errorCode: LSP_ERROR_CODES.LSP_REQUEST_FAILED,
        hints: ['Use localSearch with operation:"files" to resolve the path.'],
      },
    };
  }

  try {
    return {
      ok: true,
      value: {
        uri: toUri(absolutePath),
        absolutePath,
        content: await readBoundedSource(absolutePath),
      },
    };
  } catch (error) {
    return {
      ok: false,
      error: {
        status: 'error',
        error: error instanceof Error ? error.message : String(error),
        errorType:
          error instanceof Error &&
          error.message.includes('[lspSourceTooLarge]')
            ? 'source_limit'
            : 'file_not_found',
        errorCode: LSP_ERROR_CODES.LSP_REQUEST_FAILED,
        hints: [`Could not read file: ${uri ?? '<missing>'}`],
      },
    };
  }
}

export async function resolveSymbolAnchor(
  query: SymbolAnchoredSemanticQuery | DocumentSymbolsSemanticQuery,
  toolName: string
): Promise<AnchorResolutionResult<SymbolAnchor>> {
  const file = await resolveFileAnchor(query, toolName);
  if (file.ok === false) return file;

  if (query.type === 'documentSymbols') {
    return {
      ok: false,
      error: {
        status: 'error',
        error: 'documentSymbols is file-level and does not use a symbol anchor',
      },
    };
  }

  const resolver = new SymbolResolver();
  try {
    const resolved = resolver.resolvePositionFromContent(file.value.content, {
      symbolName: query.symbolName,
      lineHint: query.lineHint,
      orderHint: query.orderHint ?? 0,
    });

    const escapedName = query.symbolName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    // Match the native resolver's Unicode identifier boundaries; JavaScript's
    // ASCII word boundary miscounts Unicode names and substrings inside them.
    const identifierContinue = '[\\p{ID_Continue}$\\u200C\\u200D]';
    const occurrenceRegex = new RegExp(
      `(?<!${identifierContinue})${escapedName}(?!${identifierContinue})`,
      'gu'
    );
    const totalOccurrences = (file.value.content.match(occurrenceRegex) ?? [])
      .length;
    // The resolver searches within a radius around the hint — with multiple
    // same-named occurrences, ANY nonzero deviation means a stale hint could
    // have bound a neighboring occurrence, not the intended one. Surface
    // both the flag and the raw deviation instead of resolving silently
    // under full confidence (the old threshold of >3 left deviations 1-3 —
    // well inside the radius-5 search — silently unflagged). No hint → no
    // deviation to reason about.
    const lineDeviation =
      query.lineHint !== undefined
        ? Math.abs(resolved.foundAtLine - query.lineHint)
        : undefined;
    const isAmbiguous =
      totalOccurrences > 1 && lineDeviation !== undefined && lineDeviation > 0
        ? true
        : undefined;

    return {
      ok: true,
      value: {
        ...file.value,
        resolvedSymbol: {
          name: query.symbolName,
          uri: file.value.uri,
          range: rangeFromPosition(resolved.position),
          foundAtLine: resolved.foundAtLine,
          orderHint: query.orderHint,
          position: resolved.position,
          ...(isAmbiguous && { isAmbiguous }),
          ...(lineDeviation !== undefined && lineDeviation > 0
            ? { lineDeviation }
            : {}),
        },
      },
    };
  } catch (error) {
    if (error instanceof SymbolResolutionError) {
      return {
        ok: false,
        error: {
          status: 'empty',
          error: error.message,
          errorType: 'symbol_not_found',
          errorCode: LSP_ERROR_CODES.SYMBOL_NOT_FOUND,
          searchRadius: error.searchRadius,
          hints: [
            `Symbol "${query.symbolName}" was not found near line ${query.lineHint}.`,
            'Run localSearch with operation:"text" and the exact symbol name to refresh lineHint, then retry.',
          ],
        },
      };
    }
    throw error;
  }
}

function rangeFromPosition(position: ExactPosition): LSPRange {
  return {
    start: position,
    end: {
      line: position.line,
      character: position.character,
    },
  };
}
