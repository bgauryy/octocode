// Home resolution is owned by @octocodeai/config.
import path from 'node:path';

export { getOctocodeHome } from '@octocodeai/config';
import { getOctocodeHome } from '@octocodeai/config';

/**
 * Canonical skills home: <octocode-home>/skills/.
 * All installed skills have their canonical copy here.
 * Platform / workspace directories symlink INTO this directory.
 */
export function getSkillsHome(): string {
  return path.join(getOctocodeHome(), 'skills');
}
