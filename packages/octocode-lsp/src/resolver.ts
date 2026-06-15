import { nativeBinding } from './native.js';
import type { ExactPosition, FuzzyPosition } from './types.js';

export class SymbolResolutionError extends Error {
  constructor(
    public readonly symbolName: string,
    public readonly lineHint: number,
    public readonly reason: string,
    public readonly searchRadius = 5
  ) {
    super(
      `Could not find symbol '${symbolName}' at or near line ${lineHint}. ${reason}`
    );
    this.name = 'SymbolResolutionError';
  }
}

interface SymbolResolverConfig {
  lineSearchRadius?: number;
}

interface ResolvedSymbol {
  position: ExactPosition;
  foundAtLine: number;
  lineOffset: number;
  lineContent: string;
}

function normalizeResolvedSymbol(value: unknown): ResolvedSymbol {
  const record = value as {
    position: ExactPosition;
    foundAtLine?: number;
    found_at_line?: number;
    lineOffset?: number;
    line_offset?: number;
    lineContent?: string;
    line_content?: string;
  };
  return {
    position: record.position,
    foundAtLine: record.foundAtLine ?? record.found_at_line ?? 0,
    lineOffset: record.lineOffset ?? record.line_offset ?? 0,
    lineContent: record.lineContent ?? record.line_content ?? '',
  };
}

function toSymbolResolutionError(
  error: unknown,
  fuzzy: FuzzyPosition,
  searchRadius = 5
): SymbolResolutionError {
  if (error instanceof SymbolResolutionError) return error;
  const reason = error instanceof Error ? error.message : String(error);
  return new SymbolResolutionError(
    fuzzy.symbolName,
    fuzzy.lineHint ?? 0,
    reason,
    searchRadius
  );
}

export async function resolveSymbolPosition(
  filePath: string,
  symbolName: string,
  lineHint?: number,
  orderHint?: number
): Promise<ResolvedSymbol>;
export function resolveSymbolPosition(
  content: string,
  fuzzy: FuzzyPosition
): ResolvedSymbol;
export function resolveSymbolPosition(
  fileOrContent: string,
  fuzzyOrSymbolName: FuzzyPosition | string,
  lineHint?: number,
  orderHint?: number
): Promise<ResolvedSymbol> | ResolvedSymbol {
  if (typeof fuzzyOrSymbolName === 'string') {
    const fuzzy = {
      symbolName: fuzzyOrSymbolName,
      lineHint,
      orderHint,
    };
    try {
      return Promise.resolve(
        normalizeResolvedSymbol(
          nativeBinding.resolvePosition(fileOrContent, fuzzy)
        )
      );
    } catch (error) {
      return Promise.reject(toSymbolResolutionError(error, fuzzy));
    }
  }
  try {
    return normalizeResolvedSymbol(
      nativeBinding.resolvePositionFromContent(fileOrContent, fuzzyOrSymbolName)
    );
  } catch (error) {
    throw toSymbolResolutionError(error, fuzzyOrSymbolName);
  }
}

export class SymbolResolver {
  readonly lineSearchRadius: number;

  constructor(config?: SymbolResolverConfig) {
    this.lineSearchRadius = config?.lineSearchRadius ?? 5;
  }

  async resolvePosition(
    filePath: string,
    fuzzy: FuzzyPosition
  ): Promise<ResolvedSymbol> {
    try {
      return normalizeResolvedSymbol(
        nativeBinding.resolvePosition(filePath, fuzzy)
      );
    } catch (error) {
      throw toSymbolResolutionError(error, fuzzy, this.lineSearchRadius);
    }
  }

  resolvePositionFromContent(
    content: string,
    fuzzy: FuzzyPosition
  ): ResolvedSymbol {
    try {
      return normalizeResolvedSymbol(
        nativeBinding.resolvePositionFromContent(content, fuzzy)
      );
    } catch (error) {
      throw toSymbolResolutionError(error, fuzzy, this.lineSearchRadius);
    }
  }
}
