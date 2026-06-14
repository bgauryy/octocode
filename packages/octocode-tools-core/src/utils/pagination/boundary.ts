/**
 * Semantic block boundary detection for proactive pagination chunking.
 *
 * ## Primary path — Rust/tree-sitter (`snapToSemanticBoundary`)
 * Calls `getSemanticBoundaryOffsets` from octocode-minifier-utils, which uses
 * tree-sitter for TS/JS/Py/Go/Rust/Java/C/Bash and heuristic patterns for
 * 30+ other languages. When a page cut falls mid-block, automatically extends
 * the page to the next semantic boundary — no hint-and-follow-up needed.
 * Falls back to `'char-limit'` mode when:
 *   • the file type has no semantic structure (JSON, YAML, plain text)
 *   • the next boundary is > MAX_SEMANTIC_EXTENSION chars beyond the budget
 *   • the Rust call throws (panic, OOM, unsupported extension)
 *
 * ## Fallback path — TypeScript heuristics (`findNextBlockBoundary`)
 * Reactive: runs AFTER a char-limit cut, surfaces `nextBlockChar` in the
 * pagination metadata so the agent can extend charLength in a follow-up
 * request (still saves one request over pure blind pagination).
 *
 * Language coverage — both paths:
 *   tree-sitter (Rust): ts tsx js jsx mjs cjs py go rs java c h sh bash zsh
 *   heuristic (Rust+TS): cpp hpp cc cxx cs kt kotlin scala rb php swift
 *                         ex exs hs lhs css scss less html htm sql vue svelte
 *                         lua md erl hrl + generic brace-depth fallback
 */

import { getSemanticBoundaryOffsets } from '@octocodeai/octocode-minifier-utils';

const LONE_CLOSE = /^[}\])][;,]?\s*$/;

// When the next semantic boundary is farther than this from the ideal cut,
// fall back to char-limit chunking (giant function) rather than over-extending.
const MAX_SEMANTIC_EXTENSION = 8_000;

function getExtension(filePath: string | undefined): string {
  if (!filePath) return '';
  const dot = filePath.lastIndexOf('.');
  return dot >= 0 ? filePath.slice(dot + 1).toLowerCase() : '';
}

/**
 * Per-language predicate: true when `line` starts a new top-level definition.
 * Each branch mirrors the heuristic.rs pattern set for that language.
 */
function isTopLevelLine(line: string, ext: string): boolean {
  if (!line.length) return false;

  // ── Indent-based languages (Python, Ruby, Elixir, CoffeeScript) ─────────────
  // Top-level = specific keywords at column 0, regardless of indent context.

  if (ext === 'py') {
    // def / async def / class / decorator at column 0
    return /^(?:async\s+)?def\s+\w|^class\s+\w|^@\w/.test(line);
  }

  if (ext === 'rb') {
    // def, class, module at column 0; `end` is not a start but marks a boundary
    return /^(?:def|class|module)\s+\S/.test(line);
  }

  if (ext === 'ex' || ext === 'exs') {
    // Elixir: def / defp / defmodule / defmacro at column 0
    return /^(?:def|defp|defmodule|defmacro)\s/.test(line);
  }

  if (ext === 'hs' || ext === 'lhs') {
    // Haskell: any non-comment, non-blank, non-indented line is a top-level binding
    const ch = line[0];
    return ch !== ' ' && ch !== '\t' && !line.startsWith('--') && !line.startsWith('{-');
  }

  // ── Java / Kotlin / C# / Scala ───────────────────────────────────────────────
  // These languages use class-scoped members that are ALWAYS indented, so the
  // column-0 gate must NOT apply. Patterns mirror heuristic.rs java_cs_patterns()
  // and scala_patterns() which use ^\s* (any indentation).
  if (
    ext === 'java' ||
    ext === 'kt' ||
    ext === 'kotlin' ||
    ext === 'cs' ||
    ext === 'scala'
  ) {
    const t = line.trimStart();
    if (!t || t.startsWith('//') || t.startsWith('/*') || t.startsWith('*')) return false;
    if (LONE_CLOSE.test(t)) return false;

    if (ext === 'scala') {
      // scala_patterns(): package/import, class/object/trait/enum, def/val/var/type
      return (
        /^(?:package|import)\s/.test(t) ||
        /^(?:sealed\s+|abstract\s+|final\s+|case\s+)*(?:class|object|trait|enum)\s+\w/.test(t) ||
        /^(?:override\s+|private\s+|protected\s+|implicit\s+|given\s+)*(?:def|val|var|type)\s+\w/.test(t)
      );
    }

    if (ext === 'kt' || ext === 'kotlin') {
      // java_cs_patterns() + Kotlin-specific keywords.
      // companion object / object can appear without a name (e.g. `companion object {`),
      // so use \b rather than requiring \s+\w after the keyword.
      return (
        /^(?:public|private|protected|internal|open|abstract|override|sealed|final|inline|suspend|actual|expect)\s/.test(t) ||
        /^(?:class|interface|enum\s+class|data\s+class|sealed\s+class|abstract\s+class|companion\s+object|object)\b/.test(t) ||
        /^(?:import|package)\s/.test(t) ||
        /^(?:fun|val|var|const\s+val|typealias)\s+\w/.test(t)
      );
    }

    // Java and C# — java_cs_patterns(): visibility/modifier prefix OR class/interface/enum OR import/using/package/namespace
    return (
      /^(?:public|private|protected|static|abstract|final|override|sealed|internal)\s/.test(t) ||
      /^(?:class|interface|enum|record|object)\s+\w/.test(t) ||
      /^(?:import|using|package|namespace)\s/.test(t)
    );
  }

  // ── All remaining languages require the line to be at column 0 ────────────

  const ch0 = line[0];
  if (ch0 === ' ' || ch0 === '\t') return false;
  if (LONE_CLOSE.test(line)) return false;

  // ── TypeScript / JavaScript family ────────────────────────────────────────
  if (
    ext === 'ts' ||
    ext === 'tsx' ||
    ext === 'js' ||
    ext === 'jsx' ||
    ext === 'mjs' ||
    ext === 'cjs'
  ) {
    return /^(?:export|import|function|class|const|let|var|async|type|interface|enum|abstract|declare|@)/.test(
      line
    );
  }

  // ── Go ──────────────────────────────────────────────────────────────────────
  if (ext === 'go') {
    return /^(?:func|type|var|const|package|import)\b/.test(line);
  }

  // ── Rust ─────────────────────────────────────────────────────────────────────
  if (ext === 'rs') {
    return /^(?:pub\b|fn\b|impl\b|struct\b|enum\b|trait\b|mod\b|use\b|const\b|static\b|type\b|#\[)/.test(
      line
    );
  }

  // ── Shell ────────────────────────────────────────────────────────────────────
  if (ext === 'sh' || ext === 'bash' || ext === 'zsh') {
    // named function or `name()` at column 0
    return /^(?:(?:export\s+)?function\s+\w+|\w+\s*\(\s*\))/.test(line);
  }

  // ── C / C++ ─────────────────────────────────────────────────────────────────
  if (
    ext === 'c' ||
    ext === 'h' ||
    ext === 'cpp' ||
    ext === 'hpp' ||
    ext === 'cc' ||
    ext === 'cxx'
  ) {
    // Preprocessor directives or identifier-starting lines that aren't comments
    return (
      /^[A-Za-z_#]/.test(line) &&
      !line.startsWith('//') &&
      !line.startsWith('/*')
    );
  }

  // ── PHP ─────────────────────────────────────────────────────────────────────
  if (ext === 'php') {
    return (
      /^(?:function|class|interface|trait|abstract|final|namespace|use)\s/.test(
        line
      ) || line.startsWith('<?')
    );
  }

  // ── Swift ────────────────────────────────────────────────────────────────────
  if (ext === 'swift') {
    return /^(?:func|class|struct|protocol|enum|extension|import|var|let|typealias)\b/.test(
      line
    );
  }

  // ── CSS / SCSS / LESS ────────────────────────────────────────────────────────
  if (ext === 'css' || ext === 'scss' || ext === 'less') {
    // selector or @-rule at column 0 that opens a block
    return !line.startsWith('/*') && !line.startsWith('//');
  }

  // ── SQL ─────────────────────────────────────────────────────────────────────
  if (ext === 'sql' || ext === 'tsql' || ext === 'plsql') {
    return /^(?:CREATE|ALTER|DROP|SELECT|INSERT|UPDATE|DELETE|WITH)\b/i.test(
      line
    );
  }

  // ── Elixir / Erlang ──────────────────────────────────────────────────────────
  if (ext === 'erl' || ext === 'hrl') {
    return /^-(?:module|export|import|define|record|type|spec)\(/.test(line);
  }

  // ── Markdown ─────────────────────────────────────────────────────────────────
  if (ext === 'md' || ext === 'markdown') {
    return line.startsWith('#');
  }

  // ── Generic fallback (Lua, Vue, Svelte, unknown extensions) ─────────────────
  // Any non-indented, non-empty line that isn't a lone closing delimiter.
  return true;
}

/**
 * Returns true when the paginated content ends inside an indented block
 * (mid-function/class body). Used as a gate before calling findNextBlockBoundary.
 */
export function isMidBlockCut(paginatedContent: string): boolean {
  const lastMeaningfulLine = paginatedContent.trimEnd().split('\n').at(-1) ?? '';
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
  const ext = getExtension(filePath);

  // Move past the partial line at the cut point
  let pos = content.indexOf('\n', fromChar);
  if (pos === -1) return undefined;
  pos += 1;

  while (pos < content.length) {
    const lineEnd = content.indexOf('\n', pos);
    const lineEndActual = lineEnd === -1 ? content.length : lineEnd;
    const line = content.substring(pos, lineEndActual);

    if (isTopLevelLine(line, ext)) {
      return pos;
    }

    if (lineEnd === -1) break;
    pos = lineEnd + 1;
  }

  return undefined;
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
 * `'char-limit'` when falling back to the original fixed-size cut.
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
  const idealEnd = charOffset + charLength;

  // Nothing to snap — content fits entirely
  if (idealEnd >= content.length) {
    return { length: content.length - charOffset, chunkMode: 'char-limit' };
  }

  // Get tree-sitter / heuristic boundaries from Rust
  let boundaries: number[];
  try {
    boundaries = getSemanticBoundaryOffsets(content, filePath ?? '');
  } catch {
    boundaries = [];
  }

  if (boundaries.length === 0) {
    // Data file, plain text, oversized, or unsupported — use fixed char-limit
    return { length: charLength, chunkMode: 'char-limit' };
  }

  // Find the first boundary strictly past the ideal cut point
  const nextBoundary = boundaries.find(b => b > idealEnd);

  if (nextBoundary === undefined) {
    // No boundary after idealEnd — we're already in the last semantic chunk
    return { length: charLength, chunkMode: 'char-limit' };
  }

  const extension = nextBoundary - idealEnd;

  // Only snap when the extension is within budget.
  // Giant functions (> MAX_SEMANTIC_EXTENSION) stay char-limited — the reactive
  // `nextBlockChar` hint handles those as a fallback.
  if (extension <= MAX_SEMANTIC_EXTENSION) {
    return { length: nextBoundary - charOffset, chunkMode: 'semantic' };
  }

  return { length: charLength, chunkMode: 'char-limit' };
}
