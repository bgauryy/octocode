import { afterEach, describe, expect, it } from 'vitest';
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  utimesSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { connectDb } from '../src/db-runtime.js';
import {
  activeFallbackHookRun,
  activeRunForFiles,
  consumeHookRun,
  finalizeActiveFallbackHookRuns,
  hookAggregateContextRef,
  hookAggregateLockKey,
  hookEventId,
  hookRunKey,
  hookRunStateFile,
  isAggregatedFallbackHookRun,
  isHookDbBusy,
  processIsAlive,
  readHookRunEntries,
  recordHookRun,
  refreshFallbackVerificationPlan,
  removeStaleHookRunStateLock,
  runOrigin,
  startOrAttachFallbackHookRun,
  withHookDbRetry,
  withHookRunStateLock,
  writeHookRunEntries,
} from '../src/hooks/run-state.js';

const roots: string[] = [];

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function fixture() {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-hook-state-')));
  roots.push(workspace);
  return workspace;
}

describe('host hook run-state boundaries', () => {
  it('classifies SQLite contention, retries transient busy work, and probes process ownership', () => {
    expect(isHookDbBusy({ errcode: 5 })).toBe(true);
    expect(isHookDbBusy({ message: 'database is locked' })).toBe(true);
    expect(isHookDbBusy('DATABASE IS BUSY')).toBe(true);
    expect(isHookDbBusy({ message: 'constraint failed' })).toBe(false);
    expect(isHookDbBusy(null)).toBe(false);

    let attempts = 0;
    expect(withHookDbRetry(() => {
      attempts++;
      if (attempts === 1) throw new Error('database is busy');
      return 'committed';
    })).toBe('committed');
    expect(attempts).toBe(2);
    expect(() => withHookDbRetry(() => { throw new Error('not retryable'); })).toThrow('not retryable');

    expect(processIsAlive(0)).toBe(false);
    expect(processIsAlive(Number.NaN)).toBe(false);
    expect(processIsAlive(process.pid)).toBe(true);
    expect(processIsAlive(99_999_999)).toBe(false);
  });

  it('reclaims only stale file locks and keeps lock cleanup exception-safe', () => {
    const workspace = fixture();
    const stateFile = hookRunStateFile('lease', workspace);
    const lockFile = `${stateFile}.lock`;
    writeFileSync(lockFile, `${process.pid}\n`);
    expect(removeStaleHookRunStateLock(lockFile)).toBe(false);
    writeFileSync(lockFile, 'not-a-pid\n');
    expect(removeStaleHookRunStateLock(lockFile)).toBe(false);
    const old = new Date(Date.now() - 60_000);
    utimesSync(lockFile, old, old);
    expect(removeStaleHookRunStateLock(lockFile)).toBe(true);
    expect(removeStaleHookRunStateLock(lockFile)).toBe(true);

    mkdirSync(lockFile);
    expect(removeStaleHookRunStateLock(lockFile)).toBe(false);
    rmSync(lockFile, { recursive: true });
    expect(withHookRunStateLock('lease', () => 'owned', workspace)).toBe('owned');
    expect(() => withHookRunStateLock('lease', () => { throw new Error('operation failed'); }, workspace))
      .toThrow('operation failed');
    expect(withHookRunStateLock('lease', () => 'reacquired', workspace)).toBe('reacquired');
  });

  it('filters corrupt and expired correlation entries before consuming live work', () => {
    const workspace = fixture();
    writeFileSync(hookRunStateFile('bad-json', workspace), '{');
    expect(readHookRunEntries('bad-json', workspace)).toEqual([]);
    writeFileSync(hookRunStateFile('not-array', workspace), '{}');
    expect(readHookRunEntries('not-array', workspace)).toEqual([]);
    writeHookRunEntries('entries', [
      { runId: '', files: ['a.ts'], createdAt: new Date().toISOString() },
      { runId: 'bad-files', files: [''], createdAt: new Date().toISOString() },
      { runId: 'bad-date', files: ['a.ts'], createdAt: 'not-a-date' },
      { runId: 'expired', files: ['a.ts'], createdAt: '2000-01-01T00:00:00Z' },
      { runId: 'live', files: ['a.ts'], createdAt: new Date().toISOString() },
    ], workspace);
    expect(readHookRunEntries('entries', workspace)).toEqual([
      { runId: 'live', files: ['a.ts'], createdAt: expect.any(String) },
    ]);
    writeHookRunEntries('entries', [], workspace);
    expect(readHookRunEntries('entries', workspace)).toEqual([]);
  });

  it('aggregates a host session into one fallback run and consumes exact event correlation', () => {
    const workspace = fixture();
    const database = connectDb(join(workspace, 'awareness.sqlite3'));
    const payload = {
      agent_id: 'hook-agent',
      session_id: 'hook-session',
      cwd: workspace,
      artifact: 'release',
      tool_use_id: 'tool-1',
      tool_name: 'Write',
      tool_input: { file_path: 'src/a.ts' },
    };
    try {
      expect(hookEventId(payload)).toBe('tool-1');
      expect(hookEventId({ input: { eventId: 'nested-event' } })).toBe('nested-event');
      expect(hookEventId({})).toBeNull();
      expect(hookRunKey(payload, ['src/b.ts', 'src/a.ts'], workspace))
        .toBe(hookRunKey(payload, ['ignored.ts'], workspace));
      expect(hookRunKey({ ...payload, tool_use_id: undefined }, ['src/b.ts', 'src/a.ts'], workspace))
        .toBe(hookRunKey({ ...payload, tool_use_id: undefined }, ['src/a.ts', 'src/b.ts'], workspace));
      expect(hookAggregateContextRef({ agent_id: 'hook-agent' }, workspace)).toBeNull();
      expect(hookAggregateContextRef(payload, workspace)).toMatch(/^hook-scope:/);
      expect(hookAggregateLockKey(payload, workspace)).toMatch(/^aggregate-/);
      expect(hookAggregateLockKey({ agent_id: 'hook-agent' }, workspace)).toBeNull();
      expect(activeFallbackHookRun(database, { agent_id: 'hook-agent' }, workspace)).toBeNull();

      const started = startOrAttachFallbackHookRun(database, payload, workspace, ['src/a.ts']);
      expect(started.ok).toBe(true);
      if (!started.ok) throw new Error('expected hook run');
      const runId = started.run.run_id;
      const attached = startOrAttachFallbackHookRun(database, payload, workspace, ['src/b.ts']);
      expect(attached.ok).toBe(true);
      if (!attached.ok) throw new Error('expected attached hook run');
      expect(attached.run.run_id).toBe(runId);
      expect(activeFallbackHookRun(database, payload, workspace)).toBe(runId);
      expect(isAggregatedFallbackHookRun(database, runId)).toBe(true);
      expect(isAggregatedFallbackHookRun(database, 'missing')).toBe(false);
      refreshFallbackVerificationPlan(database, 'missing', workspace);
      refreshFallbackVerificationPlan(database, runId, workspace);
      expect(runOrigin(database, runId)).toBe('HOOK');
      expect(runOrigin(database, 'missing')).toBeNull();
      expect(activeRunForFiles(database, {
        agentId: 'hook-agent', workspacePath: workspace, artifact: 'release',
        files: ['src/a.ts', 'src/b.ts'], origins: ['HOOK'],
      })).toBe(runId);
      expect(activeRunForFiles(database, {
        agentId: 'hook-agent', workspacePath: workspace, artifact: 'release', files: [], origins: ['HOOK'],
      })).toBeNull();

      recordHookRun(payload, ['src/a.ts'], workspace, runId);
      expect(readFileSync(hookRunStateFile(hookRunKey(payload, ['src/a.ts'], workspace), workspace), 'utf8'))
        .toContain(runId);
      expect(consumeHookRun(database, payload, ['src/a.ts'], workspace)).toBe(runId);
      expect(consumeHookRun(database, payload, ['src/a.ts'], workspace)).toBeNull();
      expect(finalizeActiveFallbackHookRuns(database, { agent_id: 'hook-agent' }, workspace)).toEqual([]);
      expect(finalizeActiveFallbackHookRuns(database, payload, workspace)).toEqual([runId]);
      expect(activeFallbackHookRun(database, payload, workspace)).toBeNull();
    } finally {
      database.close();
    }
  });
});
