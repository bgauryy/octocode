import { minify } from 'terser';
import CleanCSS from 'clean-css';
import { minify as htmlMinifierTerser } from 'html-minifier-terser';
import type {
  CommentPatternGroup,
  FileTypeMinifyConfig,
} from '../types/index.js';
import { MINIFY_CONFIG } from '../types/index.js';

export function removeComments(
  content: string,
  commentTypes: CommentPatternGroup | CommentPatternGroup[]
): string {
  try {
    let result = content;
    const types = Array.isArray(commentTypes) ? commentTypes : [commentTypes];

    for (const type of types) {
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

export async function minifyWithTerser(
  content: string
): Promise<{ content: string; failed: boolean; reason?: string }> {
  try {
    if (!content.trim()) {
      return { content, failed: false };
    }

    const result = await minify(content, {
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
    });

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
      minifyCSS: false,
      minifyJS: false,
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
