/* Auto-generated types for @octocodeai/octocode-minifier-utils
 * Drop-in replacement for @octocodeai/octocode-minifier — same API surface.
 */

export type CommentPatternGroup =
  | 'c-style' | 'hash' | 'html' | 'sql' | 'lua' | 'template'
  | 'haskell' | 'semicolon' | 'wasm-text' | 'percent' | 'haml'
  | 'slim' | 'powershell' | 'bang' | 'apostrophe' | 'double-dash'
  | 'fsharp-block' | 'pascal' | 'python-docstring';

export type Strategy =
  | 'terser' | 'conservative' | 'aggressive' | 'json' | 'general' | 'markdown';

export type MinificationMode = 'content-view' | 'minify' | 'symbols';

export interface GetExtensionOptions {
  lowercase?: boolean;
  fallback?: string;
}

export interface MinifyResult {
  content: string;
  failed: boolean;
  type:    Strategy | 'failed';
  reason?: string;
}

export interface FileTypeMinifyConfig {
  strategy: Strategy;
  comments?: CommentPatternGroup | CommentPatternGroup[];
}

export interface KeptLine {
  lineNumber: number;
  text:       string;
}

export interface YamlConversionConfig {
  sortKeys?:      boolean;
  keysPriority?:  string[];
}

// ── File-extension util ──────────────────────────────────────────────────────
export function getExtension(filePath: string, options?: GetExtensionOptions): string;

// ── Core minification API ────────────────────────────────────────────────────
export function minifyContentSync(content: string, filePath: string): string;
/** Async-compatible drop-in for TS `minifyContent`. Rust runs sync; result wrapped in Promise.resolve(). */
export function minifyContent(content: string, filePath: string): Promise<MinifyResult>;
/** Sync version of minifyContent — same logic without the Promise wrapper. */
export function minifyContentResult(content: string, filePath: string): MinifyResult;
export function applyMinification(content: string, filePath: string): string;
export function applyContentViewMinification(content: string, filePath: string): string;

// ── Fine-grained strategy exports ────────────────────────────────────────────
export function removeComments(
  content: string,
  commentTypes: CommentPatternGroup | CommentPatternGroup[]
): string;

export function minifyConservativeCore(content: string, config: FileTypeMinifyConfig): string;
export function minifyAggressiveCore(content: string, config: FileTypeMinifyConfig): string;
export function minifyJsonCore(content: string): MinifyResult;
export function minifyJsonReadable(content: string): MinifyResult;
export function minifyCodeCore(content: string): string;
export function minifyGeneralCore(content: string): string;
export function minifyMarkdownCore(content: string): string;
export function minifyCSSCore(content: string): string;
export function minifyHTMLCore(content: string): string;
export function minifyJavaScriptCore(content: string): string;
/** High-quality CSS minification via lightningcss. Falls back to minifyCSSCore on parse error. */
export function minifyCSSQuality(content: string): string;
/** High-quality HTML minification via minify-html crate. Falls back to minifyHTMLCore on error. */
export function minifyHTMLQuality(content: string): string;
/** Strip Python triple-quoted docstrings from content. */
export function stripPythonDocstrings(content: string): string;

// ── Signature / skeleton extraction ─────────────────────────────────────────
/**
 * Returns a `NNN| text` skeleton with bodies stripped.
 * Uses tree-sitter for: ts, tsx, js, jsx, mjs, cjs, py, go, rs, java, c, h,
 * cpp, hpp, cc, cs, sh, bash, zsh.  Falls back to heuristic for all others.
 */
export function extractSignatures(content: string, filePath: string): string | null;

export const SIGNATURES_ONLY_HINT: string;
export const SUPPORTED_SIGNATURE_EXTENSIONS: ReadonlyArray<string>;

// ── YAML serialization ───────────────────────────────────────────────────────
export function jsonToYamlString(jsonObject: unknown, config?: YamlConversionConfig): string;
