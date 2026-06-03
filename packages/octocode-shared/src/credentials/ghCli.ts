/**
 * GitHub CLI token retrieval using async child_process.
 * Shared default implementation used by resolveTokenFull.
 * Both octocode-mcp and octocode-cli can override via the getGhCliToken option
 * on resolveTokenFull for testing or custom spawn logic.
 */

import { execFile } from 'child_process';

export function getGhCliToken(hostname?: string): Promise<string | null> {
  return new Promise(resolve => {
    const args = ['auth', 'token'];
    if (hostname) args.push('--hostname', hostname);
    execFile(
      'gh',
      args,
      { encoding: 'utf8', timeout: 5000 },
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
