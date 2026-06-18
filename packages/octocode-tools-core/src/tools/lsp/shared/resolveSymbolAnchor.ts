import { readFile } from 'node:fs/promises';
import { SymbolResolver, SymbolResolutionError } from 'octocode-lsp/resolver';
import type { ExactPosition, LSPRange } from 'octocode-lsp/types';
import { validateToolPath } from '../../../utils/file/toolHelpers.js';
import { LSP_ERROR_CODES } from 'octocode-lsp/lspErrorCodes';
import type { LspGetSemanticsQuery, ResolvedSymbol } from './semanticTypes.js';

export type FileAnchor = {
  uri: string;
  absolutePath: string;
  content: string;
};

export type SymbolAnchor = FileAnchor & {
  resolvedSymbol: ResolvedSymbol;
};

export type AnchorResolutionResult<T> =
  | { ok: true; value: T }
  | { ok: false; error: Record<string, unknown> };

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
  try {
    return {
      ok: true,
      value: {
        uri: absolutePath,
        absolutePath,
        content: await readFile(absolutePath, 'utf-8'),
      },
    };
  } catch (error) {
    return {
      ok: false,
      error: {
        status: 'error',
        error: error instanceof Error ? error.message : String(error),
        errorType: 'file_not_found',
        errorCode: LSP_ERROR_CODES.LSP_REQUEST_FAILED,
        hints: [`Could not read file: ${uri ?? '<missing>'}`],
      },
    };
  }
}

export async function resolveSymbolAnchor(
  query: LspGetSemanticsQuery,
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

  const resolver = new SymbolResolver({ lineSearchRadius: 5 });
  try {
    const resolved = resolver.resolvePositionFromContent(file.value.content, {
      symbolName: query.symbolName,
      lineHint: query.lineHint,
      orderHint: query.orderHint ?? 0,
    });

    const escapedName = query.symbolName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const occurrenceRegex = new RegExp(`\\b${escapedName}\\b`, 'g');
    const totalOccurrences = (file.value.content.match(occurrenceRegex) ?? [])
      .length;
    const lineDeviation = Math.abs(
      resolved.foundAtLine - (query.lineHint ?? 0)
    );
    const isAmbiguous =
      totalOccurrences > 1 && lineDeviation > 3 ? true : undefined;

    return {
      ok: true,
      value: {
        ...file.value,
        resolvedSymbol: {
          name: query.symbolName,
          uri: file.value.absolutePath,
          range: rangeFromPosition(resolved.position),
          foundAtLine: resolved.foundAtLine,
          orderHint: query.orderHint,
          position: resolved.position,
          ...(isAmbiguous && { isAmbiguous }),
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
            'Run localSearchCode with the exact symbol name to refresh lineHint, then retry.',
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
