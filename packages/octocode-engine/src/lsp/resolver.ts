import { nativeBinding } from './native.js';
import type { FuzzyPosition } from './types.js';

type ResolvedSymbol = ReturnType<typeof nativeBinding.resolvePosition>;

export class SymbolResolutionError extends Error {
  // The native resolver owns its fixed search radius.
  readonly searchRadius = 5;

  constructor(
    public readonly symbolName: string,
    public readonly lineHint: number,
    public readonly reason: string
  ) {
    super(
      `Could not find symbol '${symbolName}' at or near line ${lineHint}.${reason ? ` ${reason}` : ''}`
    );
    this.name = 'SymbolResolutionError';
  }
}

function toSymbolResolutionError(error: unknown, fuzzy: FuzzyPosition): Error {
  if (error instanceof SymbolResolutionError) return error;
  const raw = error instanceof Error ? error.message : String(error);
  if (/\[lsp(?:PositionTimeout|SourceTooLarge)\]/.test(raw)) {
    return error instanceof Error ? error : new Error(raw);
  }
  // The native resolver already emits the canonical "Could not find symbol …"
  // sentence; strip it so the wrapper message doesn't repeat it.
  const prefix = `Could not find symbol '${fuzzy.symbolName}' at or near line ${fuzzy.lineHint ?? 0}`;
  const reason = raw.startsWith(prefix)
    ? raw.slice(prefix.length).replace(/^[.\s]+/, '')
    : raw;
  return new SymbolResolutionError(
    fuzzy.symbolName,
    fuzzy.lineHint ?? 0,
    reason
  );
}

export class SymbolResolver {
  async resolvePosition(
    filePath: string,
    fuzzy: FuzzyPosition
  ): Promise<ResolvedSymbol> {
    try {
      return nativeBinding.resolvePosition(filePath, fuzzy);
    } catch (error) {
      throw toSymbolResolutionError(error, fuzzy);
    }
  }

  resolvePositionFromContent(
    content: string,
    fuzzy: FuzzyPosition
  ): ResolvedSymbol {
    try {
      return nativeBinding.resolvePositionFromContent(content, fuzzy);
    } catch (error) {
      throw toSymbolResolutionError(error, fuzzy);
    }
  }
}
