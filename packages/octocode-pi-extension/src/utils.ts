import os from 'node:os';
import path from 'node:path';
import { PI_CONFIG_DIR, CHARS_PER_TOKEN } from './constants.js';

// 800 chars gives a meaningful preview in TUI expanded view (~20 lines of 40 chars)
// while staying well below the 12000-char agent output budget.
export const USER_VISIBLE_TOOL_PREVIEW_CHARS = 800;

export function splitArgs(input: string): string[] {
  const args: string[] = [];
  const pattern = /"([^"\\]*(?:\\.[^"\\]*)*)"|'([^'\\]*(?:\\.[^'\\]*)*)'|[^\s]+/g;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(input)) !== null) {
    const value = match[1] ?? match[2] ?? match[0];
    args.push(value.replace(/\\(["'\\])/g, '$1'));
  }
  return args;
}

export function parseSetupScope(args: string): 'global' | 'project' {
  const tokens = splitArgs(args);
  if (tokens.includes('--global') || tokens.includes('global')) return 'global';
  return 'project';
}

export function getPiProjectConfigPath(cwd = process.cwd(), ...segments: string[]): string {
  return path.join(cwd, PI_CONFIG_DIR, ...segments);
}

export function getPiProjectAgentPath(cwd = process.cwd(), ...segments: string[]): string {
  return getPiProjectConfigPath(cwd, 'agent', ...segments);
}

export function getPiUserAgentPath(homeDir = os.homedir(), ...segments: string[]): string {
  return path.join(homeDir, PI_CONFIG_DIR, 'agent', ...segments);
}

export function getPiUserSkillsDir(homeDir = os.homedir()): string {
  return getPiUserAgentPath(homeDir, 'skills');
}

export function getPiMcpConfigPath(
  scope: 'global' | 'project',
  cwd = process.cwd(),
  homeDir = os.homedir(),
): string {
  return scope === 'global'
    ? getPiUserAgentPath(homeDir, 'mcp.json')
    : getPiProjectAgentPath(cwd, 'mcp.json');
}

export function getAppendSystemTarget(
  scope: 'global' | 'project',
  cwd = process.cwd(),
  homeDir = os.homedir(),
): string {
  return scope === 'global'
    ? getPiUserAgentPath(homeDir, 'APPEND_SYSTEM.md')
    : getPiProjectConfigPath(cwd, 'APPEND_SYSTEM.md');
}

export interface TruncateResult {
  text: string;
  truncated: boolean;
  omittedChars: number;
}

export function truncateUserVisibleToolOutput(
  text: string | null | undefined,
  maxChars = USER_VISIBLE_TOOL_PREVIEW_CHARS,
): TruncateResult {
  const value = String(text ?? '');
  if (value.length <= maxChars) {
    return { text: value, truncated: false, omittedChars: 0 };
  }
  return {
    text: `${value.slice(0, maxChars)}…`,
    truncated: true,
    omittedChars: value.length - maxChars,
  };
}

/** Approximate token count for a string length, using the shared CHARS_PER_TOKEN heuristic. */
export function estimateTokens(chars: number): number {
  return Math.ceil(chars / CHARS_PER_TOKEN);
}

/**
 * The text between the first occurrence of `start` and the next `end` marker
 * (to end-of-text when `end` is absent), trimmed. Shared by the dynamic skill and
 * callTool generated-output parsers.
 */
export function sliceBetween(text: string, start: string, end: string): string {
  const i = text.indexOf(start);
  if (i < 0) return '';
  const from = i + start.length;
  const j = text.indexOf(end, from);
  return (j < 0 ? text.slice(from) : text.slice(from, j)).trim();
}

/**
 * Cap a Map to `max` entries by evicting oldest insertion-order keys. Combined
 * with delete-then-set on access, this turns a plain Map into a bounded LRU —
 * used to stop cwd-keyed caches from growing without bound in a long-lived
 * process that visits many working directories.
 */
export function capMapSize<K, V>(map: Map<K, V>, max: number): void {
  while (map.size > max) {
    const oldest = map.keys().next().value as K | undefined;
    if (oldest === undefined) break;
    map.delete(oldest);
  }
}
