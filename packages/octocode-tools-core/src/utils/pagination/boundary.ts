/**
 * Semantic block boundary detection for proactive pagination chunking.
 *
 * ## Primary path — Rust/tree-sitter (`snapToSemanticBoundary`)
 * Calls `getSemanticBoundaryOffsets` from octocode-context-utils, which uses
 * the centralized Rust tree-sitter/heuristic extractor. When a page cut falls
 * mid-block, automatically extends the page to the next semantic boundary —
 * no hint-and-follow-up needed.
 * Uses `'char-limit'` mode when:
 *   • the file type has no semantic structure (JSON, YAML, plain text)
 *   • the next boundary is > MAX_SEMANTIC_EXTENSION chars beyond the budget
 *
 * ## Reactive path — Rust semantic offsets (`findNextBlockBoundary`)
 * Reactive: runs AFTER a char-limit cut, surfaces `nextBlockChar` in the
 * pagination metadata so the agent can extend charLength in a follow-up
 * request (still saves one request over pure blind pagination).
 *
 * Language coverage is owned by octocode-context-utils and tested there.
 */

import { contextUtils } from '../contextUtils.js';

// When the next semantic boundary is farther than this from the ideal cut,
// use char-limit chunking for giant functions rather than over-extending.
const MAX_SEMANTIC_EXTENSION = 8_000;
const GENERIC_BOUNDARY_FILE = '__octocode_generic__.unknown';

function resolveBoundaryFilePath(filePath: string | undefined): string {
  return filePath && filePath.trim().length > 0
    ? filePath
    : GENERIC_BOUNDARY_FILE;
}

function getSemanticBoundaries(content: string, filePath?: string): number[] {
  return contextUtils
    .getSemanticBoundaryOffsets(content, resolveBoundaryFilePath(filePath))
    .filter(
      (offset): offset is number =>
        Number.isInteger(offset) && offset >= 0 && offset <= content.length
    );
}

function nextLineStart(content: string, fromChar: number): number | undefined {
  const lineBreak = content.indexOf('\n', fromChar);
  return lineBreak === -1 ? undefined : lineBreak + 1;
}

/**
 * Returns true when the paginated content ends inside an indented block
 * (mid-function/class body). Used as a gate before calling findNextBlockBoundary.
 */
export function isMidBlockCut(paginatedContent: string): boolean {
  const lastMeaningfulLine =
    paginatedContent.trimEnd().split('\n').at(-1) ?? '';
  return (
    lastMeaningfulLine.length > 0 &&
    (lastMeaningfulLine[0] === ' ' || lastMeaningfulLine[0] === '\t')
  );
}

/**
 * Scans `content` forward from `fromChar` to find the char offset of the next
 * top-level semantic block boundary for the given file type.
 *
 * The scan starts after the partial line at the cut point (seeks to the next
 * newline first). Returns `undefined` when no boundary is found before EOF.
 *
 * `fromChar` is a JavaScript string index (UTF-16 code units). This is exact
 * for BMP content (code files); for non-BMP chars in string literals the
 * result may be off by the number of supplementary-plane characters — a
 * negligible error for source code.
 */
export function findNextBlockBoundary(
  content: string,
  fromChar: number,
  filePath?: string
): number | undefined {
  const searchStart = nextLineStart(content, Math.max(0, fromChar));
  if (searchStart === undefined) return undefined;
  return getSemanticBoundaries(content, filePath).find(
    offset => offset >= searchStart && offset > fromChar
  );
}

/**
 * Build a `nextBlockChar` value and an agent-readable boundary hint when a
 * page cut lands mid-block. Returns `{nextBlockChar, hint}` or `undefined`
 * when the cut is already at a clean boundary.
 */
export function buildBlockBoundaryHint(
  paginatedContent: string,
  fullContent: string,
  cutPos: number,
  currentCharLength: number,
  filePath?: string
): { nextBlockChar: number; hint: string } | undefined {
  if (!isMidBlockCut(paginatedContent)) return undefined;

  const nextBlockChar = findNextBlockBoundary(fullContent, cutPos, filePath);
  if (nextBlockChar === undefined) return undefined;

  const extendBy = nextBlockChar - cutPos;
  const hint =
    `Page cut mid-block at char ${cutPos}. ` +
    `Next top-level definition at char ${nextBlockChar}. ` +
    `Re-request with charLength=${currentCharLength + extendBy} to extend this page to the next boundary, ` +
    `or use charOffset=${cutPos} to continue page-by-page.`;

  return { nextBlockChar, hint };
}

/** Discriminator for how a page boundary was chosen. */
export type ChunkMode = 'semantic' | 'char-limit';

/**
 * **Proactive** semantic chunking — the primary pagination path.
 *
 * Calls `getSemanticBoundaryOffsets` (Rust/tree-sitter) to get a sorted list
 * of semantic block starts, then snaps the page end to the next boundary after
 * `charOffset + charLength`.  Returns `chunkMode: 'semantic'` on success or
 * `'char-limit'` when the original fixed-size cut is used.
 *
 * Default is always char-limit — snapping is a best-effort improvement.
 *
 * @param content      The content being paginated (already minified if applicable)
 * @param charOffset   Start of the current page (JS char index)
 * @param charLength   Requested page size in JS chars
 * @param filePath     File path used to derive the language for tree-sitter/heuristic
 * @returns `{ length, chunkMode }` — `length` is the actual page size to use
 */
export function snapToSemanticBoundary(
  content: string,
  charOffset: number,
  charLength: number,
  filePath?: string
): { length: number; chunkMode: ChunkMode } {
  const safeOffset = Math.min(Math.max(0, charOffset), content.length);
  const safeLength = Math.max(1, charLength);
  const idealEnd = safeOffset + safeLength;

  // Nothing to snap — content fits entirely
  if (idealEnd >= content.length) {
    return { length: content.length - safeOffset, chunkMode: 'char-limit' };
  }

  const boundaries = getSemanticBoundaries(content, filePath);
  if (boundaries.length === 0) {
    // Data file, plain text, oversized, or unsupported — use fixed char-limit
    return { length: safeLength, chunkMode: 'char-limit' };
  }

  // Find the first boundary strictly past the ideal cut point
  const nextBoundary = boundaries.find(b => b > idealEnd);

  if (nextBoundary === undefined) {
    // No boundary after idealEnd — we're already in the last semantic chunk
    return { length: safeLength, chunkMode: 'char-limit' };
  }

  const extension = nextBoundary - idealEnd;

  // Only snap when the extension is within budget.
  // Giant functions (> MAX_SEMANTIC_EXTENSION) stay char-limited — the reactive
  // `nextBlockChar` hint handles those as a reactive continuation.
  if (extension <= MAX_SEMANTIC_EXTENSION) {
    return { length: nextBoundary - safeOffset, chunkMode: 'semantic' };
  }

  return { length: safeLength, chunkMode: 'char-limit' };
}
