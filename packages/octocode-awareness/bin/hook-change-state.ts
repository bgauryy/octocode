import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, renameSync, statSync, unlinkSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { resolveDbPath } from '../src/db-runtime.js';
import { loadWorkspacePolicy } from '../src/workspace-policy.js';
import { agentId, hookSessionCorrelation, workspace } from './hook-payload.js';

function fileToken(path: string): string {
  try {
    const stat = statSync(path);
    return `${stat.size}:${stat.mtimeMs}`;
  } catch {
    return '-';
  }
}

export function hookChangeToken(payload: Record<string, unknown>): string {
  const cwd = workspace(payload) ?? process.cwd();
  const policy = loadWorkspacePolicy(cwd).policy;
  const paths = [...new Set([
    resolveDbPath(null, { scope: policy.storage.repository, workspace: cwd }),
    resolveDbPath(null, { scope: policy.storage.memory, workspace: cwd }),
  ])];
  return createHash('sha256')
    .update(paths.flatMap((path) => [path, `${path}-wal`]).map((path) => `${path}:${fileToken(path)}`).join('\n'))
    .digest('hex');
}

export function hookChangeStatePath(payload: Record<string, unknown>): string {
  const cwd = workspace(payload) ?? process.cwd();
  const key = createHash('sha256').update(JSON.stringify([
    agentId(payload),
    hookSessionCorrelation(payload) ?? '-',
    cwd,
  ])).digest('hex');
  return join(dirname(resolveDbPath(null, { scope: 'global', workspace: cwd })), 'hook-state', 'changes', `${key}.txt`);
}

export function hookStateUnchanged(payload: Record<string, unknown>): boolean {
  try {
    return readFileSync(hookChangeStatePath(payload), 'utf8').trim() === hookChangeToken(payload);
  } catch {
    return false;
  }
}

export function recordHookChangeState(payload: Record<string, unknown>): void {
  const path = hookChangeStatePath(payload);
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  const temporary = `${path}.tmp-${process.pid}`;
  try {
    writeFileSync(temporary, `${hookChangeToken(payload)}\n`, { encoding: 'utf8', mode: 0o600 });
    renameSync(temporary, path);
  } finally {
    try { unlinkSync(temporary); } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    }
  }
}
