import { minifyContentSync } from './minifier.js';
import {
  minifyJsonReadable,
  minifyCodeCore,
  minifyGeneralCore,
  minifyMarkdownCore,
  removeComments,
} from './strategies.js';
import { MINIFY_CONFIG } from '../types/index.js';
import { getExtension } from '../utils/fileExtension.js';

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
 * Content-safe minification for file viewing tools (localGetFileContent,
 * githubGetFileContent, PR patches).
 *
 * Designed for AGENT READABILITY — preserves structure and indentation.
 *
 * Pipeline per file type:
 *   JSON/JSONC/JSON5  → minifyJsonReadable (strip JSONC comments/trailing commas
 *                        then re-pretty-print with 2-space indent — readable);
 *                        only returned if result is shorter (i.e. JSONC had comments)
 *   Markdown          → minifyMarkdownCore (HTML comments, quote-replies stripped)
 *   Files with comments config → removeComments(configured) + minifyCodeCore
 *                                (trailing whitespace + blank-line compression;
 *                                 original indentation PRESERVED for agent readability)
 *   Plain text (txt/log/unknown) → minifyGeneralCore (whitespace + indent compression)
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
      // Readable JSON: strip JSONC noise, keep pretty-printing.
      // Falls back to original when nothing was stripped (already clean JSON).
      minified = minifyJsonReadable(content).content;
    } else if (MINIFY_CONFIG.fileTypes[ext]?.strategy === 'markdown') {
      minified = minifyMarkdownCore(content);
    } else {
      const commentType = MINIFY_CONFIG.fileTypes[ext]?.comments;
      const stripped = commentType
        ? removeComments(content, commentType)
        : content;
      // Code files: preserve indentation so agents keep structural context.
      // Plain-text files (txt/log/no registered type): allow indent compression.
      const hasRegisteredType = Boolean(MINIFY_CONFIG.fileTypes[ext]);
      minified = hasRegisteredType
        ? minifyCodeCore(stripped)
        : minifyGeneralCore(stripped);
    }

    return minified.length < content.length ? minified : content;
  } catch {
    return content;
  }
}

export { MINIFY_CONFIG };

// Skeleton (minify:"symbols") extraction lives in its own strategy-based module
// (AST for ts-js, lean heuristics for other families); re-exported here so
// both file-content tools keep their stable import path.
export {
  extractSignatures,
  SIGNATURES_ONLY_HINT,
} from '../signatures/extractSignatures.js';
