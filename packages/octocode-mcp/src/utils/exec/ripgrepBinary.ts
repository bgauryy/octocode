/**
 * Bundled-ripgrep path resolver (T3.3).
 *
 * We ship `@vscode/ripgrep` so users don't have to install ripgrep
 * separately. Its binary lives in
 * `node_modules/@vscode/ripgrep/bin/rg(.exe)` and the package exports
 * an absolute `rgPath` constant.
 *
 * If the bundled binary is missing for any reason (e.g. a downstream
 * postinstall failure, or a platform without prebuilt support), we
 * fall back to the literal string `'rg'` which `safeExec` resolves
 * against `PATH`. The contract is: this resolver **never throws** —
 * callers always get a usable command string.
 *
 * @module utils/exec/ripgrepBinary
 */

/** Fallback when the bundled binary cannot be located. */
export const RIPGREP_PATH_FALLBACK = 'rg';

let cachedPath: string | null = null;

/**
 * Return an absolute path to the bundled ripgrep binary, or `'rg'` as
 * a last resort. The result is memoised because resolving the path is
 * a synchronous import that we don't want to pay for on every search.
 */
export function resolveRipgrepBinary(): string {
  if (cachedPath !== null) return cachedPath;
  cachedPath = computePath();
  return cachedPath;
}

function computePath(): string {
  try {
    // require() inline so a missing @vscode/ripgrep at runtime
    // degrades gracefully to the PATH-lookup fallback. We avoid a
    // top-level import for the same reason.

    const mod = require('@vscode/ripgrep') as { rgPath?: string };
    if (mod.rgPath && typeof mod.rgPath === 'string') {
      return mod.rgPath;
    }
  } catch {
    // Package missing / download script never ran on this platform.
  }
  return RIPGREP_PATH_FALLBACK;
}
