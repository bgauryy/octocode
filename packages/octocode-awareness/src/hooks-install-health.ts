import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { hookCommand, hookCommandWindows, hookTargetPath, HookEntry, HookHost, HookSpec, WRITE_MATCHERS } from './hooks-install-specs.js';

export function specsFor(host: HookHost, params: {
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
    targetPath: hookTargetPath(params.hookDir),
  });
  if (host === 'cursor') {
    return [
      spec('preToolUse', 'pre-edit.sh', WRITE_MATCHERS.cursor),
      spec('postToolUse', 'post-edit.sh', WRITE_MATCHERS.cursor),
      spec('postToolUseFailure', 'post-edit.sh', WRITE_MATCHERS.cursor),
      spec('subagentStart', 'notify-deliver.sh'),
      spec('stop', 'stop-verify.sh'),
      spec('subagentStop', 'stop-verify.sh'),
      spec('sessionEnd', 'session-end.sh'),
      spec('preCompact', 'session-compact.sh'),
      spec('sessionStart', 'notify-deliver.sh'),
    ];
  }
  if (host === 'codex') {
    return [
      spec('SessionStart', 'notify-deliver.sh'),
      spec('PreToolUse', 'pre-edit.sh', WRITE_MATCHERS.codex),
      spec('PostToolUse', 'post-edit.sh', WRITE_MATCHERS.codex),
      spec('SubagentStart', 'notify-deliver.sh'),
      spec('Stop', 'stop-verify.sh'),
      spec('SubagentStop', 'stop-verify.sh'),
      spec('PreCompact', 'session-compact.sh'),
      spec('UserPromptSubmit', 'notify-deliver.sh'),
    ];
  }
  return [
    spec('PreToolUse', 'pre-edit.sh', WRITE_MATCHERS.claude),
    spec('PostToolUse', 'post-edit.sh', WRITE_MATCHERS.claude),
    spec('PostToolUseFailure', 'post-edit.sh', WRITE_MATCHERS.claude),
    spec('SubagentStart', 'notify-deliver.sh'),
    spec('Stop', 'stop-verify.sh'),
    spec('SubagentStop', 'stop-verify.sh'),
    spec('PreCompact', 'session-compact.sh'),
    spec('SessionEnd', 'session-end.sh'),
    spec('UserPromptSubmit', 'notify-deliver.sh'),
  ];
}

export function obsoleteSpecsFor(host: HookHost, params: {
  globalMode: boolean;
  projectDir: string;
  hookDir: string;
}): HookSpec[] {
  return [{
    event: host === 'cursor' ? 'preToolUse' : 'PreToolUse',
    matcher: WRITE_MATCHERS[host],
    command: hookCommand('harness-guard.sh', { host, ...params }),
    targetPath: hookTargetPath(params.hookDir),
  }];
}

export function entry(host: HookHost, spec: HookSpec): HookEntry {
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

export function awarenessHookName(command: string | undefined): string | null {
  const normalized = command?.replace(/\\/g, '/');
  if (!normalized) return null;
  const wrapper = /\/octocode-awareness\/scripts\/hooks\/(pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-compact|session-end)\.sh/.exec(normalized);
  if (wrapper?.[1]) return `${wrapper[1]}.sh`;
  const runner = /\/octocode-awareness\/scripts\/hook-runner\.mjs["']?\s+(pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-compact|session-end)(?:\s|$)/.exec(normalized);
  return runner?.[1] ? `${runner[1]}.sh` : null;
}

export function sameAwarenessCommand(actual: string | undefined, expected: string): boolean {
  if (actual === expected) return true;
  const actualHook = awarenessHookName(actual);
  const expectedHook = awarenessHookName(expected);
  return actualHook !== null && expectedHook !== null && actualHook === expectedHook;
}

export function hasCommand(groups: HookEntry[] | undefined, command: string): boolean {
  return (groups ?? []).some((group) => (
    sameAwarenessCommand(group.command, command)
    || (group.hooks ?? []).some((hook) => sameAwarenessCommand(hook.command, command))
  ));
}

export function matcherMatches(actual: unknown, expected: string | undefined): boolean {
  return expected ? actual === expected : actual == null;
}

export function isExactHookEntry(host: HookHost, group: HookEntry, spec: HookSpec): boolean {
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

export function hasExactCommand(groups: HookEntry[] | undefined, host: HookHost, spec: HookSpec): boolean {
  return (groups ?? []).some((group) => isExactHookEntry(host, group, spec));
}

export function matchingCommandCount(groups: HookEntry[] | undefined, command: string): number {
  let count = 0;
  for (const group of groups ?? []) {
    if (sameAwarenessCommand(group.command, command)) count += 1;
    count += (group.hooks ?? []).filter((hook) => sameAwarenessCommand(hook.command, command)).length;
  }
  return count;
}

export function hasDriftedCommand(groups: HookEntry[] | undefined, host: HookHost, spec: HookSpec): boolean {
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

export function hookStatusKey(spec: HookSpec): string {
  return `${spec.event}:${awarenessHookName(spec.command) ?? spec.command.split(/[\\/]/).pop()}`;
}

export interface FrontmatterHookDefinition {
  exists: boolean;
  complete: boolean;
  path: string | null;
}

export function frontmatterHookDefinition(projectDir: string, specs: HookSpec[]): FrontmatterHookDefinition {
  const candidates = [
    join(projectDir, '.claude', 'skills', 'octocode-awareness', 'SKILL.md'),
    join(projectDir, '.agents', 'skills', 'octocode-awareness', 'SKILL.md'),
    join(projectDir, 'skills', 'octocode-awareness', 'SKILL.md'),
  ];
  for (const path of candidates) {
    if (!existsSync(path)) continue;
    let text = '';
    try { text = readFileSync(path, 'utf8'); } catch { return { exists: true, complete: false, path }; }
    if (!text.startsWith('---')) return { exists: true, complete: false, path };
    const end = text.indexOf('\n---', 3);
    if (end < 0) return { exists: true, complete: false, path };
    const frontmatter = text.slice(3, end);
    const complete = /(?:^|\n)name:\s*["']?octocode-awareness["']?\s*(?:\n|$)/.test(frontmatter)
      && /(?:^|\n)hooks:\s*(?:\n|$)/.test(frontmatter)
      && specs.every((spec) => {
        const hook = awarenessHookName(spec.command)?.replace(/\.sh$/, '') ?? '';
        return frontmatter.includes(`${spec.event}:`) && Boolean(hook) && frontmatter.includes(hook);
      });
    return { exists: true, complete, path };
  }
  return { exists: false, complete: false, path: null };
}

export function removeCommand(groups: HookEntry[] | undefined, command: string): { groups: HookEntry[]; removed: boolean } {
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

export function removeUnexpectedAwarenessCommands(
  groups: HookEntry[] | undefined,
  keepNames: ReadonlySet<string> = new Set(),
): { groups: HookEntry[]; removed: boolean } {
  let removed = false;
  const out: HookEntry[] = [];
  for (const group of groups ?? []) {
    const flatName = awarenessHookName(group.command);
    if (flatName && !keepNames.has(flatName)) {
      removed = true;
      continue;
    }
    if (!Array.isArray(group.hooks)) {
      out.push(group);
      continue;
    }
    const hooks = group.hooks.filter((hook) => {
      const name = awarenessHookName(hook.command);
      if (name && !keepNames.has(name)) {
        removed = true;
        return false;
      }
      return true;
    });
    if (hooks.length > 0) out.push({ ...group, hooks });
  }
  return { groups: out, removed };
}

export function runtimeHealth(host: HookHost, globalMode: boolean): Record<string, unknown> {
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

export function hookTargetExists(spec: HookSpec): boolean {
  return existsSync(spec.targetPath);
}
