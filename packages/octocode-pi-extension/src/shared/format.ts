/** Compact numbers, durations, clock times and paths for the agents panel, the footer and `coordinate list`. */
import os from 'node:os';
import path from 'node:path';

/** `file` relative to `cwd` when inside it, else with the home directory as `~`; otherwise unchanged. */
export function shortPath(file: string, cwd = process.cwd(), home = os.homedir()): string {
  const inside = (root: string) => {
    const relative = path.relative(root, file);
    return relative && !relative.startsWith('..') && !path.isAbsolute(relative) ? relative : undefined;
  };
  const local = inside(cwd);
  if (local) return local;
  const homed = inside(home);
  return homed ? `~${path.sep}${homed}` : file;
}

export function formatTokens(count: number): string {
  if (count < 1_000) return String(count);
  return count < 100_000 ? `${(count / 1_000).toFixed(1).replace(/\.0$/, '')}k` : `${Math.round(count / 1_000)}k`;
}

/**
 * The one duration format: `45s`, `2m14s`, `15m`, `1h03m`, `2h`. `precise` adds sub-10s detail for timings
 * (`850ms`, `1.2s`).
 */
export function formatDuration(ms: number, precise = false): string {
  const safe = Number.isFinite(ms) ? Math.max(0, ms) : 0;
  if (precise && safe < 999.5) return `${Math.round(safe)}ms`;
  if (precise && safe < 9_950) return `${(safe / 1_000).toFixed(1)}s`;
  const seconds = Math.round(safe / 1_000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const pad = (value: number, unit: string) => (value ? `${String(value).padStart(2, '0')}${unit}` : '');
  return minutes < 60 ? `${minutes}m${pad(seconds % 60, 's')}` : `${Math.floor(minutes / 60)}h${pad(minutes % 60, 'm')}`;
}

export const toolCallCount = (count: number) => `${count} tool call${count === 1 ? '' : 's'}`;

export const formatClock = (at: number) => new Date(at).toTimeString().slice(0, 8);

const formatCost = (cost: number) => (cost > 0 ? ` $${cost < 0.01 ? '<0.01' : cost.toFixed(2)}` : '');

export function memberStats(member: { toolCalls: number; input: number; output: number; cost: number }): string {
  return `${toolCallCount(member.toolCalls)} · ↑${formatTokens(member.input)} ↓${formatTokens(member.output)}${formatCost(member.cost)}`;
}
