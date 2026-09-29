/**
 * Path display helpers — shared across commands.
 */

import { homedir } from 'node:os';

const HOME = homedir();

/**
 * Shorten an absolute path for display by replacing a leading home directory with ~.
 * Stable across platforms (uses os.homedir() rather than $HOME env var).
 */
export function shortPath(p: string): string {
  if (!HOME) return p;
  if (p === HOME) return '~';
  if (
    p.startsWith(HOME) &&
    (p[HOME.length] === '/' || p[HOME.length] === '\\')
  ) {
    return `~${p.slice(HOME.length)}`;
  }
  return p;
}
