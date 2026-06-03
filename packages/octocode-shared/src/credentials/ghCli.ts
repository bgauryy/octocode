/**
 * GitHub CLI token retrieval using async child_process.
 * Shared default implementation used by resolveTokenFull.
 * Both octocode-mcp and octocode-cli can override via the getGhCliToken option
 * on resolveTokenFull for testing or custom spawn logic.
 */

import { execFile } from 'child_process';

/**
 * Common gh installation prefixes across macOS (Homebrew) and Linux (linuxbrew).
 * Prepended to PATH so gh resolves even when the parent process was launched
 * with a stripped PATH (e.g. an IDE launching an MCP server via a minimal env).
 */
const COMMON_GH_PATHS = [
  '/opt/homebrew/bin',
  '/usr/local/bin',
  '/home/linuxbrew/.linuxbrew/bin',
];

export function getGhCliToken(hostname?: string): Promise<string | null> {
  return new Promise(resolve => {
    const args = ['auth', 'token'];
    if (hostname) args.push('--hostname', hostname);

    // Augment PATH with common installation prefixes so gh resolves even in
    // stripped-PATH environments (e.g. Cursor/VS Code launching MCP servers).
    const existingPath = process.env.PATH ?? '';
    const existingParts = new Set(existingPath.split(':'));
    const extra = COMMON_GH_PATHS.filter(p => !existingParts.has(p));
    const augmentedPath = extra.length
      ? `${extra.join(':')}:${existingPath}`
      : existingPath;

    execFile(
      'gh',
      args,
      {
        encoding: 'utf8',
        timeout: 5000,
        env: { ...process.env, PATH: augmentedPath },
      },
      (error, stdout) => {
        if (error || !stdout) {
          resolve(null);
          return;
        }
        resolve(stdout.trim() || null);
      }
    );
  });
}
