import { minifyContentSync } from './minifier.js';
import {
  minifyJsonCore,
  minifyGeneralCore,
  removeComments,
} from './minifierStrategies.js';
import { MINIFY_CONFIG } from './minifierTypes.js';
import type { CommentPatternGroup } from './minifierTypes.js';
import { getExtension } from '../file/filters.js';

export function applyMinification(content: string, filePath: string): string {
  try {
    const minifiedContent = minifyContentSync(content, filePath);
    return minifiedContent.length < content.length ? minifiedContent : content;
  } catch {
    return content;
  }
}

const JSON_EXTS = new Set(['json', 'jsonc', 'json5']);

/**
 * Extensions whose comment syntax we can safely strip before general
 * whitespace compression. Matches the comment patterns already defined in
 * MINIFY_CONFIG.commentPatterns. Languages NOT listed here still get
 * minifyGeneralCore (whitespace + indentation compression) — safe fallback.
 */
const COMMENT_STRIP_EXTS: Record<
  string,
  CommentPatternGroup | CommentPatternGroup[]
> = {
  ts: 'c-style',
  tsx: 'c-style',
  js: 'c-style',
  jsx: 'c-style',
  mjs: 'c-style',
  cjs: 'c-style',
  go: 'c-style',
  java: 'c-style',
  c: 'c-style',
  cpp: 'c-style',
  cs: 'c-style',
  rs: 'c-style',
  swift: 'c-style',
  kt: 'c-style',
  scala: 'c-style',
  dart: 'c-style',
  py: 'hash',
  rb: 'hash',
  sh: 'hash',
  bash: 'hash',
  yaml: 'hash',
  yml: 'hash',
  php: ['c-style', 'hash'],
};

/**
 * Content-safe minification for file viewing tools (localGetFileContent,
 * githubGetFileContent).
 *
 * Pipeline per file type:
 *   JSON/JSONC/JSON5  → minifyJsonCore (parse + re-stringify, ~12-20% savings)
 *   Known code exts   → strip comments first, then minifyGeneralCore
 *                        (indentation compression + blank-line reduction, ~13-49%)
 *   Everything else   → minifyGeneralCore (whitespace/indent only, ~9-21%)
 *
 * Always returns the original if the minified version is not shorter.
 */
export function applyContentViewMinification(
  content: string,
  filePath: string
): string {
  try {
    const ext = getExtension(filePath, { lowercase: true, fallback: 'txt' });

    let minified: string;

    if (JSON_EXTS.has(ext)) {
      minified = minifyJsonCore(content).content;
    } else {
      const commentType = COMMENT_STRIP_EXTS[ext];
      const stripped = commentType
        ? removeComments(content, commentType)
        : content;
      minified = minifyGeneralCore(stripped);
    }

    return minified.length < content.length ? minified : content;
  } catch {
    return content;
  }
}

export { MINIFY_CONFIG };

/**
 * Signature patterns per language family.
 * Matches: imports/exports, function/method declarations,
 * class/interface/type definitions. Bodies are intentionally excluded.
 */
const SIG_PATTERNS: Record<string, RegExp[]> = {
  'ts-js': [
    /^\s*(export\s+)?(default\s+)?(async\s+)?function\s*\*?\s*\w+[^{]*/,
    /^\s*(export\s+)?(abstract\s+)?class\s+\w+[^{]*/,
    /^\s*(export\s+)?interface\s+\w+[^{]*\{[^}]*\}/,
    /^\s*(export\s+)?type\s+\w+[^;\n=]*=?[^;\n]*/,
    /^\s*(import|export)\s+[^\n]+/,
    /^\s*(export\s+)?const\s+\w+[^=]*=\s*(\([^)]*\)|[^=>\n]+)\s*=>/,
    /^\s*(export\s+)?enum\s+\w+/,
    /^\s*(public|private|protected|static|abstract|readonly|override)\s+\w+/,
    /^\s*(async\s+)?\w+\s*(<[^>]+>)?\s*\([^)]*\)\s*(:\s*[^\n{]+)?$/,
  ],
  py: [
    /^\s*(def|class|async\s+def)\s+\w+/,
    /^\s*@\w+/,
    /^\s*(import|from)\s+\w+/,
    /^\s*__\w+__\s*=/,
  ],
  go: [
    /^\s*(func|type|var|const|import)\s+\w+/,
    /^\s*type\s+\w+\s+(struct|interface)/,
    /^\s*package\s+\w+/,
    /^\s*import\s+\(/,
  ],
  'java-cs': [
    /^\s*(public|private|protected|static|abstract|final|override)\s+/,
    /^\s*(class|interface|enum|record)\s+\w+/,
    /^\s*(import|using|package|namespace)\s+/,
  ],
  rust: [
    /^\s*(pub(\s*\([^)]+\))?\s+)?(fn|struct|enum|trait|impl|type|const|use|mod)\s+\w+/,
    /^\s*(use|extern)\s+/,
  ],
};

const EXT_TO_SIG_FAMILY: Record<string, keyof typeof SIG_PATTERNS> = {
  ts: 'ts-js',
  tsx: 'ts-js',
  js: 'ts-js',
  jsx: 'ts-js',
  mjs: 'ts-js',
  cjs: 'ts-js',
  py: 'py',
  go: 'go',
  java: 'java-cs',
  cs: 'java-cs',
  kt: 'java-cs',
  rs: 'rust',
};

/** Concise hint emitted whenever signaturesOnly extraction succeeds. */
export const SIGNATURES_ONLY_HINT =
  'Signatures only — bodies omitted. Use startLine/endLine to read a body.';

// Net `{`/`}` (and `(`/`[`) depth a line contributes, ignoring line comments.
// Used to keep multi-line declaration heads/blocks intact rather than just
// their opening line. Best-effort — braces inside strings are not excluded,
// consistent with the tool's stated LOSSY contract.
function netDelta(line: string, open: string, close: string): number {
  const code = line.replace(/\/\/.*$/, '');
  let depth = 0;
  for (const ch of code) {
    if (ch === open) depth++;
    else if (ch === close) depth--;
  }
  return depth;
}
const braceDelta = (line: string): number => netDelta(line, '{', '}');
const roundDelta = (line: string): number =>
  netDelta(line, '(', ')') + netDelta(line, '[', ']');

/**
 * Extract only the structural skeleton of a source file:
 * imports, function/class/interface/type signatures — bodies dropped.
 *
 * For the ts/js family, multi-line constructs are kept whole: a multi-line
 * import keeps its names, an interface/type/enum keeps its body, and a
 * multi-line function signature keeps its params + return type (function
 * bodies are still dropped). Other families keep the single matched line.
 *
 * Returns null when the file extension is not recognised or when
 * extraction produces an empty result (caller should use original content).
 *
 * Savings: typically 80-95% on code files. LOSSY — agents should follow
 * up with startLine/endLine reads to get specific function bodies.
 */
export function extractSignatures(
  content: string,
  filePath: string
): string | null {
  try {
    const ext = getExtension(filePath, { lowercase: true, fallback: 'txt' });
    const family = EXT_TO_SIG_FAMILY[ext];
    if (!family) return null;

    const patterns = SIG_PATTERNS[family];
    if (!patterns) return null;

    const isTsJs = family === 'ts-js';
    const lines = content.split('\n');
    const kept: string[] = [];

    // Keep every line of an open `{...}` block (import names / interface /
    // type / enum body) starting from a line with positive brace depth.
    const keepBraceBlock = (start: number): number => {
      let depth = braceDelta(lines[start]!);
      let i = start + 1;
      while (i < lines.length && depth > 0) {
        kept.push(lines[i]!.trimEnd());
        depth += braceDelta(lines[i]!);
        i++;
      }
      return i;
    };

    let i = 0;
    while (i < lines.length) {
      const line = lines[i]!;
      const matched = patterns.some(pattern => pattern.test(line));
      if (!matched) {
        i++;
        continue;
      }

      kept.push(line.trimEnd());

      if (isTsJs) {
        const opensBlock =
          /\b(interface|enum)\b/.test(line) || // interface / enum body
          /[:=]\s*\{[^}]*$/.test(line); // `type X = {` / `: {` object opener
        const isMultiImport =
          (/^\s*import\b/.test(line) ||
            /^\s*export\s+(type\s+)?\{/.test(line)) &&
          braceDelta(line) > 0;

        if ((opensBlock && braceDelta(line) > 0) || isMultiImport) {
          i = keepBraceBlock(i);
          continue;
        }

        // Multi-line function/method signature head: keep params + return type
        // (everything until the parameter list closes). The body that follows
        // is not matched by any pattern, so it is dropped as usual.
        if (roundDelta(line) > 0) {
          let depth = roundDelta(line);
          i++;
          while (i < lines.length && depth > 0) {
            kept.push(lines[i]!.trimEnd());
            depth += roundDelta(lines[i]!);
            i++;
          }
          continue;
        }
      }

      i++;
    }

    if (kept.length === 0) return null;

    const result = kept
      .join('\n')
      .replace(/\n{3,}/g, '\n\n')
      .trim();
    return result.length < content.length ? result : null;
  } catch {
    return null;
  }
}
