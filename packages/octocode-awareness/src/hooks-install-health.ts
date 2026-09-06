import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { hookCommand, hookCommandWindows, hookTargetPath, HookEntry, HookSpec, InstallableHookHost, WRITE_MATCHERS } from './hooks-install-specs.js';
import type { AwarenessHookProfile } from './workspace-policy.js';

export function specsFor(host: InstallableHookHost, params: {
  globalMode: boolean;
  projectDir: string;
  hookDir: string;
  profile?: AwarenessHookProfile;
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
  const filterProfile = (specs: HookSpec[]): HookSpec[] => {
    const profile = params.profile ?? 'full';
    if (profile === 'full') return specs;
    const lifecycleEvents = new Set(host === 'cursor'
      ? ['preToolUse', 'postToolUse', 'postToolUseFailure', 'stop', 'subagentStop']
      : host === 'copilot'
        ? ['preToolUse', 'postToolUse', 'postToolUseFailure', 'agentStop', 'subagentStop']
        : host === 'gemini'
          ? ['BeforeTool', 'AfterTool', 'AfterAgent']
          : ['PreToolUse', 'PostToolUse', 'PostToolUseFailure', 'Stop', 'SubagentStop']);
    return specs.filter((entry) => lifecycleEvents.has(entry.event));
  };
  if (host === 'cursor') {
    return filterProfile([
      spec('preToolUse', 'pre-edit.sh', WRITE_MATCHERS.cursor),
      spec('postToolUse', 'post-edit.sh', WRITE_MATCHERS.cursor),
      spec('postToolUseFailure', 'post-edit.sh', WRITE_MATCHERS.cursor),
      spec('subagentStart', 'notify-deliver.sh'),
      spec('stop', 'stop-verify.sh'),
      spec('subagentStop', 'stop-verify.sh'),
      spec('sessionEnd', 'session-end.sh'),
      spec('preCompact', 'session-compact.sh'),
      spec('sessionStart', 'notify-deliver.sh'),
    ]);
  }
  if (host === 'copilot') {
    return filterProfile([
      spec('sessionStart', 'notify-deliver.sh'),
      spec('preToolUse', 'pre-edit.sh', WRITE_MATCHERS.copilot),
      spec('postToolUse', 'post-edit.sh', WRITE_MATCHERS.copilot),
      spec('postToolUseFailure', 'post-edit.sh', WRITE_MATCHERS.copilot),
      spec('subagentStart', 'notify-deliver.sh'),
      spec('agentStop', 'stop-verify.sh'),
      spec('subagentStop', 'stop-verify.sh'),
      spec('preCompact', 'session-compact.sh'),
      spec('sessionEnd', 'session-end.sh'),
      spec('userPromptSubmitted', 'notify-deliver.sh'),
    ]);
  }
  if (host === 'gemini') {
    return filterProfile([
      spec('SessionStart', 'notify-deliver.sh'),
      spec('BeforeTool', 'pre-edit.sh', WRITE_MATCHERS.gemini),
      spec('AfterTool', 'post-edit.sh', WRITE_MATCHERS.gemini),
      spec('BeforeAgent', 'notify-deliver.sh'),
      spec('AfterAgent', 'stop-verify.sh'),
      spec('PreCompress', 'session-compact.sh'),
      spec('SessionEnd', 'session-end.sh'),
    ]);
  }
  if (host === 'codex') {
    return filterProfile([
      spec('SessionStart', 'notify-deliver.sh'),
      spec('PreToolUse', 'pre-edit.sh', WRITE_MATCHERS.codex),
      spec('PostToolUse', 'post-edit.sh', WRITE_MATCHERS.codex),
      spec('SubagentStart', 'notify-deliver.sh'),
      spec('Stop', 'stop-verify.sh'),
      spec('SubagentStop', 'stop-verify.sh'),
      spec('PreCompact', 'session-compact.sh'),
      spec('SessionEnd', 'session-end.sh'),
      spec('UserPromptSubmit', 'notify-deliver.sh'),
    ]);
  }
  return filterProfile([
    spec('SessionStart', 'notify-deliver.sh'),
    spec('PreToolUse', 'pre-edit.sh', WRITE_MATCHERS.claude),
    spec('PostToolUse', 'post-edit.sh', WRITE_MATCHERS.claude),
    spec('PostToolUseFailure', 'post-edit.sh', WRITE_MATCHERS.claude),
    spec('SubagentStart', 'notify-deliver.sh'),
    spec('Stop', 'stop-verify.sh'),
    spec('SubagentStop', 'stop-verify.sh'),
    spec('PreCompact', 'session-compact.sh'),
    spec('PostCompact', 'session-compact.sh'),
    spec('SessionEnd', 'session-end.sh'),
    spec('UserPromptSubmit', 'notify-deliver.sh'),
    spec('Notification', 'notify-deliver.sh'),
  ]);
}

export function obsoleteSpecsFor(host: InstallableHookHost, params: {
  globalMode: boolean;
  projectDir: string;
  hookDir: string;
}): HookSpec[] {
  return [{
    event: host === 'cursor' || host === 'copilot'
      ? 'preToolUse'
      : host === 'gemini'
        ? 'BeforeTool'
        : 'PreToolUse',
    matcher: WRITE_MATCHERS[host],
    command: hookCommand('harness-guard.sh', { host, ...params }),
    targetPath: hookTargetPath(params.hookDir),
  }];
}

export function entry(host: InstallableHookHost, spec: HookSpec): HookEntry {
  if (host === 'cursor') {
    return {
      command: spec.command,
      timeout: 20,
      ...(spec.matcher ? { matcher: spec.matcher } : {}),
    };
  }
  if (host === 'copilot') {
    return {
      type: 'command',
      command: spec.command,
      timeoutSec: 20,
      ...(spec.matcher ? { matcher: spec.matcher } : {}),
    };
  }
  return {
    ...(spec.matcher ? { matcher: spec.matcher } : {}),
    hooks: [{
      type: 'command',
      command: spec.command,
      ...(spec.commandWindows ? { commandWindows: spec.commandWindows } : {}),
      timeout: host === 'gemini' ? 20_000 : 20,
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

export function isExactHookEntry(host: InstallableHookHost, group: HookEntry, spec: HookSpec): boolean {
  if (host === 'cursor') {
    return group.command === spec.command
      && group.timeout === 20
      && matcherMatches(group.matcher, spec.matcher)
      && !Array.isArray(group.hooks);
  }
  if (host === 'copilot') {
    return group.type === 'command'
      && group.command === spec.command
      && group.timeoutSec === 20
      && matcherMatches(group.matcher, spec.matcher)
      && !Array.isArray(group.hooks);
  }

  return matcherMatches(group.matcher, spec.matcher)
    && (group.hooks ?? []).some((hook) => (
      hook.type === 'command'
      && hook.command === spec.command
      && hook.commandWindows === spec.commandWindows
      && hook.timeout === (host === 'gemini' ? 20_000 : 20)
    ));
}

export function hasExactCommand(groups: HookEntry[] | undefined, host: InstallableHookHost, spec: HookSpec): boolean {
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

export function hasDriftedCommand(groups: HookEntry[] | undefined, host: InstallableHookHost, spec: HookSpec): boolean {
  for (const group of groups ?? []) {
    if (host === 'cursor' || host === 'copilot') {
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
        && hook.timeout === (host === 'gemini' ? 20_000 : 20);
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

export function runtimeHealth(host: InstallableHookHost, globalMode: boolean): Record<string, unknown> {
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
  if (host === 'copilot') {
    return {
      ...common,
      repository_hooks: globalMode ? 'unsupported' : 'not_probed',
      cli_runtime: 'not_probed',
      cloud_agent_runtime: 'not_probed',
    };
  }
  if (host === 'gemini') {
    return {
      ...common,
      project_trust: globalMode ? 'not_applicable_global_config' : 'not_checked',
      hooks_enabled: 'not_checked',
      disabled_hook_names: 'not_checked',
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
