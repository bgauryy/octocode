/**
 * Bundled-ripgrep path resolver.
 *
 * We ship `@vscode/ripgrep` so users don't have to install ripgrep separately.
 * Its package exports an absolute `rgPath` constant. The local search tool is
 * intentionally bundled-ripgrep only; if the bundled binary is unavailable,
 * startup/execution should fail loudly instead of drifting to a host-specific
 * PATH binary.
 *
 * @module utils/exec/ripgrepBinary
 */

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
    // require() inline so a missing @vscode/ripgrep at runtime produces a
    // precise local-search dependency error when the tool is used.
    const mod = require('@vscode/ripgrep') as { rgPath?: string };
    if (mod.rgPath && typeof mod.rgPath === 'string') {
      return mod.rgPath;
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`Bundled ripgrep is unavailable: ${message}`);
  }
  throw new Error(
    'Bundled ripgrep is unavailable: @vscode/ripgrep did not export rgPath'
  );
}
