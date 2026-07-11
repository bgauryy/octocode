/**
 * hook-runner.ts — shared implementation for octocode-awareness lifecycle hooks.
 *
 * Shell hook files are intentionally thin wrappers. All parsing, file presence,
 * verification, briefing, and session-capture logic lives here so Claude/Codex
 * skill hooks and Pi native adapters share the same package-owned behavior.
 */
import { createHash } from 'node:crypto';
import { closeSync, mkdirSync, openSync, readFileSync, renameSync, statSync, unlinkSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { resolveDbPath } from '../src/db.js';
import { normalizeWorkspacePath } from '../src/git.js';
import { normalizeArtifact } from '../src/helpers.js';
import { endWork, listWork, startWork, touchWork } from '../src/work.js';
import { agentId, artifact, autoClaimRationale, fallbackVerificationPlan, firstString, hookSessionCorrelation, objectOrEmpty, payloadInput, resolveHookPath, sessionId } from './hook-payload.js';

export interface HookRunStateEntry {
  runId: string;
  files: string[];
  createdAt: string;
}

export const HOOK_RUN_STATE_TTL_MS = 10 * 60_000;
export const HOOK_RUN_STATE_LOCK_WAIT = new Int32Array(new SharedArrayBuffer(4));
export const HOOK_RUN_STATE_LOCK_RETRY_MS = 10;
export const HOOK_RUN_STATE_LOCK_TIMEOUT_MS = 2_000;
export const HOOK_RUN_STATE_LOCK_STALE_MS = 30_000;
export const HOOK_DB_RETRY_TIMEOUT_MS = 5_000;

export function isHookDbBusy(error: unknown): boolean {
  const sqlite = error as { errcode?: number; errstr?: string; message?: string } | null;
  const message = sqlite && typeof sqlite === 'object'
    ? `${sqlite.errstr ?? ''} ${sqlite.message ?? ''}`
    : String(error);
  return sqlite?.errcode === 5 || /database is (?:locked|busy)/i.test(message);
}

export function withHookDbRetry<T>(operation: () => T): T {
  const deadline = Date.now() + HOOK_DB_RETRY_TIMEOUT_MS;
  for (;;) {
    try {
      return operation();
    } catch (error) {
      if (!isHookDbBusy(error) || Date.now() >= deadline) throw error;
      Atomics.wait(HOOK_RUN_STATE_LOCK_WAIT, 0, 0, HOOK_RUN_STATE_LOCK_RETRY_MS);
    }
  }
}

export function hookRunStateDir(): string {
  const stateDir = join(dirname(resolveDbPath(null)), 'hook-state', 'runs');
  mkdirSync(stateDir, { recursive: true });
  return stateDir;
}

export function hookRunStateFile(key: string): string {
  return join(hookRunStateDir(), `${key}.json`);
}

export function processIsAlive(pid: number): boolean {
  if (!Number.isSafeInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code !== 'ESRCH';
  }
}

export function removeStaleHookRunStateLock(lockFile: string): boolean {
  try {
    const owner = Number.parseInt(readFileSync(lockFile, 'utf8'), 10);
    const staleByAge = Date.now() - statSync(lockFile).mtimeMs > HOOK_RUN_STATE_LOCK_STALE_MS;
    const validOwner = Number.isSafeInteger(owner) && owner > 0;
    // open('wx') creates the lock before its owner PID can be written. Another
    // process may observe that brief empty-file window; a fresh invalid owner
    // is therefore busy, not stale. Only age may reclaim malformed locks.
    if (!staleByAge && (!validOwner || processIsAlive(owner))) return false;
    unlinkSync(lockFile);
    return true;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return true;
    return false;
  }
}

export function withHookRunStateLock<T>(key: string, operation: () => T): T {
  const lockFile = `${hookRunStateFile(key)}.lock`;
  const deadline = Date.now() + HOOK_RUN_STATE_LOCK_TIMEOUT_MS;
  for (;;) {
    try {
      const fd = openSync(lockFile, 'wx', 0o600);
      try {
        writeFileSync(fd, `${process.pid}\n`, 'utf8');
      } finally {
        closeSync(fd);
      }
      try {
        return operation();
      } finally {
        try { unlinkSync(lockFile); } catch (error) {
          if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
        }
      }
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'EEXIST') throw error;
      if (removeStaleHookRunStateLock(lockFile)) continue;
      if (Date.now() >= deadline) {
        throw new Error(`timed out waiting for hook correlation state: ${lockFile}`);
      }
      Atomics.wait(HOOK_RUN_STATE_LOCK_WAIT, 0, 0, HOOK_RUN_STATE_LOCK_RETRY_MS);
    }
  }
}

export function readHookRunEntries(key: string): HookRunStateEntry[] {
  try {
    const parsed = JSON.parse(readFileSync(hookRunStateFile(key), 'utf8')) as unknown;
    if (!Array.isArray(parsed)) return [];
    const cutoff = Date.now() - HOOK_RUN_STATE_TTL_MS;
    return parsed.filter((entry): entry is HookRunStateEntry => {
      if (!entry || typeof entry !== 'object') return false;
      const candidate = entry as Partial<HookRunStateEntry>;
      const createdAt = typeof candidate.createdAt === 'string' ? Date.parse(candidate.createdAt) : NaN;
      return typeof candidate.runId === 'string'
        && candidate.runId.length > 0
        && Array.isArray(candidate.files)
        && candidate.files.every((file) => typeof file === 'string' && file.length > 0)
        && Number.isFinite(createdAt)
        && createdAt >= cutoff;
    });
  } catch {
    return [];
  }
}

export function writeHookRunEntries(key: string, entries: HookRunStateEntry[]): void {
  const file = hookRunStateFile(key);
  if (entries.length === 0) {
    try { unlinkSync(file); } catch { /* already absent */ }
    return;
  }
  const tempFile = `${file}.${process.pid}.${Date.now()}.tmp`;
  writeFileSync(tempFile, JSON.stringify(entries, null, 2) + '\n', 'utf8');
  renameSync(tempFile, file);
}

export function hookEventId(payload: Record<string, unknown>): string | null {
  const input = objectOrEmpty(payloadInput(payload));
  return firstString(
    payload.tool_use_id,
    payload.toolUseId,
    payload.tool_call_id,
    payload.toolCallId,
    payload.event_id,
    payload.eventId,
    payload.id,
    input.tool_use_id,
    input.toolUseId,
    input.tool_call_id,
    input.toolCallId,
    input.event_id,
    input.eventId,
    input.id,
  );
}

export function hookRunKey(payload: Record<string, unknown>, files: string[], cwd: string): string {
  const explicitId = hookEventId(payload);
  const identity = {
    agent: agentId(payload),
    workspace: normalizeWorkspacePath(cwd, cwd) ?? resolve(cwd),
    artifact: artifact(payload),
    event: explicitId,
    files: explicitId ? [] : files.map(file => resolveHookPath(file, cwd)).sort(),
  };
  return createHash('sha1').update(JSON.stringify(identity)).digest('hex');
}

export const HOOK_AGGREGATE_CONTEXT_PREFIX = 'hook-scope:';

export function hookAggregateContextRef(payload: Record<string, unknown>, cwd: string): string | null {
  const sessionCorrelation = hookSessionCorrelation(payload);
  if (!sessionCorrelation) return null;
  const identity = {
    agent: agentId(payload),
    session: sessionCorrelation,
    workspace: normalizeWorkspacePath(cwd, cwd) ?? resolve(cwd),
    artifact: normalizeArtifact(artifact(payload)),
  };
  return `${HOOK_AGGREGATE_CONTEXT_PREFIX}${createHash('sha1').update(JSON.stringify(identity)).digest('hex')}`;
}

export function activeFallbackHookRun(
  database: DatabaseSync,
  payload: Record<string, unknown>,
  cwd: string,
): string | null {
  const contextRef = hookAggregateContextRef(payload, cwd);
  if (!contextRef) return null;
  const row = database.prepare(`SELECT run_id FROM task_runs
    WHERE origin = 'HOOK' AND status = 'ACTIVE' AND agent_id = ?
      AND workspace_path = ? AND artifact IS ? AND context_ref = ?
    ORDER BY updated_at DESC, created_at DESC LIMIT 1`).get(
    agentId(payload),
    normalizeWorkspacePath(cwd, cwd) ?? resolve(cwd),
    normalizeArtifact(artifact(payload)),
    contextRef,
  ) as { run_id: string } | undefined;
  return row?.run_id ?? null;
}

export function hookAggregateLockKey(payload: Record<string, unknown>, cwd: string): string | null {
  const contextRef = hookAggregateContextRef(payload, cwd);
  return contextRef
    ? `aggregate-${createHash('sha1').update(contextRef).digest('hex')}`
    : null;
}

export function startOrAttachFallbackHookRun(
  database: DatabaseSync,
  payload: Record<string, unknown>,
  cwd: string,
  files: string[],
) {
  const contextRef = hookAggregateContextRef(payload, cwd);
  const startOrAttach = () => {
    const existingRunId = activeFallbackHookRun(database, payload, cwd);
    const result = startWork(database, {
      agentId: agentId(payload),
      sessionId: sessionId(payload),
      workspacePath: cwd,
      artifact: artifact(payload),
      runId: existingRunId ?? undefined,
      rationale: autoClaimRationale(payload, files),
      testPlan: fallbackVerificationPlan(files, cwd),
      contextRef: contextRef ?? undefined,
      targetFiles: files,
      origin: 'HOOK',
      source: 'HOOK',
      ttlMs: 10 * 60_000,
    });
    if (result.ok && existingRunId) {
      touchWork(database, {
        agentId: agentId(payload),
        runId: existingRunId,
        ttlMs: 10 * 60_000,
      });
    }
    return result;
  };
  const lockKey = hookAggregateLockKey(payload, cwd);
  return lockKey ? withHookRunStateLock(lockKey, startOrAttach) : startOrAttach();
}

export function refreshFallbackVerificationPlan(
  database: DatabaseSync,
  runId: string,
  cwd: string,
): void {
  if (!isAggregatedFallbackHookRun(database, runId)) return;
  const files = database.prepare('SELECT file_path FROM run_files WHERE run_id = ? ORDER BY file_path')
    .all(runId) as unknown as Array<{ file_path: string }>;
  database.prepare("UPDATE task_runs SET test_plan = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE run_id = ? AND origin = 'HOOK'")
    .run(fallbackVerificationPlan(files.map(file => file.file_path), cwd), runId);
}

export function isAggregatedFallbackHookRun(database: DatabaseSync, runId: string): boolean {
  const row = database.prepare(`SELECT origin, context_ref FROM task_runs WHERE run_id = ?`).get(runId) as {
    origin: string;
    context_ref: string | null;
  } | undefined;
  return row?.origin === 'HOOK' && row.context_ref?.startsWith(HOOK_AGGREGATE_CONTEXT_PREFIX) === true;
}

export function finalizeActiveFallbackHookRuns(
  database: DatabaseSync,
  payload: Record<string, unknown>,
  cwd: string,
): string[] {
  const contextRef = hookAggregateContextRef(payload, cwd);
  if (!contextRef) return [];
  const rows = database.prepare(`SELECT run_id FROM task_runs
    WHERE origin = 'HOOK' AND status = 'ACTIVE' AND agent_id = ?
      AND workspace_path = ? AND artifact IS ? AND context_ref = ?
    ORDER BY created_at`).all(
    agentId(payload),
    normalizeWorkspacePath(cwd, cwd) ?? resolve(cwd),
    normalizeArtifact(artifact(payload)),
    contextRef,
  ) as unknown as Array<{ run_id: string }>;
  const finalized: string[] = [];
  for (const row of rows) {
    endWork(database, { agentId: agentId(payload), runId: row.run_id });
    finalized.push(row.run_id);
  }
  return finalized;
}

export function recordHookRun(payload: Record<string, unknown>, files: string[], cwd: string, runId: string): void {
  const key = hookRunKey(payload, files, cwd);
  withHookRunStateLock(key, () => {
    const entries = readHookRunEntries(key);
    entries.push({
      runId,
      files: files.map(file => resolveHookPath(file, cwd)),
      createdAt: new Date().toISOString(),
    });
    writeHookRunEntries(key, entries.slice(-20));
  });
}

export function consumeHookRun(
  database: DatabaseSync,
  payload: Record<string, unknown>,
  files: string[],
  cwd: string,
): string | null {
  const key = hookRunKey(payload, files, cwd);
  return withHookRunStateLock(key, () => {
    const entries = readHookRunEntries(key);
    const activeEntries = entries.filter((entry) => {
      const activeFiles = new Set(listWork(database, {
        agentId: agentId(payload),
        workspacePath: cwd,
        artifact: artifact(payload),
        runId: entry.runId,
        activeOnly: true,
      }).files.map((file) => file.file_path));
      return entry.files.every((file) => activeFiles.has(file));
    });
    // Newest-first avoids a previously abandoned same-key event consuming the
    // post-edit for a later retry. Other live entries stay queued.
    const entry = activeEntries.pop() ?? null;
    writeHookRunEntries(key, activeEntries);
    return entry?.runId ?? null;
  });
}

export function activeRunForFiles(
  database: DatabaseSync,
  params: {
    agentId: string;
    workspacePath: string;
    artifact: string | null;
    files: string[];
    origins: Array<'WORK' | 'HOOK'>;
  },
): string | null {
  const absFiles = params.files.map(file => resolveHookPath(file, params.workspacePath));
  if (absFiles.length === 0) return null;
  const rows = listWork(database, {
    agentId: params.agentId,
    workspacePath: params.workspacePath,
    artifact: params.artifact,
    activeOnly: true,
  }).files.filter((entry) => params.origins.includes(entry.origin as 'WORK' | 'HOOK'));
  const byRun = new Map<string, Set<string>>();
  for (const row of rows) {
    const paths = byRun.get(row.run_id) ?? new Set<string>();
    paths.add(row.file_path);
    byRun.set(row.run_id, paths);
  }
  const matches = [...byRun].filter(([, paths]) => absFiles.every(file => paths.has(file)));
  return matches.length === 1 ? matches[0]![0] : null;
}

export function runOrigin(database: DatabaseSync, runId: string): 'TASK' | 'WORK' | 'HOOK' | null {
  const row = database.prepare('SELECT origin FROM task_runs WHERE run_id = ?').get(runId) as { origin: 'TASK' | 'WORK' | 'HOOK' } | undefined;
  return row?.origin ?? null;
}
