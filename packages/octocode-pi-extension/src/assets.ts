import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import {
  execCli,
  runPreEditLockGate,
  storageScopeForCommand,
  type AwarenessStorageScope,
  type PreEditHookResult,
  type PreEditHookOptions,
} from '@octocodeai/octocode-awareness';

const extensionDir = path.dirname(fileURLToPath(import.meta.url));
const requireFromExtension = createRequire(import.meta.url);

// One root package and one CLI serve both the harness and external agents.
export const AWARENESS_PACKAGE = '@octocodeai/octocode-awareness';

export interface AwarenessCommandSpec {
  cmd: string;
  args: string[];
}

export interface AwarenessRunResult {
  code: number;
  stdout: string;
  stderr: string;
}

export function resolveAwarenessCliPath(): string {
  return requireFromExtension.resolve(`${AWARENESS_PACKAGE}/bin/awareness`);
}

/**
 * Build a spawn spec (`node cli.js …`) for the Awareness bin. Retained for
 * the surfaces the model/user or a foreign host invokes as a real command:
 * launcher verbs (surfaces.ts) and the `$OCTOCODE_AWARENESS_CLI` env var. The
 * extension's OWN calls run in-process via runAwarenessInProcess instead.
 */
export function buildAwarenessCommand(args: string[] = []): AwarenessCommandSpec {
  return { cmd: process.execPath, args: [resolveAwarenessCliPath(), ...args] };
}

/**
 * Run an Awareness command vector IN-PROCESS — no child process — via the
 * library's `execCli`. Returns the same JSON-on-stdout / exit-code contract the
 * `cli.js` bin produced, so callers that already build an argv and parse stdout
 * keep working unchanged (exit 2 still signals a lock-wait/pre-edit block).
 */
export function runAwarenessInProcess(args: string[]): AwarenessRunResult {
  return execCli(args);
}

/** Run the Awareness pre-edit lock gate in-process (library call, no spawn). */
export function runAwarenessPreEdit(options: PreEditHookOptions): PreEditHookResult {
  return runPreEditLockGate(options);
}

export function resolveAwarenessCoordinationScope(workspace: string): AwarenessStorageScope {
  return storageScopeForCommand('coordination', workspace);
}

export interface AssetPaths {
  baseDir: string;
  docsDir: string;
  skillsDir: string;
  systemPrompt: string;
  /** Agent-facing Awareness command display string. */
  awarenessCliPath: string;
}

export function getAssetPaths(baseDir = extensionDir): AssetPaths {
  return {
    baseDir,
    docsDir: path.join(baseDir, 'docs'),
    skillsDir: path.join(baseDir, 'skills'),
    systemPrompt: path.join(baseDir, 'system', 'SYSTEM_PROMPT.md'),
    awarenessCliPath: getAwarenessCLIPath(baseDir),
  };
}

/**
 * Returns the agent-facing Awareness command DISPLAY string ("node
 * /path/cli.js"). Kept under the historical name because launcher/status code
 * imports it. Display-only — the executable-facing `$OCTOCODE_AWARENESS_CLI`
 * env var carries the bare script path (see index.ts). Falls back to the npx
 * form when the package cannot be resolved so status surfaces never crash.
 */
export function getAwarenessCLIPath(_baseDir = extensionDir): string {
  try {
    return `${process.execPath} ${resolveAwarenessCliPath()}`;
  } catch {
    return `npx -p ${AWARENESS_PACKAGE} octocode-awareness`;
  }
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
