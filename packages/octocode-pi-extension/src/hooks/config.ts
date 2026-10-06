import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { globalPaths, projectHookFiles, resolveToolPath } from '../shared/home.js';
import { errorMessage, isRecord } from '../shared/util.js';

/**
 * Claude Code / Codex command hooks, read from the files those agents use. The shape is theirs:
 * `{ "hooks": { "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "…", "timeout": 60 }] }] } }`.
 * Only `command` hooks for the six events Pi can map are kept; anything else is ignored.
 */

export const HOOK_EVENTS = ['PreToolUse', 'PostToolUse', 'SessionStart', 'PreCompact', 'Stop', 'Notification'] as const;
export type HookEvent = (typeof HOOK_EVENTS)[number];

export interface HookCommand {
  command: string;
  timeoutMs: number;
  /** The file it came from. */
  source: string;
}

interface HookGroup {
  /** Regex over the tool name (or the SessionStart/PreCompact trigger); empty or `*` matches all. */
  matcher: string;
  hooks: HookCommand[];
}

export type HookConfig = Record<HookEvent, HookGroup[]>;

const DEFAULT_TIMEOUT_S = 60;
const MAX_TIMEOUT_S = 600;

export const emptyHooks = (): HookConfig => ({ PreToolUse: [], PostToolUse: [], SessionStart: [], PreCompact: [], Stop: [], Notification: [] });

/**
 * User hook files always (`~/.claude/settings.json`, `~/.codex/hooks.json`, `~/.octocode/hooks.json`); project files
 * (the same three names under the repo root, plus `.claude/settings.local.json`) only when Pi trusts the project.
 */
export function hookFiles(cwd: string, projectTrusted: boolean, home = os.homedir()): string[] {
  return [
    path.join(home, '.claude', 'settings.json'),
    path.join(home, '.codex', 'hooks.json'),
    globalPaths(process.env, home).hooks,
    ...(projectTrusted ? projectHookFiles(cwd) : []),
  ];
}

/** Adds the command hooks in one parsed file to `into`. */
export function parseHooks(value: unknown, source: string, into: HookConfig = emptyHooks()): HookConfig {
  const hooks = isRecord(value) ? value.hooks : undefined;
  if (!isRecord(hooks)) return into;
  for (const event of HOOK_EVENTS) {
    const groups = hooks[event];
    if (!Array.isArray(groups)) continue;
    for (const group of groups) {
      if (!isRecord(group) || !Array.isArray(group.hooks)) continue;
      const commands = group.hooks.flatMap((hook): HookCommand[] => {
        if (!isRecord(hook) || hook.type !== 'command' || typeof hook.command !== 'string' || !hook.command.trim()) return [];
        const seconds = typeof hook.timeout === 'number' && hook.timeout > 0 ? Math.min(hook.timeout, MAX_TIMEOUT_S) : DEFAULT_TIMEOUT_S;
        return [{ command: hook.command, timeoutMs: seconds * 1000, source }];
      });
      if (commands.length > 0) into[event].push({ matcher: typeof group.matcher === 'string' ? group.matcher : '', hooks: commands });
    }
  }
  return into;
}

export function loadHooks(cwd: string, projectTrusted: boolean, home = os.homedir()): { config: HookConfig; errors: string[] } {
  const config = emptyHooks();
  const errors: string[] = [];
  for (const file of new Set(hookFiles(cwd, projectTrusted, home))) {
    let text: string;
    try {
      text = fs.readFileSync(file, 'utf8');
    } catch {
      continue;
    }
    try {
      parseHooks(JSON.parse(text), file, config);
    } catch (error) {
      errors.push(`${file}: ${errorMessage(error)}`);
    }
  }
  return { config, errors };
}

/** Claude Code tool names for Pi's tools, so `"matcher": "Bash"` or `"Edit|Write"` work unchanged. */
const CLAUDE_NAMES: Record<string, string[]> = {
  bash: ['Bash'],
  read: ['Read'],
  edit: ['Edit'],
  write: ['Write'],
  file: ['Edit', 'Write', 'MultiEdit'],
  grep: ['Grep'],
  find: ['Glob'],
  ls: ['LS'],
  web: ['WebFetch', 'WebSearch'],
  agent: ['Task'],
};

export const toolAliases = (tool: string): string[] => [tool, ...(CLAUDE_NAMES[tool] ?? [])];

/** One hook dispatch: the matcher names, and the Claude Code `tool_name` / `tool_input` the hook receives on stdin. */
export interface HookCall {
  names: string[];
  tool_name: string;
  tool_input: unknown;
}

const text = (value: unknown): string => (typeof value === 'string' ? value : '');

/** One `file` query in Claude Code's shape: Edit / MultiEdit `{file_path, old_string, new_string | edits}`, Write `{file_path, content}`. */
function fileQueryCall(query: Record<string, unknown>, cwd: string): HookCall {
  const file_path = resolveToolPath(cwd, text(query['path']));
  if (query['type'] === 'write') return { names: ['file', 'Write'], tool_name: 'Write', tool_input: { file_path, content: text(query['content']) } };
  if (query['type'] === 'delete') return { names: ['file', 'Delete'], tool_name: 'Delete', tool_input: { file_path } };
  const edits = (Array.isArray(query['edits']) ? query['edits'] : []).filter(isRecord).map((edit) => ({ old_string: text(edit['oldText']), new_string: text(edit['newText']) }));
  if (edits.length === 1) return { names: ['file', 'Edit'], tool_name: 'Edit', tool_input: { file_path, ...edits[0] } };
  return { names: ['file', 'MultiEdit'], tool_name: 'MultiEdit', tool_input: { file_path, edits } };
}

/**
 * The hook dispatches for one Pi tool call, with Claude Code's tool names and inputs so hooks written for Claude Code
 * (`jq .tool_input.file_path`, `"matcher": "Edit|Write"`) work unchanged: a `file` call is one dispatch per query
 * (Edit, MultiEdit, Write, or Delete, which Claude Code lacks); `read` and Pi's `edit`/`write` send `file_path`;
 * `bash` already takes Claude's `{command}`. Other tools keep their Pi name and input.
 */
export function hookCalls(tool: string, input: Record<string, unknown>, cwd: string): HookCall[] {
  if (tool === 'file') return (Array.isArray(input['queries']) ? input['queries'] : []).filter(isRecord).map((query) => fileQueryCall(query, cwd));
  if (tool === 'bash') return [{ names: toolAliases(tool), tool_name: 'Bash', tool_input: input }];
  const claude = { read: 'Read', edit: 'Edit', write: 'Write' }[tool];
  if (claude) {
    const { path: file, ...rest } = input;
    return [{ names: toolAliases(tool), tool_name: claude, tool_input: { ...rest, file_path: resolveToolPath(cwd, text(file)) } }];
  }
  return [{ names: toolAliases(tool), tool_name: tool, tool_input: input }];
}

/** Whether a group's matcher selects any of `names`. An invalid regex matches nothing. */
export function matches(matcher: string, names: string[]): boolean {
  if (!matcher || matcher === '*') return true;
  try {
    const pattern = new RegExp(`^(?:${matcher})$`);
    return names.some((name) => pattern.test(name));
  } catch {
    return false;
  }
}

/** The hooks matching `names`; the same command configured in several files runs once (as in Claude Code). */
export function commandsFor(config: HookConfig, event: HookEvent, names: string[]): HookCommand[] {
  const seen = new Set<string>();
  return config[event].filter((group) => matches(group.matcher, names)).flatMap((group) => group.hooks).filter((hook) => !seen.has(hook.command) && Boolean(seen.add(hook.command)));
}
