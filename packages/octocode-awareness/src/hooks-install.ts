import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';

type HookHost = 'claude' | 'codex' | 'cursor';

interface HookSpec {
  event: string;
  matcher?: string;
  command: string;
}

interface NestedHook {
  type?: string;
  command?: string;
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

export function hooksInstallUsage(): string {
  return `usage: octocode-awareness hooks install|check|remove [options]

Install, check, dry-run, or remove octocode-awareness lifecycle hooks.

Targets:
  --host claude         Write Claude Code hooks to .claude/settings.json (default).
  --host codex         Write Codex hooks to .codex/hooks.json.
  --host cursor        Write Cursor hooks to .cursor/hooks.json.

Options:
  --project-dir <path>  Target a project hook file under <path> (default: cwd).
  --global              Target the user hook file with absolute hook paths.
  --check               Report whether the hooks are installed.
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

function hookCommand(name: string, params: {
  host: HookHost;
  globalMode: boolean;
  projectDir: string;
  hookDir: string;
}): string {
  const abs = join(params.hookDir, name);
  if (params.host === 'codex' || params.host === 'cursor' || params.globalMode) return abs;
  const rel = relative(params.projectDir, abs);
  if (rel && !rel.startsWith('..') && !isAbsolute(rel)) {
    return '${CLAUDE_PROJECT_DIR}/' + rel.split(sep).join('/');
  }
  return abs;
}

function specsFor(host: HookHost, params: {
  globalMode: boolean;
  projectDir: string;
  hookDir: string;
}): HookSpec[] {
  const command = (name: string) => hookCommand(name, { host, ...params });
  if (host === 'cursor') {
    return [
      { event: 'preToolUse', matcher: WRITE_MATCHER, command: command('pre-edit.sh') },
      { event: 'preToolUse', matcher: WRITE_MATCHER, command: command('harness-guard.sh') },
      { event: 'postToolUse', matcher: WRITE_MATCHER, command: command('post-edit.sh') },
      { event: 'stop', command: command('stop-verify.sh') },
      { event: 'subagentStop', command: command('stop-verify.sh') },
      { event: 'sessionEnd', command: command('session-end.sh') },
      { event: 'preCompact', command: command('session-end.sh') },
      { event: 'sessionStart', command: command('notify-deliver.sh') },
    ];
  }
  if (host === 'codex') {
    return [
      { event: 'PreToolUse', matcher: WRITE_MATCHER, command: command('pre-edit.sh') },
      { event: 'PreToolUse', matcher: WRITE_MATCHER, command: command('harness-guard.sh') },
      { event: 'PostToolUse', matcher: WRITE_MATCHER, command: command('post-edit.sh') },
      { event: 'Stop', command: command('stop-verify.sh') },
      { event: 'SubagentStop', command: command('stop-verify.sh') },
      { event: 'PreCompact', command: command('session-end.sh') },
      { event: 'UserPromptSubmit', command: command('notify-deliver.sh') },
    ];
  }
  return [
    { event: 'PreToolUse', matcher: WRITE_MATCHER, command: command('pre-edit.sh') },
    { event: 'PreToolUse', matcher: WRITE_MATCHER, command: command('harness-guard.sh') },
    { event: 'PostToolUse', matcher: WRITE_MATCHER, command: command('post-edit.sh') },
    { event: 'Stop', command: command('stop-verify.sh') },
    { event: 'SubagentStop', command: command('stop-verify.sh') },
    { event: 'SessionEnd', command: command('session-end.sh') },
    { event: 'UserPromptSubmit', command: command('notify-deliver.sh') },
  ];
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
    hooks: [{ type: 'command', command: spec.command, timeout: 20 }],
  };
}

function awarenessHookName(command: string | undefined): string | null {
  const normalized = command?.replace(/\\/g, '/');
  if (!normalized) return null;
  const marker = '/octocode-awareness/scripts/hooks/';
  const index = normalized.lastIndexOf(marker);
  return index >= 0 ? normalized.slice(index + marker.length) : null;
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

export function runHooksInstall(argv: string[], options: HooksInstallOptions): HooksInstallResult {
  if (flag(argv, '--help') || flag(argv, '-h')) {
    return { exitCode: 0, text: hooksInstallUsage() + '\n' };
  }
  if (flag(argv, '--global') && argv.includes('--project-dir')) {
    return fail('use either --global or --project-dir, not both');
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

  const status = {
    host,
    settingsPath,
    hooks: Object.fromEntries(
      specs.map((spec) => [
        `${spec.event}:${spec.command.split(/[\\/]/).pop()}`,
        hasCommand(settings.hooks?.[spec.event], spec.command),
      ]),
    ),
  };

  if (flag(argv, '--check')) {
    return { exitCode: 0, payload: { ok: true, action: 'check', installed: status } };
  }

  let changed = false;
  settings.hooks ??= {};
  if (host === 'cursor' && !flag(argv, '--remove') && settings.version == null) {
    settings.version = 1;
    changed = true;
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
    for (const spec of specs) {
      const groups = settings.hooks[spec.event] ?? [];
      settings.hooks[spec.event] = groups;
      if (!hasCommand(groups, spec.command)) {
        groups.push(entry(host, spec));
        changed = true;
      }
    }
  }

  if (flag(argv, '--dry-run')) {
    return {
      exitCode: 0,
      payload: { ok: true, action: 'dry-run', host, changed, settingsPath, resultingSettings: settings },
    };
  }

  if (changed) {
    mkdirSync(dirname(settingsPath), { recursive: true });
    writeFileSync(settingsPath, JSON.stringify(settings, null, 2) + '\n');
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
    },
  };
}
