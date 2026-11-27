/** Communication storage location, using the shared Octocode home policy of @octocodeai/config. */
import { getOctocodeHome } from '@octocodeai/config';
import { join, resolve } from 'node:path';

export function databasePath(override?: string | null): string {
  if (override != null) return resolve(override);
  return join(getOctocodeHome(), 'agents-communication', 'communication.sqlite');
}
