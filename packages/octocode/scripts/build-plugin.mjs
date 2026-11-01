import {
  cpSync,
  existsSync,
  readFileSync,
  readdirSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { stageSkills } from './stage-skills.mjs';

/** @type {(path: string) => any} */
export const readJson = path => JSON.parse(readFileSync(path, 'utf8'));
/** @type {(path: string, value: unknown) => void} */
export const writeJson = (path, value) =>
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);

/**
 * @typedef {object} HostFiles
 * @property {{ version: string }} pkg The plugin package's own `package.json`.
 * @property {Record<string, unknown>} manifest The plugin's `src/plugin.json`.
 * @property {{ type: string; command: string; args: string[] }} mcpServer The `octocode-mcp` server entry.
 */

/**
 * The build every host plugin (Claude Code, Codex) shares: the public skills, including onboarding, staged with `stageSkills`, and the license. The host
 * writes only what differs (its manifest and MCP file) in `writeHostFiles({ pkg, manifest, mcpServer })`, where `mcpServer` is
 * the `octocode-mcp` server entry pinned to the current version.
 */
/**
 * @param {object} options
 * @param {string} options.packageRoot
 * @param {string} options.label What the final line calls the plugin.
 * @param {(files: HostFiles) => void} options.writeHostFiles
 */
export function buildPlugin({ packageRoot, label, writeHostFiles }) {
  const repoRoot = resolve(packageRoot, '../..');
  const pkg = readJson(join(packageRoot, 'package.json'));
  const mcp = readJson(join(repoRoot, 'packages/octocode-mcp/package.json'));
  const manifest = readJson(join(packageRoot, 'src/plugin.json'));
  const skills = join(packageRoot, 'skills');
  stageSkills(join(repoRoot, 'skills'), skills);
  // A collection README is not an installed skill and contains repository-only links.
  for (const entry of readdirSync(skills, { withFileTypes: true })) {
    if (
      !entry.isDirectory() ||
      !existsSync(join(skills, entry.name, 'SKILL.md'))
    ) {
      rmSync(join(skills, entry.name), { recursive: true, force: true });
    }
  }
  writeHostFiles({
    pkg,
    manifest,
    mcpServer: {
      type: 'stdio',
      command: 'npx',
      args: ['-y', `octocode-mcp@${mcp.version}`],
    },
  });
  cpSync(join(repoRoot, 'LICENSE'), join(packageRoot, 'LICENSE'));
  const count = readdirSync(skills).filter(name =>
    existsSync(join(skills, name, 'SKILL.md'))
  ).length;
  console.log(
    `Built ${label} ${pkg.version}: ${count} skills; MCP ${mcp.version}.`
  );
}

// argv[1] keeps a symlinked path while import.meta.url is the real path.
const realOrSelf = (/** @type {string} */ p) => {
  try {
    return realpathSync(p);
  } catch {
    return resolve(p);
  }
};

/** Whether the module at `moduleUrl` is the script node was started with. */
export const isMainModule = (/** @type {string} */ moduleUrl) =>
  Boolean(process.argv[1]) &&
  realOrSelf(process.argv[1]) === realOrSelf(fileURLToPath(moduleUrl));
