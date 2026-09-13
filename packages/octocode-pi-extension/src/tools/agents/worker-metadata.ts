import fs from 'node:fs';
import type { WorkerWorktreeState } from '../../types.js';

export function getArgValue(args: string[], flag: string): string | undefined {
  const index = args.indexOf(flag);
  return index >= 0 ? args[index + 1] : undefined;
}

export function getArgCsv(args: string[], flag: string): string[] | undefined {
  const value = getArgValue(args, flag);
  return value ? value.split(',').map((item) => item.trim()).filter(Boolean) : undefined;
}

export function statHandbackArtifact(
  filePath: string,
): { path: string; exists: boolean; bytes?: number; modifiedAt?: string } {
  try {
    const stat = fs.statSync(filePath);
    if (!stat.isFile()) return { path: filePath, exists: false };
    return { path: filePath, exists: true, bytes: stat.size, modifiedAt: stat.mtime.toISOString() };
  } catch {
    return { path: filePath, exists: false };
  }
}

export function worktreeSnapshot(worktree: WorkerWorktreeState | undefined): WorkerWorktreeState | undefined {
  return worktree ? { ...worktree } : undefined;
}
