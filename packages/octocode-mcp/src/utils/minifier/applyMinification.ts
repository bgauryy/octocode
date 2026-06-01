import { minifyContentSync } from './minifier.js';

/**
 * Apply sync minification to content, keeping the result ONLY when it is
 * smaller than the original (otherwise the verbatim input is returned).
 *
 * Shared by both fetch-content verbosity finalizers (github + local). It lives
 * here — beside the minifier it wraps — rather than under either tool's folder,
 * so neither tool reaches across into the other for it (#4). Kept as a separate
 * module from `minifier.ts` so tests can stub `minifyContentSync` while still
 * exercising the real wrapper.
 *
 * @param content - The content to minify.
 * @param filePath - File path used to pick the minification strategy.
 * @returns Minified content if smaller, otherwise the original.
 */
export function applyMinification(content: string, filePath: string): string {
  try {
    const minifiedContent = minifyContentSync(content, filePath);
    // Only use minified version if it's actually smaller
    return minifiedContent.length < content.length ? minifiedContent : content;
  } catch {
    // Keep original if minification fails
    return content;
  }
}
