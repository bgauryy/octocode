import { closeSync, existsSync, mkdirSync, openSync, readFileSync, renameSync, statSync, unlinkSync, writeFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';

export type HookHost = 'claude' | 'codex' | 'cursor';

export interface HookSpec {
  event: string;
  matcher?: string;
  command: string;
  commandWindows?: string;
  targetPath: string;
}

export interface NestedHook {
  type?: string;
  command?: string;
  commandWindows?: string;
  timeout?: number;
}

export interface HookEntry {
  command?: string;
  timeout?: number;
  matcher?: string;
  hooks?: NestedHook[];
  [key: string]: unknown;
}

export interface HookSettings {
  version?: number;
  hooks?: Record<string, HookEntry[]>;
  [key: string]: unknown;
}

export interface HooksInstallResult {
  exitCode: number;
  payload?: Record<string, unknown>;
  text?: string;
}

export interface HooksInstallOptions {
  cwd?: string;
  homeDir?: string;
  hookDir: string;
  dbPath?: string;
}

export const WRITE_MATCHERS: Record<HookHost, string> = {
  claude: '^(?:Write|Edit|MultiEdit|NotebookEdit)$',
  codex: '^(?:apply_patch|Write|Edit)$',
  cursor: '^(?:Write|Edit|StrReplace|Delete|MultiEdit|NotebookEdit|apply_patch|ApplyPatch)$',
};
export const HOSTS = new Set<HookHost>(['claude', 'codex', 'cursor']);
export const CONFIG_LOCK_WAIT = new Int32Array(new SharedArrayBuffer(4));
export const CONFIG_LOCK_RETRY_MS = 25;
export const CONFIG_LOCK_TIMEOUT_MS = 10_000;
export const CONFIG_LOCK_STALE_MS = 30_000;

export function hooksInstallUsage(): string {
  return `usage: octocode-awareness hooks install|check|remove [options]

Install, check, dry-run, or remove octocode-awareness lifecycle hooks.

Targets:
  --host claude         Write Claude Code hooks to .claude/settings.json (install default).
  --host codex         Write Codex hooks to .codex/hooks.json.
  --host cursor        Write Cursor hooks to .cursor/hooks.json.
  Pi                   No shell install target; use wirePiAwarenessHooks(pi).

Options:
  --project-dir <path>  Target a project hook file under <path> (default: cwd).
  --global              Target the user hook file with absolute hook paths.
  --check               Report whether the hooks are installed.
  --strict              With --check, exit 2 if config is missing or drifted.
                        Runtime execution, host trust, and enablement remain unprobed.
  --dry-run             Print the resulting settings without writing.
  --compact             Minify JSON output when supported.
  --remove              Remove only octocode-awareness hooks.`;
}

export function flag(argv: string[], value: string): boolean {
  return argv.includes(value);
}

export function opt(argv: string[], name: string, fallback: string): string {
  const index = argv.indexOf(name);
  return index >= 0 && argv[index + 1] ? argv[index + 1]! : fallback;
}

export function fail(message: string, extra: Record<string, unknown> = {}): HooksInstallResult {
  return { exitCode: 1, payload: { ok: false, error: message, ...extra } };
}

export function requestedHost(argv: string[]): string {
  return opt(argv, '--host', 'claude').toLowerCase();
}

export function targetConfig(host: HookHost): { dir: string; file: string } {
  switch (host) {
    case 'codex': return { dir: '.codex', file: 'hooks.json' };
    case 'cursor': return { dir: '.cursor', file: 'hooks.json' };
    case 'claude': return { dir: '.claude', file: 'settings.json' };
  }
}

export function loadSettings(settingsPath: string): HookSettings {
  if (!existsSync(settingsPath)) return {};
  const raw = readFileSync(settingsPath, 'utf8');
  const parsed = JSON.parse(raw) as unknown;
  return parsed && typeof parsed === 'object' ? parsed as HookSettings : {};
}

export function processIsAlive(pid: number): boolean {
  if (!Number.isSafeInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code !== 'ESRCH';
  }
}

export function removeStaleConfigLock(lockPath: string): boolean {
  try {
    const owner = Number.parseInt(readFileSync(lockPath, 'utf8'), 10);
    const staleByAge = Date.now() - statSync(lockPath).mtimeMs > CONFIG_LOCK_STALE_MS;
    if (processIsAlive(owner) && !staleByAge) return false;
    unlinkSync(lockPath);
    return true;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return true;
    return false;
  }
}

export function acquireConfigLock(settingsPath: string): () => void {
  mkdirSync(dirname(settingsPath), { recursive: true });
  const lockPath = `${settingsPath}.octocode-awareness.lock`;
  const deadline = Date.now() + CONFIG_LOCK_TIMEOUT_MS;
  for (;;) {
    try {
      const fd = openSync(lockPath, 'wx', 0o600);
      try {
        writeFileSync(fd, `${process.pid}\n`, 'utf8');
      } finally {
        closeSync(fd);
      }
      return () => {
        try { unlinkSync(lockPath); } catch (error) {
          if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
        }
      };
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'EEXIST') throw error;
      if (removeStaleConfigLock(lockPath)) continue;
      if (Date.now() >= deadline) {
        throw new Error(`timed out waiting for concurrent hook update: ${settingsPath}`);
      }
      Atomics.wait(CONFIG_LOCK_WAIT, 0, 0, CONFIG_LOCK_RETRY_MS);
    }
  }
}

export function writeSettingsAtomic(settingsPath: string, settings: HookSettings): void {
  mkdirSync(dirname(settingsPath), { recursive: true });
  const temporaryPath = `${settingsPath}.tmp-${process.pid}-${randomUUID()}`;
  try {
    writeFileSync(temporaryPath, JSON.stringify(settings, null, 2) + '\n');
    renameSync(temporaryPath, settingsPath);
  } finally {
    try { unlinkSync(temporaryPath); } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    }
  }
}

export function hookCommand(name: string, params: {
  host: HookHost;
  globalMode: boolean;
  projectDir: string;
  hookDir: string;
}): string {
  const runner = resolve(params.hookDir, '..', 'hook-runner.mjs');
  const skillRoot = resolve(params.hookDir, '..', '..');
  const command = name.replace(/\.sh$/, '');
  const projectPath = (absolutePath: string): { value: string; expandsProjectDir: boolean } => {
    if (params.host !== 'claude' || params.globalMode) {
      return { value: absolutePath, expandsProjectDir: false };
    }
    const rel = relative(params.projectDir, absolutePath);
    if (rel && !rel.startsWith('..') && !isAbsolute(rel)) {
      return {
        value: '${CLAUDE_PROJECT_DIR}/' + rel.split(sep).join('/'),
        expandsProjectDir: true,
      };
    }
    return { value: absolutePath, expandsProjectDir: false };
  };
  const quoted = (value: string, expandsVariables = false) => {
    const pattern = expandsVariables ? /["\\`]/g : /["\\$`]/g;
    return `"${value.replace(pattern, '\\$&')}"`;
  };
  const runnerPath = projectPath(runner);
  const skillRootPath = projectPath(skillRoot);
  return [
    quoted(process.execPath),
    quoted(runnerPath.value, runnerPath.expandsProjectDir),
    command,
    '--host',
    params.host,
    '--skill-root',
    quoted(skillRootPath.value, skillRootPath.expandsProjectDir),
  ].join(' ');
}

export function projectHookDir(_host: HookHost, globalMode: boolean, projectDir: string, hookDir: string): string {
  if (globalMode) return hookDir;
  const canonical = join(projectDir, 'skills', 'octocode-awareness', 'scripts', 'hooks');
  return existsSync(hookTargetPath(canonical)) ? canonical : hookDir;
}

export function hookTargetPath(hookDir: string): string {
  return resolve(hookDir, '..', 'hook-runner.mjs');
}

export function hookCommandWindows(name: string, params: {
  host: HookHost;
  hookDir: string;
}): string | undefined {
  if (params.host !== 'codex') return undefined;
  const runner = resolve(params.hookDir, '..', 'hook-runner.mjs');
  const skillRoot = resolve(params.hookDir, '..', '..');
  const command = name.replace(/\.sh$/, '');
  const quoted = (value: string) => `"${value.replace(/"/g, '""')}"`;
  return [
    quoted(process.execPath),
    quoted(runner),
    command,
    '--host',
    params.host,
    '--skill-root',
    quoted(skillRoot),
  ].join(' ');
}
