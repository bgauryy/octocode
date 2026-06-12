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

type MarkdownFenceState = {
  readonly marker: string;
  readonly length: number;
};

type HtmlCommentStripResult = {
  readonly line: string;
  readonly inComment: boolean;
};

function markdownFenceStart(line: string): MarkdownFenceState | null {
  const match = /^( {0,3})(`{3,}|~{3,})/.exec(line);
  if (!match) return null;

  const fence = match[2] ?? '';
  return {
    marker: fence[0] ?? '',
    length: fence.length,
  };
}

function isMarkdownFenceClose(
  line: string,
  fence: MarkdownFenceState
): boolean {
  const match = /^( {0,3})(`+|~+)[ \t]*$/.exec(line);
  if (!match) return false;

  const closing = match[2] ?? '';
  return closing[0] === fence.marker && closing.length >= fence.length;
}

function isMarkdownIndentedCode(line: string): boolean {
  return /^(?: {4}|\t)/.test(line);
}

function stripMarkdownHtmlComments(
  line: string,
  inComment: boolean
): HtmlCommentStripResult {
  let output = '';
  let cursor = 0;
  let insideComment = inComment;

  while (cursor < line.length) {
    if (insideComment) {
      const end = line.indexOf('-->', cursor);
      if (end === -1) {
        return { line: output, inComment: true };
      }
      cursor = end + 3;
      insideComment = false;
      continue;
    }

    const start = line.indexOf('<!--', cursor);
    if (start === -1) {
      output += line.slice(cursor);
      break;
    }

    output += line.slice(cursor, start);
    const end = line.indexOf('-->', start + 4);
    if (end === -1) {
      return { line: output, inComment: true };
    }

    cursor = end + 3;
  }

  return { line: output, inComment: insideComment };
}

function isMarkdownPseudoCommentLine(line: string): boolean {
  return /^\s*\[\/\/\]:\s*#/.test(line);
}

function isMarkdownGeneratedTocStart(line: string): boolean {
  return /<!--\s*(?:start\s+)?(?:toc|table of contents|doctoc|markdown-toc)\b[^>]*-->/i.test(
    line
  );
}

function isMarkdownGeneratedTocEnd(line: string): boolean {
  return /<!--\s*(?:(?:end|\/)\s*(?:toc|table of contents|doctoc|markdown-toc)\b|tocstop\b)[^>]*-->/i.test(
    line
  );
}

function isMarkdownBadgeUrl(url: string): boolean {
  const normalizedUrl = url.toLowerCase();
  return (
    normalizedUrl.includes('img.shields.io') ||
    normalizedUrl.includes('badge.fury.io') ||
    normalizedUrl.includes('badgen.net') ||
    normalizedUrl.includes('codecov.io') ||
    normalizedUrl.includes('coveralls.io') ||
    normalizedUrl.includes('circleci.com') ||
    normalizedUrl.includes('travis-ci.com') ||
    normalizedUrl.includes('travis-ci.org') ||
    /github\.com\/[^/]+\/[^/]+\/(?:workflows|actions\/workflows)\/[^)]+badge\.svg/.test(
      normalizedUrl
    )
  );
}

function isMarkdownBadgeImageLine(line: string): boolean {
  const trimmed = line.trim();
  const imageMatches = [
    ...trimmed.matchAll(/!\[[^\]]*]\((https?:\/\/[^)\s]+)(?:\s+[^)]*)?\)/g),
  ];
  if (imageMatches.length === 0) return false;
  if (!imageMatches.every(match => isMarkdownBadgeUrl(match[1] ?? ''))) {
    return false;
  }

  const withoutBadges = trimmed
    .replace(/\[!\[[^\]]*]\([^)]*\)]\([^)]*\)/g, '')
    .replace(/!\[[^\]]*]\([^)]*\)/g, '')
    .trim();

  return withoutBadges.length === 0;
}

function markdownSetextLevel(line: string): 1 | 2 | null {
  if (/^ {0,3}=+[ \t]*$/.test(line)) return 1;
  if (/^ {0,3}-+[ \t]*$/.test(line)) return 2;
  return null;
}

function canBeMarkdownSetextText(line: string): boolean {
  const trimmed = line.trim();
  if (!trimmed) return false;
  if (/^#{1,6}(?:\s|$)/.test(trimmed)) return false;
  if (trimmed.startsWith('>')) return false;
  if (/^(?:[-+*]|\d{1,9}[.)])(?:\s|$)/.test(trimmed)) return false;
  if (isMarkdownThematicBreak(line)) return false;
  return markdownFenceStart(line) === null;
}

function isMarkdownThematicBreak(line: string): boolean {
  const compact = line.trim().replace(/[ \t]/g, '');
  if (compact.length < 3) return false;

  const marker = compact[0] ?? '';
  return (
    (marker === '-' || marker === '_' || marker === '*') &&
    [...compact].every(char => char === marker)
  );
}

function compactMarkdownAtxHeading(line: string): string {
  const match = /^( {0,3})(#{1,6})(?:[ \t]+(.*))?$/.exec(line);
  if (!match) return line;

  const indent = match[1] ?? '';
  const hashes = match[2] ?? '';
  const text = (match[3] ?? '').replace(/[ \t]+#+[ \t]*$/, '').trim();

  return text ? `${indent}${hashes} ${text}` : `${indent}${hashes}`;
}

function compactMarkdownBlockquote(line: string): string {
  const match = /^( {0,3})((?:>[ \t]*)+)(.*)$/.exec(line);
  if (!match) return line;

  const indent = match[1] ?? '';
  const markerRun = match[2] ?? '';
  const text = (match[3] ?? '').trimStart();
  const depth = [...markerRun].filter(char => char === '>').length;
  const markers = Array.from({ length: depth }, () => '>').join(' ');

  return text ? `${indent}${markers} ${text}` : `${indent}${markers}`;
}

function compactMarkdownListMarker(line: string): string {
  return line
    .replace(/^(\s{0,3}(?:[-+*]|\d{1,9}[.)]))[ \t]+/, '$1 ')
    .replace(/^(\s{0,3}(?:[-+*]|\d{1,9}[.)]) )\[([ xX])][ \t]+/, '$1[$2] ');
}

function splitOnUnescapedPipes(line: string): readonly string[] {
  const parts: string[] = [];
  let current = '';
  let escaped = false;

  for (const char of line) {
    if (char === '|' && !escaped) {
      parts.push(current);
      current = '';
      escaped = false;
      continue;
    }

    current += char;
    escaped = char === '\\' && !escaped;
    if (char !== '\\') escaped = false;
  }

  parts.push(current);
  return parts;
}

function isMarkdownTableDelimiterLine(line: string): boolean {
  const parts = splitOnUnescapedPipes(line.trim()).filter(part => part !== '');
  if (parts.length < 2) return false;

  return parts.every(part => /^:?-{3,}:?$/.test(part.trim()));
}

function compactMarkdownTableRow(line: string): string {
  return splitOnUnescapedPipes(line)
    .map(part => part.trim())
    .join('|');
}

function trimMarkdownLineEnd(line: string): string {
  if (/\S {2,}$/.test(line) && !/\\[ \t]*$/.test(line)) {
    return line.replace(/[ \t]+$/, '\\');
  }

  return line.replace(/[ \t]+$/, '');
}

function isMarkdownReferenceDefinition(line: string): boolean {
  return /^ {0,3}\[[^\]]+]:[ \t]*\S/.test(line);
}

function isMarkdownRawHtmlLine(line: string): boolean {
  return /^ {0,3}<[A-Za-z!/][^>]*>/.test(line);
}

function isMarkdownDiffMetadataLine(line: string): boolean {
  return /^(?:@@\s|diff --git |index [0-9a-f]|--- |\+\+\+ )/.test(line);
}

function splitMarkdownControlPrefix(line: string): {
  prefix: string;
  text: string;
} {
  const indented = /^( {0,3})(.*)$/.exec(line);
  let prefix = indented?.[1] ?? '';
  let text = indented?.[2] ?? line;

  const diffContent = /^([+-])(?=\S| {2,})(.*)$/.exec(text);
  if (diffContent && !/^(?:---|\+\+\+)/.test(text)) {
    prefix += diffContent[1] ?? '';
    text = diffContent[2] ?? '';
  }

  const blockquote = /^((?:> ?)+)(.*)$/.exec(text);
  if (blockquote) {
    prefix += blockquote[1] ?? '';
    text = blockquote[2] ?? '';

    const callout =
      /^(\[!(?:NOTE|TIP|IMPORTANT|WARNING|CAUTION)]\s+)(.*)$/i.exec(text);
    if (callout) {
      prefix += callout[1] ?? '';
      text = callout[2] ?? '';
    }
  }

  const heading = /^(#{1,6} )(.*)$/.exec(text);
  if (heading) {
    return { prefix: prefix + (heading[1] ?? ''), text: heading[2] ?? '' };
  }

  const list = /^((?:[-+*]|\d{1,9}[.)]) (?:\[[ xX]\] )?)(.*)$/.exec(text);
  if (list) {
    return { prefix: prefix + (list[1] ?? ''), text: list[2] ?? '' };
  }

  return { prefix, text };
}

function sanitizeMarkdownVisibleText(text: string): string {
  const hardBreakSuffix = text.endsWith('\\') ? '\\' : '';
  const body = hardBreakSuffix ? text.slice(0, -1) : text;
  const protectedSpans: string[] = [];
  const protectSpan = (value: string): string => {
    const index = protectedSpans.push(value) - 1;
    return `\ue000${index}\ue001`;
  };

  const protectedBody = body
    .replace(/`[^`\n]*`/g, protectSpan)
    .replace(/!?\[[^\]\n]*]\([^)]+\)/g, protectSpan)
    .replace(/!?\[[^\]\n]*]\[[^\]\n]*]/g, protectSpan)
    .replace(/<https?:\/\/[^>\s]+>/gi, protectSpan)
    .replace(/<mailto:[^>\s]+>/gi, protectSpan);

  const sanitized = protectedBody
    .replace(/[‘’]/g, "'")
    .replace(/[“”]/g, '"')
    .replace(/[–—]/g, '-')
    .replace(/…/g, '...')
    .replace(/\u00a0/gu, ' ')
    .replace(/\p{Extended_Pictographic}/gu, '')
    .replace(/\u200d/gu, '')
    .replace(/\ufe0f/gu, '')
    .replace(/[\u{1f3fb}-\u{1f3ff}]/gu, '')
    .replace(/[^\p{L}\p{N}\p{M}\s\x20-\x7e\ue000\ue001]/gu, ' ')
    .replace(/[ \t]{2,}/g, ' ')
    .trim();

  const restored = sanitized.replace(
    /\ue000(\d+)\ue001/g,
    (_match: string, index: string): string =>
      protectedSpans[Number(index)] ?? ''
  );

  return `${restored}${hardBreakSuffix}`;
}

function sanitizeMarkdownProseLine(line: string): string {
  if (
    isMarkdownReferenceDefinition(line) ||
    isMarkdownRawHtmlLine(line) ||
    isMarkdownDiffMetadataLine(line)
  ) {
    return line;
  }

  const { prefix, text } = splitMarkdownControlPrefix(line);
  const sanitizedText = sanitizeMarkdownVisibleText(text);
  if (!sanitizedText) return prefix.trimEnd();

  return `${prefix}${sanitizedText}`;
}

function compactMarkdownTextLine(line: string): string {
  const compacted = compactMarkdownListMarker(
    compactMarkdownBlockquote(
      compactMarkdownAtxHeading(trimMarkdownLineEnd(line))
    )
  ).replace(/([^ \t])[ \t]{5,}([^ \t])/g, '$1 $2');

  return sanitizeMarkdownProseLine(compacted);
}

function appendMarkdownLine(
  lines: string[],
  line: string,
  preserveBlank: boolean
): void {
  if (line.trim().length > 0 || preserveBlank) {
    lines.push(line);
    return;
  }

  if (lines.length === 0 || lines[lines.length - 1]?.trim().length === 0) {
    return;
  }

  lines.push('');
}

function convertPreviousSetextHeading(lines: string[], level: 1 | 2): boolean {
  const headingLines: string[] = [];

  while (lines.length > 0) {
    const candidate = lines[lines.length - 1] ?? '';
    if (candidate.trim().length === 0) break;
    if (!canBeMarkdownSetextText(candidate)) return false;

    headingLines.unshift(candidate.trim());
    lines.pop();
  }

  if (headingLines.length === 0) return false;
  appendMarkdownLine(
    lines,
    `${level === 1 ? '#' : '##'} ${headingLines.join(' ')}`,
    false
  );
  return true;
}

export function minifyMarkdownCore(content: string): string {
  try {
    const sourceLines = content.replace(/\r\n?/g, '\n').split('\n');
    const outputLines: string[] = [];
    const firstContentLine = sourceLines.findIndex(line => line.trim() !== '');
    let fence: MarkdownFenceState | null = null;
    let inHtmlComment = false;
    let inGeneratedToc = false;
    let inFrontmatter =
      firstContentLine === 0 && /^---[ \t]*$/.test(sourceLines[0] ?? '');

    for (let index = 0; index < sourceLines.length; index++) {
      const originalLine = sourceLines[index] ?? '';

      if (fence) {
        appendMarkdownLine(outputLines, originalLine, true);
        if (isMarkdownFenceClose(originalLine, fence)) {
          fence = null;
        }
        continue;
      }

      const fenceStart = markdownFenceStart(originalLine);
      if (fenceStart) {
        fence = fenceStart;
        appendMarkdownLine(
          outputLines,
          trimMarkdownLineEnd(originalLine),
          true
        );
        continue;
      }

      if (isMarkdownIndentedCode(originalLine)) {
        appendMarkdownLine(outputLines, originalLine, true);
        continue;
      }

      if (inFrontmatter) {
        appendMarkdownLine(
          outputLines,
          trimMarkdownLineEnd(originalLine),
          false
        );
        if (index > 0 && /^(---|\.\.\.)[ \t]*$/.test(originalLine)) {
          inFrontmatter = false;
        }
        continue;
      }

      if (inGeneratedToc) {
        if (isMarkdownGeneratedTocEnd(originalLine)) {
          inGeneratedToc = false;
          appendMarkdownLine(outputLines, '', false);
        }
        continue;
      }

      if (isMarkdownGeneratedTocStart(originalLine)) {
        inGeneratedToc = !isMarkdownGeneratedTocEnd(originalLine);
        appendMarkdownLine(outputLines, '', false);
        continue;
      }

      const withoutHtmlComment = stripMarkdownHtmlComments(
        originalLine,
        inHtmlComment
      );
      inHtmlComment = withoutHtmlComment.inComment;
      const line = withoutHtmlComment.line;
      if (isMarkdownPseudoCommentLine(line) || isMarkdownBadgeImageLine(line)) {
        appendMarkdownLine(outputLines, '', false);
        continue;
      }

      const setextLevel = markdownSetextLevel(line);
      if (
        setextLevel &&
        convertPreviousSetextHeading(outputLines, setextLevel)
      ) {
        continue;
      }

      if (isMarkdownThematicBreak(line)) {
        appendMarkdownLine(outputLines, '---', false);
        continue;
      }

      const isTableRow =
        isMarkdownTableDelimiterLine(line) ||
        isMarkdownTableDelimiterLine(sourceLines[index - 1] ?? '') ||
        isMarkdownTableDelimiterLine(sourceLines[index + 1] ?? '');
      const compactedLine = isTableRow
        ? compactMarkdownTableRow(trimMarkdownLineEnd(line))
        : compactMarkdownTextLine(line);

      appendMarkdownLine(outputLines, compactedLine, false);
    }

    return outputLines.join('\n').trim();
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
