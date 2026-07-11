import {
  closeSync, existsSync, mkdirSync, openSync, readFileSync, renameSync,
  statSync, unlinkSync, writeFileSync,
} from 'node:fs';
import { randomUUID } from 'node:crypto';
import { homedir } from 'node:os';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';

type HookHost = 'claude' | 'codex' | 'cursor';

interface HookSpec {
  event: string;
  matcher?: string;
  command: string;
  commandWindows?: string;
}

interface NestedHook {
  type?: string;
  command?: string;
  commandWindows?: string;
  timeout?: number;
}

interface HookEntry {
  command?: string;
  timeout?: number;
  matcher?: string;
  hooks?: NestedHook[];
  [key: string]: unknown;
}

interface HookSettings {
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
}

const WRITE_MATCHER = 'Write|Edit|MultiEdit|NotebookEdit|apply_patch|ApplyPatch';
const HOSTS = new Set<HookHost>(['claude', 'codex', 'cursor']);
const CONFIG_LOCK_WAIT = new Int32Array(new SharedArrayBuffer(4));
const CONFIG_LOCK_RETRY_MS = 25;
const CONFIG_LOCK_TIMEOUT_MS = 10_000;
const CONFIG_LOCK_STALE_MS = 30_000;

export function hooksInstallUsage(): string {
  return `usage: octocode-awareness hooks install|check|remove [options]

Install, check, dry-run, or remove octocode-awareness lifecycle hooks.

Targets:
  --host claude         Write Claude Code hooks to .claude/settings.json (install default).
  --host codex         Write Codex hooks to .codex/hooks.json.
  --host cursor        Write Cursor hooks to .cursor/hooks.json.
  Pi                   No shell install target; use wirePiAwarenessHooks(pi) or @octocodeai/pi-extension.

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

function flag(argv: string[], value: string): boolean {
  return argv.includes(value);
}

function opt(argv: string[], name: string, fallback: string): string {
  const index = argv.indexOf(name);
  return index >= 0 && argv[index + 1] ? argv[index + 1]! : fallback;
}

function fail(message: string, extra: Record<string, unknown> = {}): HooksInstallResult {
  return { exitCode: 1, payload: { ok: false, error: message, ...extra } };
}

function requestedHost(argv: string[]): string {
  return opt(argv, '--host', 'claude').toLowerCase();
}

function targetConfig(host: HookHost): { dir: string; file: string } {
  switch (host) {
    case 'codex': return { dir: '.codex', file: 'hooks.json' };
    case 'cursor': return { dir: '.cursor', file: 'hooks.json' };
    case 'claude': return { dir: '.claude', file: 'settings.json' };
  }
}

function loadSettings(settingsPath: string): HookSettings {
  if (!existsSync(settingsPath)) return {};
  const raw = readFileSync(settingsPath, 'utf8');
  const parsed = JSON.parse(raw) as unknown;
  return parsed && typeof parsed === 'object' ? parsed as HookSettings : {};
}

function processIsAlive(pid: number): boolean {
  if (!Number.isSafeInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code !== 'ESRCH';
  }
}

function removeStaleConfigLock(lockPath: string): boolean {
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

function acquireConfigLock(settingsPath: string): () => void {
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

function writeSettingsAtomic(settingsPath: string, settings: HookSettings): void {
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

function hookCommand(name: string, params: {
  host: HookHost;
  globalMode: boolean;
  projectDir: string;
  hookDir: string;
}): string {
  const abs = join(params.hookDir, name);
  let scriptPath = abs;
  if (params.host !== 'codex' && params.host !== 'cursor' && !params.globalMode) {
    const rel = relative(params.projectDir, abs);
    if (rel && !rel.startsWith('..') && !isAbsolute(rel)) {
      scriptPath = '${CLAUDE_PROJECT_DIR}/' + rel.split(sep).join('/');
    }
  }
  const quoted = (value: string) => `"${value.replace(/["\\$`]/g, '\\$&')}"`;
  return `OCTOCODE_AGENT_HOST=${params.host} OCTOCODE_NODE_BIN=${quoted(process.execPath)} ${quoted(scriptPath)}`;
}

function hookCommandWindows(name: string, params: {
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

function specsFor(host: HookHost, params: {
  globalMode: boolean;
  projectDir: string;
  hookDir: string;
}): HookSpec[] {
  const spec = (event: string, name: string, matcher?: string): HookSpec => ({
    event,
    ...(matcher ? { matcher } : {}),
    command: hookCommand(name, { host, ...params }),
    ...(hookCommandWindows(name, { host, hookDir: params.hookDir })
      ? { commandWindows: hookCommandWindows(name, { host, hookDir: params.hookDir }) }
      : {}),
  });
  if (host === 'cursor') {
    return [
      spec('preToolUse', 'pre-edit.sh', WRITE_MATCHER),
      spec('postToolUse', 'post-edit.sh', WRITE_MATCHER),
      spec('stop', 'stop-verify.sh'),
      spec('subagentStop', 'stop-verify.sh'),
      spec('sessionEnd', 'session-end.sh'),
      spec('preCompact', 'session-compact.sh'),
      spec('sessionStart', 'notify-deliver.sh'),
    ];
  }
  if (host === 'codex') {
    return [
      spec('PreToolUse', 'pre-edit.sh', WRITE_MATCHER),
      spec('PostToolUse', 'post-edit.sh', WRITE_MATCHER),
      spec('Stop', 'stop-verify.sh'),
      spec('SubagentStop', 'stop-verify.sh'),
      spec('PreCompact', 'session-compact.sh'),
      spec('UserPromptSubmit', 'notify-deliver.sh'),
    ];
  }
  return [
    spec('PreToolUse', 'pre-edit.sh', WRITE_MATCHER),
    spec('PostToolUse', 'post-edit.sh', WRITE_MATCHER),
    spec('Stop', 'stop-verify.sh'),
    spec('SubagentStop', 'stop-verify.sh'),
    spec('SessionEnd', 'session-end.sh'),
    spec('UserPromptSubmit', 'notify-deliver.sh'),
  ];
}

function obsoleteSpecsFor(host: HookHost, params: {
  globalMode: boolean;
  projectDir: string;
  hookDir: string;
}): HookSpec[] {
  return [{
    event: host === 'cursor' ? 'preToolUse' : 'PreToolUse',
    matcher: WRITE_MATCHER,
    command: hookCommand('harness-guard.sh', { host, ...params }),
  }];
}

function entry(host: HookHost, spec: HookSpec): HookEntry {
  if (host === 'cursor') {
    return {
      command: spec.command,
      timeout: 20,
      ...(spec.matcher ? { matcher: spec.matcher } : {}),
    };
  }
  return {
    ...(spec.matcher ? { matcher: spec.matcher } : {}),
    hooks: [{
      type: 'command',
      command: spec.command,
      ...(spec.commandWindows ? { commandWindows: spec.commandWindows } : {}),
      timeout: 20,
    }],
  };
}

function awarenessHookName(command: string | undefined): string | null {
  const normalized = command?.replace(/\\/g, '/');
  if (!normalized) return null;
  const wrapper = /\/octocode-awareness\/scripts\/hooks\/(pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-compact|session-end)\.sh/.exec(normalized);
  if (wrapper?.[1]) return `${wrapper[1]}.sh`;
  const runner = /\/octocode-awareness\/scripts\/hook-runner\.mjs["']?\s+(pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-compact|session-end)(?:\s|$)/.exec(normalized);
  return runner?.[1] ? `${runner[1]}.sh` : null;
}

function sameAwarenessCommand(actual: string | undefined, expected: string): boolean {
  if (actual === expected) return true;
  const actualHook = awarenessHookName(actual);
  const expectedHook = awarenessHookName(expected);
  return actualHook !== null && expectedHook !== null && actualHook === expectedHook;
}

function hasCommand(groups: HookEntry[] | undefined, command: string): boolean {
  return (groups ?? []).some((group) => (
    sameAwarenessCommand(group.command, command)
    || (group.hooks ?? []).some((hook) => sameAwarenessCommand(hook.command, command))
  ));
}

function matcherMatches(actual: unknown, expected: string | undefined): boolean {
  return expected ? actual === expected : actual == null;
}

function isExactHookEntry(host: HookHost, group: HookEntry, spec: HookSpec): boolean {
  if (host === 'cursor') {
    return group.command === spec.command
      && group.timeout === 20
      && matcherMatches(group.matcher, spec.matcher)
      && !Array.isArray(group.hooks);
  }

  return matcherMatches(group.matcher, spec.matcher)
    && (group.hooks ?? []).some((hook) => (
      hook.type === 'command'
      && hook.command === spec.command
      && hook.commandWindows === spec.commandWindows
      && hook.timeout === 20
    ));
}

function hasExactCommand(groups: HookEntry[] | undefined, host: HookHost, spec: HookSpec): boolean {
  return (groups ?? []).some((group) => isExactHookEntry(host, group, spec));
}

function matchingCommandCount(groups: HookEntry[] | undefined, command: string): number {
  let count = 0;
  for (const group of groups ?? []) {
    if (sameAwarenessCommand(group.command, command)) count += 1;
    count += (group.hooks ?? []).filter((hook) => sameAwarenessCommand(hook.command, command)).length;
  }
  return count;
}

function hasDriftedCommand(groups: HookEntry[] | undefined, host: HookHost, spec: HookSpec): boolean {
  for (const group of groups ?? []) {
    if (host === 'cursor') {
      if (sameAwarenessCommand(group.command, spec.command) && !isExactHookEntry(host, group, spec)) {
        return true;
      }
      continue;
    }

    for (const hook of group.hooks ?? []) {
      if (!sameAwarenessCommand(hook.command, spec.command)) continue;
      const exact = matcherMatches(group.matcher, spec.matcher)
        && hook.type === 'command'
        && hook.commandWindows === spec.commandWindows
        && hook.timeout === 20;
      if (!exact) return true;
    }
  }
  return false;
}

function hookStatusKey(spec: HookSpec): string {
  return `${spec.event}:${awarenessHookName(spec.command) ?? spec.command.split(/[\\/]/).pop()}`;
}

function removeCommand(groups: HookEntry[] | undefined, command: string): { groups: HookEntry[]; removed: boolean } {
  let removed = false;
  const out: HookEntry[] = [];
  for (const group of groups ?? []) {
    if (sameAwarenessCommand(group.command, command)) {
      removed = true;
      continue;
    }
    if (!Array.isArray(group.hooks)) {
      out.push(group);
      continue;
    }
    const hooks = group.hooks.filter((hook) => {
      if (sameAwarenessCommand(hook.command, command)) {
        removed = true;
        return false;
      }
      return true;
    });
    if (hooks.length > 0) out.push({ ...group, hooks });
  }
  return { groups: out, removed };
}

function runtimeHealth(host: HookHost, globalMode: boolean): Record<string, unknown> {
  const common = {
    status: 'unverified',
    verified: false,
    execution: 'not_probed',
    strict_scope: 'config_only',
    next: 'Run a harmless write and inspect the host hook log before relying on enforcement.',
  };
  if (host === 'codex') {
    return {
      ...common,
      project_trust: globalMode ? 'not_applicable_global_config' : 'not_checked',
      hook_definition_trust: 'not_checked',
      hooks_feature_enabled: 'not_checked',
    };
  }
  if (host === 'claude') {
    return {
      ...common,
      activation: globalMode ? 'global_config_not_probed' : 'skill_or_project_activation_not_checked',
    };
  }
  return {
    ...common,
    local_runtime: 'not_probed',
    cloud_runtime: 'not_probed',
    windows_command: 'not_guaranteed_by_cursor_flat_hook_format',
  };
}

export function runHooksInstall(argv: string[], options: HooksInstallOptions): HooksInstallResult {
  const hostValue = requestedHost(argv);
  const writes = !flag(argv, '--help')
    && !flag(argv, '-h')
    && !flag(argv, '--check')
    && !flag(argv, '--dry-run')
    && !(flag(argv, '--global') && argv.includes('--project-dir'))
    && HOSTS.has(hostValue as HookHost);
  if (!writes) return runHooksInstallUnlocked(argv, options);

  const host = hostValue as HookHost;
  const cwd = options.cwd ?? process.cwd();
  const home = options.homeDir ?? homedir();
  const config = targetConfig(host);
  const settingsPath = flag(argv, '--global')
    ? join(home, config.dir, config.file)
    : join(resolve(opt(argv, '--project-dir', cwd)), config.dir, config.file);

  let release: (() => void) | undefined;
  try {
    release = acquireConfigLock(settingsPath);
    return runHooksInstallUnlocked(argv, options);
  } catch (error) {
    return fail(`cannot update ${settingsPath}: ${(error as Error).message}`);
  } finally {
    release?.();
  }
}

function runHooksInstallUnlocked(argv: string[], options: HooksInstallOptions): HooksInstallResult {
  if (flag(argv, '--help') || flag(argv, '-h')) {
    return { exitCode: 0, text: hooksInstallUsage() + '\n' };
  }
  if (flag(argv, '--global') && argv.includes('--project-dir')) {
    return fail('use either --global or --project-dir, not both');
  }
  if (flag(argv, '--check') && !argv.includes('--host')) {
    return fail('hooks check requires --host claude, --host codex, or --host cursor');
  }

  const hostValue = requestedHost(argv);
  if (!HOSTS.has(hostValue as HookHost)) {
    return fail('invalid --host; expected claude, codex, or cursor', { host: hostValue });
  }

  const host = hostValue as HookHost;
  const cwd = options.cwd ?? process.cwd();
  const home = options.homeDir ?? homedir();
  const globalMode = flag(argv, '--global');
  const projectDir = resolve(opt(argv, '--project-dir', cwd));
  const config = targetConfig(host);
  const settingsPath = globalMode
    ? join(home, config.dir, config.file)
    : join(projectDir, config.dir, config.file);

  let settings: HookSettings;
  try {
    settings = loadSettings(settingsPath);
  } catch (error) {
    return fail(`cannot parse ${settingsPath}: ${(error as Error).message}`);
  }

  const specs = specsFor(host, {
    globalMode,
    projectDir,
    hookDir: options.hookDir,
  });
  const obsoleteSpecs = obsoleteSpecsFor(host, {
    globalMode,
    projectDir,
    hookDir: options.hookDir,
  });

  const checks = specs.map((spec) => {
    const groups = settings.hooks?.[spec.event];
    const present = hasCommand(groups, spec.command);
    const exact = hasExactCommand(groups, host, spec);
    const matchingCount = matchingCommandCount(groups, spec.command);
    const drifted = present && (!exact || hasDriftedCommand(groups, host, spec) || matchingCount > 1);
    return {
      key: hookStatusKey(spec),
      event: spec.event,
      hook: awarenessHookName(spec.command) ?? spec.command.split(/[\\/]/).pop(),
      installed: exact,
      present,
      matching_count: matchingCount,
      drifted,
      expected: {
        matcher: spec.matcher ?? null,
        command: spec.command,
        command_windows: spec.commandWindows ?? null,
        timeout: 20,
        shape: host === 'cursor' ? 'flat' : 'nested',
      },
    };
  });
  const hooks = Object.fromEntries(checks.map((check) => [check.key, check.installed]));
  const obsolete = obsoleteSpecs
    .filter((spec) => hasCommand(settings.hooks?.[spec.event], spec.command))
    .map(hookStatusKey);
  const status = {
    host,
    settingsPath,
    hooks,
    installed_all: checks.every((check) => check.installed) && obsolete.length === 0,
    missing: checks.filter((check) => !check.present).map((check) => check.key),
    drifted: [...checks.filter((check) => check.drifted).map((check) => check.key), ...obsolete],
    details: Object.fromEntries(checks.map((check) => [check.key, check])),
  };

  if (flag(argv, '--check')) {
    const strict = flag(argv, '--strict');
    const configReady = status.installed_all && status.drifted.length === 0;
    return {
      exitCode: strict && !configReady ? 2 : 0,
      payload: {
        ok: configReady,
        action: 'check',
        strict,
        strict_scope: 'config_only',
        installed: status,
        health: {
          config: {
            status: configReady ? 'ready' : 'needs_repair',
            verified: configReady,
            settings_path: settingsPath,
          },
          runtime: runtimeHealth(host, globalMode),
        },
      },
    };
  }

  let changed = false;
  settings.hooks ??= {};
  if (host === 'cursor' && !flag(argv, '--remove') && settings.version == null) {
    settings.version = 1;
    changed = true;
  }

  for (const spec of obsoleteSpecs) {
    const result = removeCommand(settings.hooks[spec.event], spec.command);
    if (!result.removed) continue;
    changed = true;
    if (result.groups.length > 0) settings.hooks[spec.event] = result.groups;
    else delete settings.hooks[spec.event];
  }

  if (flag(argv, '--remove')) {
    for (const spec of specs) {
      const result = removeCommand(settings.hooks[spec.event], spec.command);
      if (result.removed) {
        changed = true;
        if (result.groups.length > 0) settings.hooks[spec.event] = result.groups;
        else delete settings.hooks[spec.event];
      }
    }
    if (Object.keys(settings.hooks).length === 0) delete settings.hooks;
  } else {
    const checksByKey = new Map(checks.map((check) => [check.key, check]));
    for (const spec of specs) {
      const groups = settings.hooks[spec.event] ?? [];
      settings.hooks[spec.event] = groups;
      const check = checksByKey.get(hookStatusKey(spec));
      if (!check?.installed || check.drifted) {
        const pruned = removeCommand(groups, spec.command);
        settings.hooks[spec.event] = pruned.groups;
        settings.hooks[spec.event]!.push(entry(host, spec));
        changed = true;
      }
    }
  }

  if (flag(argv, '--dry-run')) {
    return {
      exitCode: 0,
      payload: {
        ok: true,
        action: 'dry-run',
        host,
        changed,
        settingsPath,
        resultingSettings: settings,
        runtime: runtimeHealth(host, globalMode),
      },
    };
  }

  if (changed) {
    writeSettingsAtomic(settingsPath, settings);
  }

  return {
    exitCode: 0,
    payload: {
      ok: true,
      action: flag(argv, '--remove') ? 'remove' : 'install',
      host,
      changed,
      settingsPath,
      note: changed ? `${settingsPath.split(/[\\/]/).pop()} updated` : 'already up to date - no change',
      runtime: runtimeHealth(host, globalMode),
    },
  };
}
