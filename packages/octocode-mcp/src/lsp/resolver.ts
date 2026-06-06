import { promises as fs } from 'fs';
import type { FuzzyPosition, ExactPosition } from './types.js';

export class SymbolResolutionError extends Error {
  public readonly symbolName: string;
  public readonly lineHint: number;
  public readonly reason: string;
  public readonly searchRadius: number;

  constructor(
    symbolName: string,
    lineHint: number,
    reason: string,
    searchRadius: number = 5
  ) {
    super(
      `Could not find symbol '${symbolName}' at or near line ${lineHint}. ${reason}`
    );
    this.name = 'SymbolResolutionError';
    this.symbolName = symbolName;
    this.lineHint = lineHint;
    this.reason = reason;
    this.searchRadius = searchRadius;
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

interface QuoteScanState {
  inSingle: boolean;
  inDouble: boolean;
  inTemplate: boolean;

  templateExprDepth: number;
  escaped: boolean;
}

function stepStringScan(
  line: string,
  i: number,
  state: QuoteScanState
): { commentFound: boolean; nextIndex: number } {
  if (state.escaped) {
    state.escaped = false;
    return { commentFound: false, nextIndex: i + 1 };
  }

  const ch = line[i]!;

  if (ch === '\\') {
    state.escaped = true;
    return { commentFound: false, nextIndex: i + 1 };
  }

  if (
    ch === '/' &&
    line[i + 1] === '/' &&
    !state.inSingle &&
    !state.inDouble &&
    !state.inTemplate
  ) {
    return { commentFound: true, nextIndex: i + 1 };
  }

  if (
    state.inTemplate &&
    state.templateExprDepth === 0 &&
    ch === '$' &&
    line[i + 1] === '{'
  ) {
    state.templateExprDepth = 1;
    return { commentFound: false, nextIndex: i + 2 };
  }
  if (state.templateExprDepth > 0) {
    if (ch === '{') state.templateExprDepth++;
    else if (ch === '}') state.templateExprDepth--;
    return { commentFound: false, nextIndex: i + 1 };
  }

  if (ch === "'" && !state.inDouble && !state.inTemplate)
    state.inSingle = !state.inSingle;
  else if (ch === '"' && !state.inSingle && !state.inTemplate)
    state.inDouble = !state.inDouble;
  else if (ch === '`' && !state.inSingle && !state.inDouble)
    state.inTemplate = !state.inTemplate;

  return { commentFound: false, nextIndex: i + 1 };
}

export class SymbolResolver {
  private readonly lineSearchRadius: number;

  constructor(config?: SymbolResolverConfig) {
    this.lineSearchRadius = config?.lineSearchRadius ?? 5;
  }

  async resolvePosition(
    filePath: string,
    fuzzy: FuzzyPosition
  ): Promise<ResolvedSymbol> {
    const content = await fs.readFile(filePath, 'utf-8');
    return this.resolvePositionFromContent(content, fuzzy);
  }

  resolvePositionFromContent(
    content: string,
    fuzzy: FuzzyPosition
  ): ResolvedSymbol {
    const lines = content.split(/\r?\n/);
    const targetLine = fuzzy.lineHint - 1;
    const orderHint = fuzzy.orderHint ?? 0;

    if (targetLine < 0 || targetLine >= lines.length) {
      throw new SymbolResolutionError(
        fuzzy.symbolName,
        fuzzy.lineHint,
        `Line ${fuzzy.lineHint} is out of range (file has ${lines.length} lines)`,
        this.lineSearchRadius
      );
    }

    const exactLine = lines[targetLine];
    if (exactLine !== undefined) {
      const exactResult = this.findSymbolInLine(
        exactLine,
        fuzzy.symbolName,
        orderHint
      );
      if (exactResult !== null) {
        return {
          position: { line: targetLine, character: exactResult },
          foundAtLine: fuzzy.lineHint,
          lineOffset: 0,
          lineContent: exactLine,
        };
      }
    }

    for (let offset = 1; offset <= this.lineSearchRadius; offset++) {
      for (const delta of [-offset, offset]) {
        const searchLine = targetLine + delta;
        if (searchLine >= 0 && searchLine < lines.length) {
          const line = lines[searchLine];
          if (line !== undefined) {
            const result = this.findSymbolInLine(line, fuzzy.symbolName, 0);
            if (result !== null) {
              return {
                position: { line: searchLine, character: result },
                foundAtLine: searchLine + 1,
                lineOffset: delta,
                lineContent: line,
              };
            }
          }
        }
      }
    }

    throw new SymbolResolutionError(
      fuzzy.symbolName,
      fuzzy.lineHint,
      `Symbol not found in target line or within ±${this.lineSearchRadius} lines. Verify the exact symbol name and line number.`,
      this.lineSearchRadius
    );
  }

  private findSymbolInLine(
    line: string,
    symbolName: string,
    orderHint: number
  ): number | null {
    let searchStart = 0;
    let occurrenceCount = 0;

    while (searchStart < line.length) {
      const index = line.indexOf(symbolName, searchStart);
      if (index === -1) return null;

      const isWordBoundaryStart =
        index === 0 || !this.isIdentifierChar(line[index - 1]!);
      const isWordBoundaryEnd =
        index + symbolName.length >= line.length ||
        !this.isIdentifierChar(line[index + symbolName.length]!);

      if (isWordBoundaryStart && isWordBoundaryEnd) {
        if (this.isInsideStringOrComment(line, index)) {
          searchStart = index + 1;
          continue;
        }

        if (occurrenceCount === orderHint) {
          return index;
        }
        occurrenceCount++;
      }

      searchStart = index + 1;
    }

    return null;
  }

  private isInsideStringOrComment(line: string, position: number): boolean {
    const state: QuoteScanState = {
      inSingle: false,
      inDouble: false,
      inTemplate: false,
      templateExprDepth: 0,
      escaped: false,
    };

    for (let i = 0; i < position; ) {
      const step = stepStringScan(line, i, state);
      if (step.commentFound) return true;
      i = step.nextIndex;
    }

    if (state.inTemplate && state.templateExprDepth > 0) return false;

    return state.inSingle || state.inDouble || state.inTemplate;
  }

  private isIdentifierChar(char: string): boolean {
    const c = char.charCodeAt(0);
    return (
      (c >= 48 && c <= 57) ||
      (c >= 65 && c <= 90) ||
      (c >= 97 && c <= 122) ||
      c === 95 ||
      c === 36
    );
  }

  extractContext(
    content: string,
    lineNumber: number,
    contextLines: number
  ): { content: string; startLine: number; endLine: number } {
    const lines = content.split(/\r?\n/);
    const startLine = Math.max(1, lineNumber - contextLines);
    const endLine = Math.min(lines.length, lineNumber + contextLines);

    const contextContent = lines.slice(startLine - 1, endLine).join('\n');

    return {
      content: contextContent,
      startLine,
      endLine,
    };
  }
}

export const defaultResolver = new SymbolResolver({ lineSearchRadius: 5 });

export async function resolveSymbolPosition(
  filePath: string,
  symbolName: string,
  lineHint: number,
  orderHint?: number
): Promise<ResolvedSymbol> {
  return defaultResolver.resolvePosition(filePath, {
    symbolName,
    lineHint,
    orderHint,
  });
}
