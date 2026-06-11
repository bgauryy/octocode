import { minify, minify_sync } from 'terser';
import type { MinifyOptions } from 'terser';
import CleanCSS from 'clean-css';
import { minify as htmlMinifierTerser } from 'html-minifier-terser';
import ts from 'typescript';
import type {
  CommentPatternGroup,
  FileTypeMinifyConfig,
} from '../types/index.js';
import { MINIFY_CONFIG } from '../types/index.js';

type BlockCommentRule = {
  start: string;
  end: string;
  nested?: boolean;
};

type LineCommentRule = {
  token: string;
  requireBoundary?: boolean;
  preserveShebang?: boolean;
};

type StringAwareCommentRules = {
  block?: BlockCommentRule[];
  line?: LineCommentRule[];
  regex?: boolean;
  powershellHereStrings?: boolean;
  quoteDelimiters?: readonly string[];
};

type StrategyMinifyResult = {
  content: string;
  failed: boolean;
  reason?: string;
};

const TERSER_OPTIONS: MinifyOptions = {
  compress: {
    drop_console: false,
    drop_debugger: false,
    sequences: true,
    conditionals: true,
    comparisons: true,
    evaluate: true,
    booleans: true,
    loops: false,
    unused: false,
    dead_code: true,
    side_effects: false,
  },
  mangle: false,
  format: {
    comments: false,
    beautify: false,
    semicolons: true,
  },
  sourceMap: false,
};

const TYPESCRIPT_COMPILER_OPTIONS: ts.CompilerOptions = {
  target: ts.ScriptTarget.ES2020,
  module: ts.ModuleKind.ESNext,
  jsx: ts.JsxEmit.ReactJSX,
  removeComments: true,
  sourceMap: false,
  inlineSourceMap: false,
  importHelpers: false,
};

function stringAwareRulesFor(
  type: CommentPatternGroup
): StringAwareCommentRules | null {
  switch (type) {
    case 'c-style':
      return {
        block: [{ start: '/*', end: '*/' }],
        line: [{ token: '//' }],
        regex: true,
      };
    case 'hash':
      return {
        line: [{ token: '#', preserveShebang: true }],
      };
    case 'html':
      return {
        block: [{ start: '<!--', end: '-->' }],
      };
    case 'sql':
      return {
        block: [{ start: '/*', end: '*/' }],
        line: [{ token: '--', requireBoundary: false }],
      };
    case 'lua':
      return {
        block: [{ start: '--[[', end: ']]' }],
        line: [{ token: '--' }],
      };
    case 'haskell':
      return {
        block: [{ start: '{-', end: '-}' }],
        line: [{ token: '--' }],
      };
    case 'semicolon':
      return { line: [{ token: ';' }] };
    case 'wasm-text':
      return {
        block: [{ start: '(;', end: ';)' }],
        line: [{ token: ';;' }],
      };
    case 'percent':
      return { line: [{ token: '%' }] };
    case 'template':
      return {
        block: [
          { start: '{{!--', end: '--}}' },
          { start: '{{!', end: '}}' },
          { start: '<%#', end: '%>' },
          { start: '{#', end: '#}' },
        ],
      };
    case 'haml':
      return { line: [{ token: '-#' }] };
    case 'slim':
      return { line: [{ token: '/' }] };
    case 'powershell':
      return {
        block: [{ start: '<#', end: '#>' }],
        line: [{ token: '#', preserveShebang: true }],
        powershellHereStrings: true,
      };
    case 'bang':
      return { line: [{ token: '!' }] };
    case 'apostrophe':
      return {
        line: [{ token: "'" }],
        quoteDelimiters: ['"""', '"', '`'],
      };
    case 'double-dash':
      return { line: [{ token: '--' }] };
    case 'fsharp-block':
      return {
        block: [{ start: '(*', end: '*)', nested: true }],
      };
    case 'pascal':
      return {
        block: [
          { start: '(*', end: '*)', nested: true },
          { start: '{', end: '}' },
        ],
        line: [{ token: '//' }],
        quoteDelimiters: ["'", '"'],
      };
    default:
      return null;
  }
}

const DEFAULT_QUOTE_DELIMITERS = ['"""', "'''", '"', "'", '`'] as const;

function quoteDelimiterAt(
  content: string,
  index: number,
  rules: StringAwareCommentRules
): string | null {
  const delimiters = rules.quoteDelimiters ?? DEFAULT_QUOTE_DELIMITERS;
  const orderedDelimiters = [...delimiters].sort(
    (left, right) => right.length - left.length
  );

  return (
    orderedDelimiters.find(delimiter => content.startsWith(delimiter, index)) ??
    null
  );
}

function findRustRawStringEnd(content: string, index: number): number | null {
  const match = /^(?:b?r)(#*)"/.exec(content.slice(index));
  if (!match) return null;

  const hashes = match[1]!;
  const endMarker = `"${hashes}`;
  const bodyStart = index + match[0].length;
  const endIndex = content.indexOf(endMarker, bodyStart);
  return endIndex === -1 ? content.length : endIndex + endMarker.length;
}

function findCSharpVerbatimStringEnd(
  content: string,
  index: number
): number | null {
  let bodyStart: number | null = null;

  if (content.startsWith('@"', index)) {
    bodyStart = index + 2;
  } else if (
    content.startsWith('$@"', index) ||
    content.startsWith('@$"', index)
  ) {
    bodyStart = index + 3;
  }

  if (bodyStart === null) return null;

  for (let i = bodyStart; i < content.length; i++) {
    if (content[i] !== '"') continue;
    if (content[i + 1] === '"') {
      i++;
      continue;
    }
    return i + 1;
  }

  return content.length;
}

function isRegexLiteralStart(content: string, index: number): boolean {
  if (
    content[index] !== '/' ||
    content[index + 1] === '/' ||
    content[index + 1] === '*'
  ) {
    return false;
  }

  let previousIndex = index - 1;
  while (previousIndex >= 0 && /\s/.test(content[previousIndex]!)) {
    previousIndex--;
  }

  if (previousIndex < 0) return true;

  const previous = content[previousIndex]!;
  if ('([{=,:;!&|?+-*~^<>'.includes(previous)) return true;

  const before = content.slice(
    Math.max(0, previousIndex - 12),
    previousIndex + 1
  );
  return /\b(?:return|throw|case|delete|typeof|void|yield|await)$/.test(before);
}

function findRegexLiteralEnd(content: string, index: number): number | null {
  let escaped = false;
  let inCharacterClass = false;

  for (let i = index + 1; i < content.length; i++) {
    const char = content[i]!;

    if (escaped) {
      escaped = false;
      continue;
    }

    if (char === '\\') {
      escaped = true;
      continue;
    }

    if (char === '[') {
      inCharacterClass = true;
      continue;
    }

    if (char === ']') {
      inCharacterClass = false;
      continue;
    }

    if (char === '/' && !inCharacterClass) {
      let end = i + 1;
      while (/[A-Za-z]/.test(content[end] ?? '')) end++;
      return end;
    }

    if (char === '\n' || char === '\r') return null;
  }

  return null;
}

function findPowerShellHereStringEnd(
  content: string,
  index: number
): number | null {
  const quote = content.startsWith('@"', index)
    ? '"'
    : content.startsWith("@'", index)
      ? "'"
      : null;

  if (!quote) return null;

  const afterStart = index + 2;
  if (content[afterStart] !== '\n' && content[afterStart] !== '\r') {
    return null;
  }

  const endPattern = new RegExp(`(?:^|\\r?\\n)${quote}@`, 'g');
  endPattern.lastIndex = afterStart;
  const endMatch = endPattern.exec(content);

  return endMatch ? endMatch.index + endMatch[0]!.length : content.length;
}

function findLexicalIslandEnd(
  content: string,
  index: number,
  rules: StringAwareCommentRules
): number | null {
  return (
    (rules.powershellHereStrings
      ? findPowerShellHereStringEnd(content, index)
      : null) ??
    findRustRawStringEnd(content, index) ??
    findCSharpVerbatimStringEnd(content, index) ??
    (rules.regex && isRegexLiteralStart(content, index)
      ? findRegexLiteralEnd(content, index)
      : null)
  );
}

function hasLineCommentBoundary(content: string, index: number): boolean {
  if (index === 0) return true;
  const previous = content[index - 1];
  return (
    previous === ' ' ||
    previous === '\t' ||
    previous === '\n' ||
    previous === '\r'
  );
}

function shouldStripLineComment(
  content: string,
  index: number,
  rule: LineCommentRule
): boolean {
  if (!content.startsWith(rule.token, index)) return false;
  if (rule.preserveShebang && content.startsWith('#!', index)) return false;

  return rule.requireBoundary === false
    ? true
    : hasLineCommentBoundary(content, index);
}

function preserveLineBreaks(content: string): string {
  return content.replace(/[^\r\n]/g, '');
}

function findBlockCommentEnd(
  content: string,
  afterStart: number,
  rule: BlockCommentRule
): number {
  if (!rule.nested) {
    const endIndex = content.indexOf(rule.end, afterStart);
    return endIndex === -1 ? content.length : endIndex + rule.end.length;
  }

  let depth = 1;
  for (let cursor = afterStart; cursor < content.length; ) {
    if (content.startsWith(rule.start, cursor)) {
      depth++;
      cursor += rule.start.length;
      continue;
    }

    if (content.startsWith(rule.end, cursor)) {
      depth--;
      cursor += rule.end.length;
      if (depth === 0) return cursor;
      continue;
    }

    cursor++;
  }

  return content.length;
}

function stripStringAwareComments(
  content: string,
  rules: StringAwareCommentRules
): string {
  let result = '';
  let quoteEnd: string | null = null;
  let escaped = false;

  for (let i = 0; i < content.length; ) {
    if (quoteEnd) {
      if (quoteEnd.length > 1 && content.startsWith(quoteEnd, i)) {
        result += quoteEnd;
        i += quoteEnd.length;
        quoteEnd = null;
        continue;
      }

      const char = content[i]!;
      result += char;
      i++;

      if (quoteEnd.length === 1) {
        if (escaped) {
          escaped = false;
        } else if (char === '\\') {
          escaped = true;
        } else if (char === quoteEnd) {
          quoteEnd = null;
        }
      }
      continue;
    }

    const lexicalIslandEnd = findLexicalIslandEnd(content, i, rules);
    if (lexicalIslandEnd !== null) {
      result += content.slice(i, lexicalIslandEnd);
      i = lexicalIslandEnd;
      continue;
    }

    const quote = quoteDelimiterAt(content, i, rules);
    if (quote) {
      quoteEnd = quote;
      escaped = false;
      result += quote;
      i += quote.length;
      continue;
    }

    const blockRule = rules.block?.find(rule =>
      content.startsWith(rule.start, i)
    );
    if (blockRule) {
      const commentStart = i;
      const afterStart = i + blockRule.start.length;
      const commentEnd = findBlockCommentEnd(content, afterStart, blockRule);
      result += preserveLineBreaks(content.slice(commentStart, commentEnd));
      i = commentEnd;
      continue;
    }

    const lineRule = rules.line?.find(rule =>
      shouldStripLineComment(content, i, rule)
    );
    if (lineRule) {
      const newlineIndex = content.indexOf('\n', i);
      if (newlineIndex === -1) break;
      i = newlineIndex;
      continue;
    }

    result += content[i]!;
    i++;
  }

  return result;
}

/**
 * Strip Python triple-quoted strings that are in docstring position:
 * — module docstring (appears before any code)
 * — class/function docstring (first statement after a line ending with ':')
 *
 * The heuristic is: a triple-quoted string whose opening delimiter is the
 * FIRST non-whitespace token on its line, and whose immediately preceding
 * non-blank non-comment line either (a) ends with ':' or (b) does not exist
 * (start of file).
 *
 * Known safe: string literals assigned to variables (e.g. `x = """…"""`) are
 * NOT stripped because the `=` appears before the delimiter on the same line.
 * Known limitation: a bare triple-quoted string inside an `if`/`for` body would
 * be stripped — but such code is essentially non-existent in real Python.
 */
export function stripPythonDocstrings(content: string): string {
  try {
    const lines = content.split('\n');
    const out: string[] = [];
    let i = 0;

    // Scan backwards for the last non-blank, non-comment line before `idx`.
    function prevCodeLine(idx: number): string {
      for (let j = idx - 1; j >= 0; j--) {
        const t = lines[j]!.trim();
        if (t && !t.startsWith('#')) return t;
      }
      return '';
    }

    while (i < lines.length) {
      const line = lines[i]!;
      const trimmed = line.trim();

      // Docstring opener: first non-whitespace is """ or '''
      if (trimmed.startsWith('"""') || trimmed.startsWith("'''")) {
        const delimiter = trimmed.startsWith('"""') ? '"""' : "'''";
        const prev = prevCodeLine(i);

        // Docstring context: after def/class/with/else/try/except ending in ':'  OR module top
        const isDocContext = prev === '' || prev.endsWith(':');

        if (isDocContext) {
          // Single-line docstring: """text""" (delimiter appears twice on same line)
          const afterOpen = trimmed.slice(3);
          const hasSameLineClose = afterOpen.includes(delimiter);

          out.push(''); // preserve line count
          i++;

          if (!hasSameLineClose) {
            // Multi-line: consume until closing delimiter
            while (i < lines.length) {
              const nextLine = lines[i]!;
              out.push('');
              i++;
              if (nextLine.includes(delimiter)) break;
            }
          }
          continue;
        }
      }

      out.push(line);
      i++;
    }

    return out.join('\n');
  } /* v8 ignore start */ catch {
    return content;
  } /* v8 ignore stop */
}

export function removeComments(
  content: string,
  commentTypes: CommentPatternGroup | CommentPatternGroup[]
): string {
  try {
    let result = content;
    const types = Array.isArray(commentTypes) ? commentTypes : [commentTypes];

    for (const type of types) {
      // Python docstrings are handled by a dedicated function, not the string-aware scanner.
      if (type === 'python-docstring') {
        result = stripPythonDocstrings(result);
        continue;
      }

      const stringAwareRules = stringAwareRulesFor(type);
      if (stringAwareRules) {
        result = stripStringAwareComments(result, stringAwareRules);
        continue;
      }

      const patterns = MINIFY_CONFIG.commentPatterns[type];
      if (patterns) {
        for (const pattern of patterns) {
          try {
            result = result.replace(pattern, '');
          } /* v8 ignore start */ catch {
            continue;
          } /* v8 ignore stop */
        }
      }
    }
    return result;
  } /* v8 ignore start */ catch {
    return content;
  } /* v8 ignore stop */
}

export function minifyConservativeCore(
  content: string,
  config: FileTypeMinifyConfig
): string {
  try {
    let result = content;

    if (config.comments) {
      result = removeComments(result, config.comments);
    }

    return result
      .replace(/[ \t]+$/gm, '')
      .replace(/\r\n/g, '\n')
      .replace(/\n\s*\n\s*\n+/g, '\n\n')
      .trim();
  } /* v8 ignore start */ catch {
    return content;
  } /* v8 ignore stop */
}

export function minifyAggressiveCore(
  content: string,
  config: FileTypeMinifyConfig
): string {
  try {
    let result = content;

    if (config.comments) {
      result = removeComments(result, config.comments);
    }

    return result
      .replace(/\s+/g, ' ')
      .replace(/\s*([{}:;,])\s*/g, '$1')
      .replace(/>\s+</g, '><')
      .trim();
  } /* v8 ignore start */ catch {
    return content;
  } /* v8 ignore stop */
}

function stripJsonComments(content: string): string {
  let result = '';
  let quote: '"' | "'" | null = null;
  let escaped = false;

  for (let i = 0; i < content.length; i++) {
    const char = content[i]!;
    const next = content[i + 1];

    if (quote !== null) {
      result += char;
      if (escaped) {
        escaped = false;
      } else if (char === '\\') {
        escaped = true;
      } else if (char === quote) {
        quote = null;
      }
      continue;
    }

    if (char === '"' || char === "'") {
      quote = char;
      result += char;
      continue;
    }

    if (char === '/' && next === '/') {
      while (i + 1 < content.length && content[i + 1] !== '\n') {
        i++;
      }
      continue;
    }

    if (char === '/' && next === '*') {
      i += 2;
      while (
        i < content.length &&
        !(content[i] === '*' && content[i + 1] === '/')
      ) {
        i++;
      }
      if (i < content.length) {
        i++;
      }
      continue;
    }

    result += char;
  }

  return result;
}

function stripJsonTrailingCommas(content: string): string {
  let result = '';
  let quote: '"' | "'" | null = null;
  let escaped = false;

  for (let i = 0; i < content.length; i++) {
    const char = content[i]!;

    if (quote !== null) {
      result += char;
      if (escaped) {
        escaped = false;
      } else if (char === '\\') {
        escaped = true;
      } else if (char === quote) {
        quote = null;
      }
      continue;
    }

    if (char === '"' || char === "'") {
      quote = char;
      result += char;
      continue;
    }

    if (char === ',') {
      let lookahead = i + 1;
      while (/\s/.test(content[lookahead] ?? '')) {
        lookahead++;
      }
      if (content[lookahead] === '}' || content[lookahead] === ']') {
        continue;
      }
    }

    result += char;
  }

  return result;
}

function normalizeJsonLikeContent(content: string): string {
  return stripJsonTrailingCommas(stripJsonComments(content));
}

export function minifyJsonCore(content: string): {
  content: string;
  failed: boolean;
  reason?: string;
} {
  try {
    return { content: JSON.stringify(JSON.parse(content)), failed: false };
  } catch {
    try {
      const cleaned = normalizeJsonLikeContent(content);
      return { content: JSON.stringify(JSON.parse(cleaned)), failed: false };
    } catch {
      return { content: content.trim(), failed: false };
    }
  }
}

/**
 * Readable JSON for agent consumption.
 *
 * Strategy: strip JSONC/JSON5 noise (comments, trailing commas) while
 * preserving the original whitespace structure so agents keep readability.
 * Does NOT re-format with JSON.stringify — that can expand compact arrays
 * and produce output LARGER than the input, defeating the guard.
 *
 * - Clean JSON    → returned as-is  (no change, guard returns original)
 * - JSONC/JSON5   → comments + trailing commas stripped, structure preserved
 * - Unparseable   → trimmed original
 */
export function minifyJsonReadable(content: string): {
  content: string;
  failed: boolean;
  reason?: string;
} {
  try {
    JSON.parse(content); // already valid JSON — return unchanged
    return { content, failed: false };
  } catch {
    try {
      // JSONC / JSON5: strip noise while keeping original indentation.
      // Also remove trailing whitespace (from stripped inline comments) and
      // collapse consecutive blank lines left behind by comment-only lines.
      const cleaned = normalizeJsonLikeContent(content)
        .replace(/[ \t]+$/gm, '')
        .replace(/\n\s*\n\s*\n+/g, '\n\n')
        .trim();
      JSON.parse(cleaned); // validate the cleaned result
      return { content: cleaned, failed: false };
    } catch {
      return { content: content.trim(), failed: false };
    }
  }
}

/**
 * Whitespace-only compression for code files (comment stripping is handled
 * by the caller before this runs). Preserves original indentation so agents
 * keep structural context; only removes trailing whitespace and collapses
 * 3+ consecutive blank lines to max 2.
 *
 * Does NOT halve indentation — `minifyGeneralCore` is reserved for plain-text
 * (txt/log/unknown) files where structural indentation carries no meaning.
 */
export function minifyCodeCore(content: string): string {
  try {
    // Leading blank lines are dropped but the first line's own indentation is
    // preserved — skeleton gutters (` 1| …`) and indented first lines must
    // keep their alignment.
    return content
      .replace(/[ \t]+$/gm, '')
      .replace(/\r\n/g, '\n')
      .replace(/\n\s*\n\s*\n+/g, '\n\n')
      .replace(/^\n+/, '')
      .trimEnd();
  } /* v8 ignore start */ catch {
    return content;
  } /* v8 ignore stop */
}

export function minifyGeneralCore(content: string): string {
  try {
    return content
      .replace(/[ \t]+$/gm, '')
      .replace(/\r\n/g, '\n')
      .replace(/\n\s*\n\s*\n+/g, '\n\n')
      .replace(/^([ \t]+)/gm, match => {
        const spaces = match.replace(/\t/g, '    ').length;
        return ' '.repeat(Math.max(1, Math.ceil(spaces / 2)));
      })
      .trim();
  } /* v8 ignore start */ catch {
    return content;
  } /* v8 ignore stop */
}

export function minifyMarkdownCore(content: string): string {
  try {
    return (
      content
        .replace(/<!--[\s\S]*?-->/g, '')
        // Strip quoted-reply lines (lines starting with ">") — pure redundancy
        // in PR/issue comments where the original message is already in context.
        .replace(/^[ \t]*>.*$/gm, '')
        .replace(/[ \t]+$/gm, '')
        .replace(/\r\n/g, '\n')
        .replace(/\n\s*\n\s*\n+/g, '\n\n')
        .replace(/([^\n])[ \t]{5,}([^\n])/g, '$1 $2')
        .replace(/\s*\|\s*/g, ' | ')
        .replace(/^(#{1,6})[ \t]+/gm, '$1 ')
        .replace(/^(\s*)([-*+]|\d+\.)[ \t]+/gm, '$1$2 ')
        .replace(/\n{3,}(```)/g, '\n\n$1')
        .replace(/(```)\n{3,}/g, '$1\n\n')
        .trim()
    );
  } /* v8 ignore start */ catch {
    return content;
  } /* v8 ignore stop */
}

export function minifyCSSCore(content: string): string {
  try {
    return removeComments(content, 'c-style')
      .replace(/\s+/g, ' ')
      .replace(/\s*([{}:;,])\s*/g, '$1')
      .trim();
  } /* v8 ignore start */ catch {
    return content;
  } /* v8 ignore stop */
}

export function minifyHTMLCore(content: string): string {
  try {
    return removeComments(content, 'html')
      .replace(/\s+/g, ' ')
      .replace(/>\s+</g, '><')
      .trim();
  } /* v8 ignore start */ catch {
    return content;
  } /* v8 ignore stop */
}

export function minifyJavaScriptCore(content: string): string {
  try {
    return removeComments(content, 'c-style')
      .replace(/\s+/g, ' ')
      .replace(/\s*([{}();,:])\s*/g, '$1')
      .split('\n')
      .map(line => line.trim())
      .filter(line => line.length > 0)
      .join('\n');
  } /* v8 ignore start */ catch {
    return content;
  } /* v8 ignore stop */
}

export function minifyWithTerserSync(content: string): {
  content: string;
  failed: boolean;
  reason?: string;
} {
  try {
    if (!content.trim()) {
      return { content, failed: false };
    }

    const result = minify_sync(content, TERSER_OPTIONS);
    return { content: result.code || content, failed: false };
  } catch (error: unknown) {
    return {
      content: minifyJavaScriptCore(content),
      failed: true,
      reason: `Terser sync minification failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
    };
  }
}

function transpileTypeScriptLike(
  content: string,
  filePath: string
): StrategyMinifyResult {
  try {
    if (!content.trim()) {
      return { content, failed: false };
    }

    const result = ts.transpileModule(content, {
      fileName: filePath,
      compilerOptions: TYPESCRIPT_COMPILER_OPTIONS,
      reportDiagnostics: true,
    });
    const errorDiagnostic = result.diagnostics?.find(
      diagnostic => diagnostic.category === ts.DiagnosticCategory.Error
    );

    if (errorDiagnostic) {
      const message = ts.flattenDiagnosticMessageText(
        errorDiagnostic.messageText,
        '\n'
      );
      return {
        content,
        failed: true,
        reason: `TypeScript transpilation failed: ${message}`,
      };
    }

    return { content: result.outputText || content, failed: false };
  } catch (error: unknown) {
    return {
      content,
      failed: true,
      reason: `TypeScript transpilation failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
    };
  }
}

export function minifyTypeScriptLikeSync(
  content: string,
  filePath: string
): StrategyMinifyResult {
  const transpiled = transpileTypeScriptLike(content, filePath);
  if (transpiled.failed) return transpiled;

  try {
    const result = minify_sync(transpiled.content, TERSER_OPTIONS);
    return { content: result.code || transpiled.content, failed: false };
  } catch (error: unknown) {
    return {
      content: transpiled.content,
      failed: true,
      reason: `Terser minification failed after TypeScript transpilation: ${
        error instanceof Error ? error.message : 'Unknown error'
      }`,
    };
  }
}

export async function minifyTypeScriptLike(
  content: string,
  filePath: string
): Promise<StrategyMinifyResult> {
  const transpiled = transpileTypeScriptLike(content, filePath);
  if (transpiled.failed) return transpiled;

  try {
    const result = await minify(transpiled.content, TERSER_OPTIONS);
    return { content: result.code || transpiled.content, failed: false };
  } catch (error: unknown) {
    return {
      content: transpiled.content,
      failed: true,
      reason: `Terser minification failed after TypeScript transpilation: ${
        error instanceof Error ? error.message : 'Unknown error'
      }`,
    };
  }
}

export async function minifyWithTerser(
  content: string
): Promise<{ content: string; failed: boolean; reason?: string }> {
  try {
    if (!content.trim()) {
      return { content, failed: false };
    }

    const result = await minify(content, TERSER_OPTIONS);

    return { content: result.code || content, failed: false };
  } catch (error: unknown) {
    return {
      content,
      failed: true,
      reason: `Terser minification failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
    };
  }
}

export async function minifyCSSAsync(
  content: string
): Promise<{ content: string; failed: boolean; reason?: string }> {
  try {
    const cleanCSS = new CleanCSS({
      level: 2,
      format: false,
      inline: false,
      rebase: false,
    });

    const result = cleanCSS.minify(content);

    if (result.errors && result.errors.length > 0) {
      throw new Error(result.errors.join(', '));
    }

    return { content: result.styles, failed: false };
  } catch (error: unknown) {
    return {
      content: minifyCSSCore(content),
      failed: false,
      reason: `CleanCSS fallback: ${error instanceof Error ? error.message : 'unknown'}`,
    };
  }
}

type EmbeddedBlockReplacer = (
  attributes: string,
  body: string
) => Promise<string>;

async function replaceEmbeddedBlocks(
  content: string,
  pattern: RegExp,
  replacer: EmbeddedBlockReplacer
): Promise<string> {
  let output = '';
  let lastIndex = 0;
  let match: RegExpExecArray | null = pattern.exec(content);

  while (match) {
    const matchIndex = match.index;
    const fullMatch = match[0] ?? '';
    const attributes = match[1] ?? '';
    const body = match[2] ?? '';

    output += content.slice(lastIndex, matchIndex);
    output += await replacer(attributes, body);
    lastIndex = matchIndex + fullMatch.length;
    match = pattern.exec(content);
  }

  return output + content.slice(lastIndex);
}

function isTypeScriptLikeScript(attributes: string): boolean {
  return /\blang\s*=\s*["']tsx?["']/i.test(attributes);
}

async function minifyEmbeddedScript(
  attributes: string,
  body: string
): Promise<string> {
  const result = isTypeScriptLikeScript(attributes)
    ? await minifyTypeScriptLike(body, 'component.tsx')
    : await minifyWithTerser(body);
  const fallback = minifyJavaScriptCore(body);
  const candidate =
    !result.failed && result.content.length <= body.length
      ? result.content
      : fallback;
  const minifiedBody = candidate.length < body.length ? candidate : body.trim();

  return `<script${attributes}>${minifiedBody}</script>`;
}

async function minifyEmbeddedStyle(
  attributes: string,
  body: string
): Promise<string> {
  const result = await minifyCSSAsync(body);
  const fallback = minifyCSSCore(body);
  const candidate =
    !result.failed && result.content.length <= body.length
      ? result.content
      : fallback;
  const minifiedBody = candidate.length < body.length ? candidate : body.trim();

  return `<style${attributes}>${minifiedBody}</style>`;
}

export async function minifyHTMLAsync(
  content: string
): Promise<{ content: string; failed: boolean; reason?: string }> {
  try {
    if (!content.trim()) {
      return { content, failed: false };
    }

    const result = await htmlMinifierTerser(content, {
      collapseWhitespace: true,
      removeComments: true,
      removeRedundantAttributes: true,
      removeScriptTypeAttributes: true,
      removeStyleLinkTypeAttributes: true,
      minifyCSS: true,
      minifyJS: TERSER_OPTIONS,
      caseSensitive: false,
    });

    return { content: result, failed: false };
  } catch (error: unknown) {
    return {
      content: minifyHTMLCore(content),
      failed: false,
      reason: `html-minifier fallback: ${error instanceof Error ? error.message : 'unknown'}`,
    };
  }
}

export async function minifyComponentAsync(
  content: string
): Promise<{ content: string; failed: boolean; reason?: string }> {
  try {
    if (!content.trim()) {
      return { content, failed: false };
    }

    const withScripts = await replaceEmbeddedBlocks(
      content,
      /<script\b([^>]*)>([\s\S]*?)<\/script>/gi,
      minifyEmbeddedScript
    );
    const withStyles = await replaceEmbeddedBlocks(
      withScripts,
      /<style\b([^>]*)>([\s\S]*?)<\/style>/gi,
      minifyEmbeddedStyle
    );
    const compact = minifyHTMLCore(withStyles);

    return {
      content: compact.length < withStyles.length ? compact : withStyles.trim(),
      failed: false,
    };
  } catch (error: unknown) {
    return {
      content: minifyHTMLCore(content),
      failed: false,
      reason: `component minifier fallback: ${
        error instanceof Error ? error.message : 'unknown'
      }`,
    };
  }
}
