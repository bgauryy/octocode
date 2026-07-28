/**
 * getOctocodeHome — single source of truth for the Octocode home directory.
 * Kept in its own file so config/loader.ts can import it without creating a
 * circular dependency with the main index.ts.
 *
 * Unified home on every platform:
 *   default:  <os.homedir()>/.octocode
 *   override: OCTOCODE_HOME
 */
import { homedir } from 'node:os';
import path from 'node:path';

export function getOctocodeHome(env: Record<string, string | undefined> = process.env): string {
  const override = env['OCTOCODE_HOME'];
  if (override && override.trim()) return path.resolve(override.trim());
  return path.join(homedir(), '.octocode');
}
