import os from 'node:os';
import path from 'node:path';

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

export function getAppendSystemTarget(
  scope: 'global' | 'project',
  cwd = process.cwd(),
  homeDir = os.homedir(),
): string {
  if (scope === 'global') {
    return path.join(homeDir, '.pi', 'agent', 'APPEND_SYSTEM.md');
  }
  return path.join(cwd, '.pi', 'APPEND_SYSTEM.md');
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
