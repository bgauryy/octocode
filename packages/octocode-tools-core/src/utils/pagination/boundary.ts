/**
 * Language-aware semantic block boundary detection for pagination.
 *
 * When a page cut lands inside an indented block (mid-function/class body),
 * `findNextBlockBoundary` scans forward to the next top-level definition so
 * the agent can re-request with a larger charLength and get a semantically
 * complete page in one call rather than paginating blindly through a body.
 *
 * Language coverage matches octocode-minifier-utils heuristic.rs patterns:
 *   tree-sitter: ts tsx js jsx mjs cjs py go rs java c h sh bash zsh
 *   heuristic:   cpp hpp cc cxx cs kt scala rb php swift ex exs hs lhs
 *                css scss less html htm sql vue svelte lua md
 *   generic:     everything else (brace-depth fallback)
 *
 * Architecture note: this module intentionally mirrors the per-language logic
 * from packages/octocode-minifier-utils/src/signatures/heuristic.rs so it can
 * be replaced by a single Rust NAPI call (`findNextBlockBoundary`) once the
 * addon exposes it — the TypeScript caller site is identical.
 */

const LONE_CLOSE = /^[}\])][;,]?\s*$/;

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

  // ── Java / Kotlin / C# / Scala ───────────────────────────────────────────────
  if (
    ext === 'java' ||
    ext === 'kt' ||
    ext === 'kotlin' ||
    ext === 'cs' ||
    ext === 'scala'
  ) {
    return /^(?:public|private|protected|static|abstract|final|override|sealed|internal|class|interface|enum|object)\b/.test(
      line
    );
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
