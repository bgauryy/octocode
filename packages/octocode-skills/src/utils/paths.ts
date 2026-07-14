/**
 * Path display helpers — shared across commands.
 */

import os from 'node:os';

const HOME = os.homedir();

/**
 * Shorten an absolute path for display by replacing the home directory with ~.
 * Stable across platforms (uses os.homedir() rather than $HOME env var).
 */
export function shortPath(p: string): string {
  return HOME ? p.replace(HOME, '~') : p;
}
