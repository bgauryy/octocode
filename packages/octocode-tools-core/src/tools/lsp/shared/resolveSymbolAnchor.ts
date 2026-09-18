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
        hints: ['Use astSearch with operation:"files" to resolve the path.'],
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

  if (query.operation === 'documentSymbols') {
    return {
      ok: false,
      error: {
        status: 'error',
        error: 'documentSymbols is file-level and does not use a symbol anchor',
      },
    };
  }

  if (query.position) {
    const position = query.position;
    const line = file.value.content
      .split('\n')
      [position.line]?.replace(/\r$/, '');
    if (line === undefined || position.character > line.length) {
      return {
        ok: false,
        error: {
          status: 'empty',
          error: 'The supplied position is outside the source.',
          errorType: 'anchor_drift',
          reanchor: { uri: file.value.uri, position },
        },
      };
    }
    // An LSP position can address quoted keys, operators, or module paths.
    // The identifier is only a discovery hint; the provider owns its meaning.
    const hint = identifierAtPosition(line, position.character);
    return {
      ok: true,
      value: {
        ...file.value,
        resolvedSymbol: {
          ...(hint && { name: hint.symbolName, orderHint: hint.orderHint }),
          uri: file.value.uri,
          range: rangeFromPosition(position),
          foundAtLine: position.line + 1,
          position,
        },
      },
    };
  }
  const { symbolName, lineHint, orderHint } = query;
  if (!symbolName || lineHint === undefined) {
    return {
      ok: false,
      error: {
        status: 'error',
        error:
          'A semantic anchor requires either position or symbolName with lineHint.',
        errorType: 'anchor_missing',
      },
    };
  }
  try {
    const resolver = new SymbolResolver();
    const resolved = resolver.resolvePositionFromContent(file.value.content, {
      symbolName,
      lineHint,
      orderHint: orderHint ?? 0,
    });

    const lineDeviation = Math.abs(resolved.foundAtLine - lineHint);
    const sameLineOccurrences = countLineOccurrences(
      file.value.content,
      symbolName,
      lineHint
    );
    if (
      lineDeviation > 0 ||
      (orderHint === undefined && sameLineOccurrences > 1)
    ) {
      return {
        ok: false,
        error: {
          status: 'empty',
          error:
            'The semantic anchor drifted or is ambiguous; refresh the source anchor and retry.',
          errorType: 'anchor_drift',
          reanchor: {
            uri: file.value.uri,
            symbolName,
            lineHint: resolved.foundAtLine,
            orderHint: orderHint ?? 0,
            position: resolved.position,
          },
          ...(lineDeviation > 0 ? { lineDeviation } : {}),
        },
      };
    }

    return {
      ok: true,
      value: {
        ...file.value,
        resolvedSymbol: {
          name: symbolName,
          uri: file.value.uri,
          range: rangeFromPosition(resolved.position),
          foundAtLine: resolved.foundAtLine,
          orderHint,
          position: resolved.position,
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
            `Symbol "${symbolName}" was not found near line ${lineHint}.`,
            'Run localSearch with searchText and the exact symbol name to refresh lineHint, then retry.',
          ],
        },
      };
    }
    throw error;
  }
}

function identifierAtPosition(
  text: string,
  character: number
):
  | {
      symbolName: string;
      orderHint: number;
    }
  | undefined {
  const tokens = /[$_\p{ID_Start}][$\u200C\u200D\p{ID_Continue}]*/gu;
  for (const match of text.matchAll(tokens)) {
    const start = match.index ?? 0;
    const end = start + match[0].length;
    if (character >= start && character < end) {
      const orderHint = [...text.slice(0, start).matchAll(tokens)].filter(
        previous => previous[0] === match[0]
      ).length;
      return {
        symbolName: match[0],
        orderHint,
      };
    }
  }
  return undefined;
}

function countLineOccurrences(
  content: string,
  symbolName: string,
  lineHint: number
): number {
  const line = content.split('\n')[lineHint - 1]?.replace(/\r$/, '') ?? '';
  const escaped = symbolName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const regex = new RegExp(
    `(?<![\\p{ID_Continue}$\\u200C\\u200D])${escaped}(?![\\p{ID_Continue}$\\u200C\\u200D])`,
    'gu'
  );
  return [...line.matchAll(regex)].length;
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
