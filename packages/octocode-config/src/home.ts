/**
 * getOctocodeHome — single source of truth for the Octocode home directory.
 * Kept in its own file so config/loader.ts can import it without creating a
 * circular dependency with the main index.ts.
 *
 * Unified home per platform (matches octocode-tools-core/src/shared/paths.ts):
 *   macOS:   ~/.octocode
 *   Linux:   ${XDG_CONFIG_HOME:-~/.config}/.octocode
 *   Windows: %APPDATA%\.octocode
 *   override: OCTOCODE_HOME
 */
import os from 'node:os';
import path from 'node:path';

export function getOctocodeHome(env: Record<string, string | undefined> = process.env): string {
  const override = env['OCTOCODE_HOME'];
  if (override && override.trim()) return path.resolve(override.trim());
  const home = os.homedir();
  const platform = os.platform();
  if (platform === 'win32') {
    const appData = env['APPDATA'] ?? path.join(home, 'AppData', 'Roaming');
    return path.join(appData, '.octocode');
  }
  if (platform === 'darwin') return path.join(home, '.octocode');
  const xdg = env['XDG_CONFIG_HOME'] ?? path.join(home, '.config');
  return path.join(xdg, '.octocode');
}
