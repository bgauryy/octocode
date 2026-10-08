/**
 * The Octocode home directory and the config file paths under it, on every
 * platform: `OCTOCODE_HOME` when set, else `<os.homedir()>/.octocode`.
 * python/octocode_config.py implements the same rule for Python skills.
 */
import { homedir } from 'node:os';
import path from 'node:path';

export function getOctocodeHome(env: Record<string, string | undefined> = process.env): string {
  const override = env['OCTOCODE_HOME'];
  if (override && override.trim()) return path.resolve(override.trim());
  return path.join(homedir(), '.octocode');
}

/** Absolute path to the global `<home>/.octocoderc` config file. */
export function getConfigFilePath(home: string = getOctocodeHome()): string {
  return path.join(home, '.octocoderc');
}

/** Absolute path to the workspace `<cwd>/.octocode/.octocoderc` config file. */
export function getProjectConfigFilePath(cwd: string = process.cwd()): string {
  return path.join(path.resolve(cwd), '.octocode', '.octocoderc');
}
