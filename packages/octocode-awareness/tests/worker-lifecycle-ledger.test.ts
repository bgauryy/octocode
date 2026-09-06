import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import {
  appendWorkerLifecycleEvent,
  connectDb,
  listWorkerLifecycleEvents,
  type WorkerLifecycleEventInput,
} from '../src/index.js';

const directories: string[] = [];

function fixturePath(): string {
  const directory = mkdtempSync(join(tmpdir(), 'awareness-worker-ledger-'));
  directories.push(directory);
  return join(directory, 'awareness.sqlite3');
}

function event(overrides: Partial<WorkerLifecycleEventInput> = {}): WorkerLifecycleEventInput {
  return {
    packetId: 'packet-1',
    workspace: '/workspace/repository-a',
    sessionId: 'session-1',
    workerId: 'worker-1',
    correlationId: 'correlation-1',
    type: 'worker.spawn',
    redaction: 'sensitive',
    createdAt: '2026-08-28T12:00:00.000Z',
    payload: { promptDigest: 'sha256:abc', capabilities: { maxTurns: 4, tools: ['localSearch'] } },
    ...overrides,
  };
}

afterEach(() => {
  for (const directory of directories.splice(0)) rmSync(directory, { recursive: true, force: true });
});

describe('worker lifecycle ledger', () => {
  it('appends ordered correlated events and treats an identical packet as idempotent', () => {
    const dbPath = fixturePath();
    const db = connectDb(dbPath);
    try {
      const first = appendWorkerLifecycleEvent(db, event());
      const duplicate = appendWorkerLifecycleEvent(db, event({ payload: { capabilities: { tools: ['localSearch'], maxTurns: 4 }, promptDigest: 'sha256:abc' } }));
      const second = appendWorkerLifecycleEvent(db, event({
        packetId: 'packet-2',
        type: 'worker.state',
        createdAt: '2026-08-28T12:00:01.000Z',
        payload: { state: 'running' },
      }));

      expect(first).toMatchObject({ duplicate: false, event: { sequence: 1, packetId: 'packet-1' } });
      expect(duplicate).toEqual({ duplicate: true, event: first.event });
      expect(second.event.sequence).toBe(2);
      expect(listWorkerLifecycleEvents(db, {
        workspace: '/workspace/repository-a',
        sessionId: 'session-1',
      })).toEqual([first.event, second.event]);
    } finally {
      db.close();
    }
  });

  it('rejects reuse of a packet id with divergent envelope or payload data', () => {
    const dbPath = fixturePath();
    const db = connectDb(dbPath);
    try {
      appendWorkerLifecycleEvent(db, event());
      expect(() => appendWorkerLifecycleEvent(db, event({ workerId: 'worker-2' }))).toThrow(/packetId.*different/i);
      expect(() => appendWorkerLifecycleEvent(db, event({ payload: { promptDigest: 'sha256:different' } }))).toThrow(/packetId.*different/i);
    } finally {
      db.close();
    }
  });

  it('requires workspace and session scope while supporting indexed replay filters', () => {
    const dbPath = fixturePath();
    const db = connectDb(dbPath);
    try {
      const events = [
        event({ packetId: 'a-1', workerId: 'worker-a', correlationId: 'corr-a' }),
        event({ packetId: 'a-2', workerId: 'worker-b', correlationId: 'corr-b', type: 'worker.state', payload: { state: 'queued' } }),
        event({ packetId: 'session-2', sessionId: 'session-2' }),
        event({ packetId: 'workspace-b', workspace: '/workspace/repository-b' }),
      ];
      for (const entry of events) appendWorkerLifecycleEvent(db, entry);

      expect(listWorkerLifecycleEvents(db, {
        workspace: '/workspace/repository-a', sessionId: 'session-1', workerId: 'worker-b',
      }).map(({ packetId }) => packetId)).toEqual(['a-2']);
      expect(listWorkerLifecycleEvents(db, {
        workspace: '/workspace/repository-a', sessionId: 'session-1', correlationId: 'corr-a', afterSequence: 0,
      }).map(({ packetId }) => packetId)).toEqual(['a-1']);
      expect(listWorkerLifecycleEvents(db, {
        workspace: '/workspace/repository-a', sessionId: 'session-1', type: 'worker.state',
      }).map(({ packetId }) => packetId)).toEqual(['a-2']);
    } finally {
      db.close();
    }
  });

  it('replays committed events after the SQLite connection restarts', () => {
    const dbPath = fixturePath();
    const initial = connectDb(dbPath);
    appendWorkerLifecycleEvent(initial, event());
    initial.close();

    const reopened = connectDb(dbPath);
    try {
      expect(listWorkerLifecycleEvents(reopened, {
        workspace: '/workspace/repository-a', sessionId: 'session-1',
      })).toMatchObject([{ packetId: 'packet-1', payload: { promptDigest: 'sha256:abc' } }]);
    } finally {
      reopened.close();
    }
  });

  it('rejects malformed, oversized, non-JSON, and secret-like durable content', () => {
    const dbPath = fixturePath();
    const db = connectDb(dbPath);
    try {
      expect(() => appendWorkerLifecycleEvent(db, event({ createdAt: 'not-a-date' }))).toThrow(/createdAt/);
      expect(() => appendWorkerLifecycleEvent(db, event({ redaction: 'unsafe' as 'public' }))).toThrow(/redaction/);
      expect(() => appendWorkerLifecycleEvent(db, event({ workerId: 'worker\u0000unsafe' }))).toThrow(/control characters/);
      expect(() => appendWorkerLifecycleEvent(db, event({ payload: { output: 'x'.repeat(70_000) } }))).toThrow(/payload.*bytes/i);
      expect(() => appendWorkerLifecycleEvent(db, event({ payload: { password: 'extremely-secret-value' } }))).toThrow(/secret-like/i);
      expect(() => appendWorkerLifecycleEvent(db, event({ payload: { value: Number.NaN } }))).toThrow(/JSON-safe/i);
      const cyclic: Record<string, unknown> = {};
      cyclic.self = cyclic;
      expect(() => appendWorkerLifecycleEvent(db, event({
        payload: cyclic as WorkerLifecycleEventInput['payload'],
      }))).toThrow(/JSON-safe/i);
      expect(() => listWorkerLifecycleEvents(db, {
        workspace: '/workspace/repository-a', sessionId: 'session-1', limit: 0,
      })).toThrow(/limit/);
    } finally {
      db.close();
    }
  });

  it('fails closed when persisted payload bytes are malformed', () => {
    const dbPath = fixturePath();
    const db = connectDb(dbPath);
    try {
      appendWorkerLifecycleEvent(db, event());
      db.prepare('UPDATE worker_lifecycle_events SET payload_json = ? WHERE packet_id = ?').run('{', 'packet-1');
      expect(() => listWorkerLifecycleEvents(db, {
        workspace: '/workspace/repository-a', sessionId: 'session-1',
      })).toThrow(/malformed payload.*packet-1/i);
    } finally {
      db.close();
    }
  });
});
