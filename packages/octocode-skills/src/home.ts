/**
 * Octocode home directory resolution — inlined here so @octocodeai/skills
 * has zero runtime npm dependencies.
 *
 * Priority: OCTOCODE_HOME env → platform default
 *   macOS:   ~/.octocode
 *   Linux:   ${XDG_CONFIG_HOME:-~/.config}/.octocode
 *   Windows: %APPDATA%\.octocode
 */

import os from 'node:os';
import path from 'node:path';

export function getOctocodeHome(env: Record<string, string | undefined> = process.env): string {
  const override = env['OCTOCODE_HOME'];
  if (override?.trim()) return path.resolve(override.trim());

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

/**
 * Canonical skills home: ~/.octocode/skills/
 * All installed skills have their canonical copy here.
 * Platform / workspace directories symlink INTO this directory.
 */
export function getSkillsHome(): string {
  return path.join(getOctocodeHome(), 'skills');
}
