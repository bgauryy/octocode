import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const extensionDir = path.dirname(fileURLToPath(import.meta.url));

export interface AssetPaths {
  baseDir: string;
  docsDir: string;
  skillsDir: string;
  systemPrompt: string;
  /** Absolute path to the bundled octocode CLI entry point (dist/cli/octocode.js). */
  cliPath: string;
}

export function getAssetPaths(baseDir = extensionDir): AssetPaths {
  return {
    baseDir,
    docsDir: path.join(baseDir, 'docs'),
    skillsDir: path.join(baseDir, 'skills'),
    systemPrompt: path.join(baseDir, 'system', 'SYSTEM_PROMPT.md'),
    cliPath: path.join(baseDir, 'cli', 'octocode.js'),
  };
}

/**
 * Returns the absolute path to the bundled octocode CLI entry point.
 * Agents run it with: `node <cliPath> <command>`
 * Also exposed via the OCTOCODE_CLI env var (set at extension load).
 */
export function getCLIPath(baseDir = extensionDir): string {
  return path.join(baseDir, 'cli', 'octocode.js');
}

/**
 * Returns the absolute path to the bundled octocode-awareness CLI entry point
 * (the standalone script shipped with the awareness skill). Agents run it with:
 * `node <awarenessCliPath> <noun> <verb>`. Also exposed via the
 * OCTOCODE_AWARENESS_CLI env var (set at extension load) so bash-spawned CLI
 * calls share the same interface the skill documents.
 */
export function getAwarenessCLIPath(baseDir = extensionDir): string {
  const bundled = path.join(baseDir, 'skills', 'octocode-awareness', 'scripts', 'awareness.mjs');
  if (fs.existsSync(bundled)) return bundled;
  // Source-mode tests/dev run from src/, while the build/runtime runs from
  // dist/. Both consume the same generated package-level skill tree.
  const sourceMode = path.join(path.dirname(baseDir), 'skills', 'octocode-awareness', 'scripts', 'awareness.mjs');
  return fs.existsSync(sourceMode) ? sourceMode : bundled;
}

export function readTextIfExists(filePath: string): string {
  try {
    return fs.readFileSync(filePath, 'utf8');
  } catch (error) {
    if ((error as NodeJS.ErrnoException)?.code === 'ENOENT') return '';
    throw error;
  }
}

export function listBundledSkills(baseDir = extensionDir): string[] {
  const { skillsDir } = getAssetPaths(baseDir);
  if (!fs.existsSync(skillsDir)) return [];
  return fs
    .readdirSync(skillsDir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .filter((skillName) =>
      fs.existsSync(path.join(skillsDir, skillName, 'SKILL.md')),
    )
    .sort();
}

export function getInstallSource(baseDir = extensionDir): string {
  const packageRoot = path.dirname(baseDir);
  if (
    packageRoot.includes(
      path.join('node_modules', '@octocodeai', 'pi-extension'),
    )
  ) {
    return 'npm:@octocodeai/pi-extension';
  }
  return packageRoot;
}
