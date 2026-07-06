import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { memoryHome as resolveMemoryHome } from '@octocodeai/octocode-awareness';

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
 * Awareness memory home: delegates to @octocodeai/octocode-awareness.
 * Kept as a named export for backward compat with external callers.
 */
export function getOctocodeMemoryHome(): string {
  return resolveMemoryHome();
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


