/**
 * Symbol resolver for LSP tools
 * Resolves fuzzy positions (symbolName + lineHint) to exact positions
 * @module lsp/resolver
 */

import { promises as fs } from 'fs';
import type { FuzzyPosition, ExactPosition } from './types.js';

/**
 * Error thrown when symbol cannot be resolved
 */
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

/**
 * Configuration for symbol resolver
 */
interface SymbolResolverConfig {
  /** Number of lines to search above and below lineHint (default: 5) */
  lineSearchRadius?: number;
}

/**
 * Result of symbol resolution
 */
interface ResolvedSymbol {
  /** Exact position where symbol was found */
  position: ExactPosition;
  /** The line where symbol was found (1-indexed) */
  foundAtLine: number;
  /** Offset from the lineHint (0 if found at exact line) */
  lineOffset: number;
  /** The actual line content */
  lineContent: string;
}

/**
 * Symbol resolver class
 * Finds exact character position from fuzzy position (symbolName + lineHint)
 */
/** Mutable quote/comment scan state for {@link stepStringScan}. */
interface QuoteScanState {
  inSingle: boolean;
  inDouble: boolean;
  inTemplate: boolean;
  /** Nested `${...}` depth — code inside is NOT string text. */
  templateExprDepth: number;
  escaped: boolean;
}

/**
 * Advance the string/comment scanner one character. Mutates `state` and
 * returns whether a line comment started here plus the next index to scan
 * (usually `i + 1`, but `i + 2` when skipping past a `${`).
 */
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

  // Line comment — everything after `//` (outside strings) is a comment
  if (
    ch === '/' &&
    line[i + 1] === '/' &&
    !state.inSingle &&
    !state.inDouble &&
    !state.inTemplate
  ) {
    return { commentFound: true, nextIndex: i + 1 };
  }

  // Template expression tracking: `${...}` contains code, not string text
  if (
    state.inTemplate &&
    state.templateExprDepth === 0 &&
    ch === '$' &&
    line[i + 1] === '{'
  ) {
    state.templateExprDepth = 1;
    return { commentFound: false, nextIndex: i + 2 }; // skip the '{'
  }
  if (state.templateExprDepth > 0) {
    if (ch === '{') state.templateExprDepth++;
    else if (ch === '}') state.templateExprDepth--;
    return { commentFound: false, nextIndex: i + 1 }; // inside ${...}: code
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

  /**
   * Resolve a fuzzy position to an exact position
   *
   * @param filePath - Absolute path to the file
   * @param fuzzy - Fuzzy position with symbolName and lineHint
   * @returns Resolved symbol with exact position
   * @throws SymbolResolutionError if symbol cannot be found
   */
  async resolvePosition(
    filePath: string,
    fuzzy: FuzzyPosition
  ): Promise<ResolvedSymbol> {
    const content = await fs.readFile(filePath, 'utf-8');
    return this.resolvePositionFromContent(content, fuzzy);
  }

  /**
   * Resolve a fuzzy position from content string
   * Useful when content is already loaded
   *
   * @param content - File content as string
   * @param fuzzy - Fuzzy position with symbolName and lineHint
   * @returns Resolved symbol with exact position
   * @throws SymbolResolutionError if symbol cannot be found
   */
  resolvePositionFromContent(
    content: string,
    fuzzy: FuzzyPosition
  ): ResolvedSymbol {
    const lines = content.split(/\r?\n/);
    const targetLine = fuzzy.lineHint - 1; // Convert to 0-indexed
    const orderHint = fuzzy.orderHint ?? 0;

    // Validate line number
    if (targetLine < 0 || targetLine >= lines.length) {
      throw new SymbolResolutionError(
        fuzzy.symbolName,
        fuzzy.lineHint,
        `Line ${fuzzy.lineHint} is out of range (file has ${lines.length} lines)`,
        this.lineSearchRadius
      );
    }

    // Search exact line first
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

    // Search nearby lines (alternating above and below).
    // orderHint is only meaningful for the exact target line (it selects the
    // Nth occurrence on that specific line). When falling back to nearby lines,
    // always pick the first occurrence (orderHint 0) — otherwise a non-zero
    // orderHint causes every single-occurrence nearby line to miss.
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
                foundAtLine: searchLine + 1, // Convert back to 1-indexed
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

  /**
   * Find symbol in a single line, skipping occurrences inside string
   * literals or comments.
   *
   * @param line - Line content
   * @param symbolName - Symbol to find (exact match)
   * @param orderHint - Which occurrence to return (0 = first)
   * @returns Character position or null if not found
   */
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

      // Check for word boundary (symbol should not be part of larger identifier)
      const isWordBoundaryStart =
        index === 0 || !this.isIdentifierChar(line[index - 1]!);
      const isWordBoundaryEnd =
        index + symbolName.length >= line.length ||
        !this.isIdentifierChar(line[index + symbolName.length]!);

      if (isWordBoundaryStart && isWordBoundaryEnd) {
        // Skip matches inside string literals or line comments —
        // the LSP server cannot resolve definitions for text in strings.
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

  /**
   * Determine whether a character position falls inside a string literal
   * or a line comment (`//`).
   *
   * Walks the line left-to-right tracking quote state (`'`, `"`, `` ` ``),
   * handling backslash escapes. This is a lightweight heuristic — it does
   * not handle block comments or regex literals, but those
   * are extremely rare in the single-line symbol-resolution context.
   *
   * @param line - Full line content
   * @param position - 0-based character index to test
   * @returns true if position is inside a string or comment
   * @internal Exported via class for testing
   */
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

    // Inside a template but within a ${...} expression → code context, not string
    if (state.inTemplate && state.templateExprDepth > 0) return false;

    return state.inSingle || state.inDouble || state.inTemplate;
  }

  /**
   * Check if character is a valid identifier character.
   * Uses charCode comparison instead of regex for performance —
   * this is called per-character during symbol boundary checks.
   */
  private isIdentifierChar(char: string): boolean {
    const c = char.charCodeAt(0);
    return (
      (c >= 48 && c <= 57) || // 0-9
      (c >= 65 && c <= 90) || // A-Z
      (c >= 97 && c <= 122) || // a-z
      c === 95 || // _
      c === 36 // $
    );
  }

  /**
   * Extract context lines around a position
   *
   * @param content - File content
   * @param lineNumber - 1-indexed line number
   * @param contextLines - Number of lines before and after
   * @returns Code snippet with context
   */
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

/**
 * Default symbol resolver instance
 */
export const defaultResolver = new SymbolResolver({ lineSearchRadius: 5 });

/**
 * Convenience function to resolve symbol position
 */
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
