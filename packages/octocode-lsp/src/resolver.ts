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
    return Promise.resolve(
      normalizeResolvedSymbol(
        nativeBinding.resolvePosition(fileOrContent, {
          symbolName: fuzzyOrSymbolName,
          lineHint,
          orderHint,
        })
      )
    );
  }
  return normalizeResolvedSymbol(
    nativeBinding.resolvePositionFromContent(fileOrContent, fuzzyOrSymbolName)
  );
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
    return normalizeResolvedSymbol(
      nativeBinding.resolvePosition(filePath, fuzzy)
    );
  }

  resolvePositionFromContent(
    content: string,
    fuzzy: FuzzyPosition
  ): ResolvedSymbol {
    return normalizeResolvedSymbol(
      nativeBinding.resolvePositionFromContent(content, fuzzy)
    );
  }
}
