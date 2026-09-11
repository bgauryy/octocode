import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { describe, expect, it } from 'vitest';
import {
  insertOutboxEvent,
  listOutboxEvents,
} from '../src/event-outbox.js';
import { parseAgentEventEnvelopeV1 } from '../src/continuity-contracts.js';
import { initDb, initializeFreshDb } from '../src/db-init.js';
import {
  SCHEMA_DDL,
  SCHEMA_INDEX_DDL,
} from '../src/db-schema.js';
import {
  EVENT_OUTBOX_V1_DDL,
  EVENT_OUTBOX_V1_INDEX_DDL,
} from '../src/db-continuity-schema.js';
import { AWARENESS_APPLICATION_ID } from '../src/storage-scope.js';
import { WORKER_LIFECYCLE_DDL } from '../src/db-worker-schema.js';

function event(id: string, retentionClass: 'delivery' | 'operational' | 'audit') {
  return {
    version: 1 as const,
    eventId: id,
    workspace: '/repo',
    type: `test.${retentionClass}`,
    retentionClass,
    actor: { kind: 'agent' as const, id: 'agent-1' },
    provenance: { source: 'tool' as const, trust: 'attributed-data' as const },
    createdAt: '2026-09-11T00:00:00Z',
    payload: { id },
  };
}

describe('typed event outbox contract', () => {
  it('persists the envelope version and explicit retention class', () => {
    const db = new DatabaseSync(':memory:');
    initializeFreshDb(db);
    insertOutboxEvent(db, event('evt-operational', 'operational'));

    expect(db.prepare(`SELECT event_type, schema_version, retention_class
      FROM event_outbox WHERE event_id = 'evt-operational'`).get()).toEqual({
      event_type: 'test.operational',
      schema_version: 1,
      retention_class: 'operational',
    });
    expect(() => parseAgentEventEnvelopeV1({
      ...event('evt-invalid', 'audit'), retentionClass: 'forever',
    })).toThrow(/retention/i);
    db.close();
  });

  it('replays a filtered stream in bounded, lossless pages', () => {
    const db = new DatabaseSync(':memory:');
    initializeFreshDb(db);
    for (let index = 1; index <= 5; index += 1) {
      insertOutboxEvent(db, event(`evt-${index}`, index % 2 ? 'audit' : 'operational'));
    }

    const first = listOutboxEvents(db, { workspace: '/repo', retentionClass: 'audit', limit: 2 });
    expect(first.events.map(({ eventId }) => eventId)).toEqual(['evt-1', 'evt-3']);
    expect(first.next).toEqual({ afterSequence: first.events[1]!.sequence });
    const second = listOutboxEvents(db, {
      workspace: '/repo', retentionClass: 'audit', limit: 2, ...first.next!,
    });
    expect(second.events.map(({ eventId }) => eventId)).toEqual(['evt-5']);
    expect(second.next).toBeNull();
    db.close();
  });

  it('classifies the exact v2 predecessor without changing its source bytes', () => {
    const directory = mkdtempSync(join(tmpdir(), 'awareness-event-v2-'));
    const database = join(directory, 'awareness.sqlite3');
    let db = new DatabaseSync(database);
    db.exec(SCHEMA_DDL.replace(/CREATE TABLE IF NOT EXISTS event_outbox[\s\S]*?\n      \);/, EVENT_OUTBOX_V1_DDL));
    db.exec(SCHEMA_INDEX_DDL.replace(/\s*CREATE INDEX IF NOT EXISTS idx_event_outbox_(?:retention|type)_sequence[^;]+;/g, ''));
    db.exec(EVENT_OUTBOX_V1_INDEX_DDL);
    db.prepare(`INSERT INTO awareness_meta(application_id, schema_version, store_id, created_at, last_migrated_at)
      VALUES (?, 2, '11111111-1111-4111-8111-111111111111', '2026-09-10T00:00:00Z', NULL)`)
      .run(AWARENESS_APPLICATION_ID);
    db.prepare(`INSERT INTO event_outbox(
      event_id, workspace_path, event_type, actor_json, provenance_json, payload_json, created_at
    ) VALUES ('old', '/repo', 'peer.message', '{"kind":"agent","id":"a"}',
      '{"source":"peer","trust":"attributed-data"}', '{}', '2026-09-10T00:00:00Z')`).run();
    db.exec(`PRAGMA application_id=${AWARENESS_APPLICATION_ID}`);

    db.close();
    const before = createHash('sha256').update(readFileSync(database)).digest('hex');
    db = new DatabaseSync(database);
    expect(() => initDb(db)).toThrow(/copy-on-write.*source has not been changed/i);
    db.close();
    expect(createHash('sha256').update(readFileSync(database)).digest('hex')).toBe(before);
    db = new DatabaseSync(database, { readOnly: true });
    expect([...db.prepare('PRAGMA table_info(event_outbox)').all()]
      .map((row) => (row as { name: string }).name)).not.toContain('retention_class');
    expect(db.prepare('SELECT schema_version, store_id FROM awareness_meta').get()).toEqual({
      schema_version: 2,
      store_id: '11111111-1111-4111-8111-111111111111',
    });
    db.close();
    rmSync(directory, { recursive: true, force: true });
  });

  it('rejects a lookalike predecessor without mutating it', () => {
    const db = new DatabaseSync(':memory:');
    db.exec(SCHEMA_DDL.replace(/CREATE TABLE IF NOT EXISTS event_outbox[\s\S]*?\n      \);/, EVENT_OUTBOX_V1_DDL));
    db.exec(SCHEMA_INDEX_DDL.replace(/\s*CREATE INDEX IF NOT EXISTS idx_event_outbox_(?:retention|type)_sequence[^;]+;/g, ''));
    db.exec(EVENT_OUTBOX_V1_INDEX_DDL);
    db.exec('ALTER TABLE event_outbox ADD COLUMN impostor TEXT');
    db.prepare(`INSERT INTO awareness_meta(application_id, schema_version, store_id, created_at, last_migrated_at)
      VALUES (?, 2, '11111111-1111-4111-8111-111111111111', '2026-09-10T00:00:00Z', NULL)`)
      .run(AWARENESS_APPLICATION_ID);
    db.exec(`PRAGMA application_id=${AWARENESS_APPLICATION_ID}`);

    expect(() => initDb(db)).toThrow(/exact|supported|canonical/i);
    expect([...db.prepare('PRAGMA table_info(event_outbox)').all()]
      .map((row) => (row as { name: string }).name)).toContain('impostor');
    expect(db.prepare('SELECT schema_version FROM awareness_meta').get()).toEqual({ schema_version: 2 });
    db.close();
  });

  it('leaves the exact worker lifecycle predecessor untouched for copy-on-write migration', () => {
    const db = new DatabaseSync(':memory:');
    initializeFreshDb(db);
    db.exec(WORKER_LIFECYCLE_DDL);
    const insert = db.prepare(`INSERT INTO worker_lifecycle_events(
      packet_id, workspace_path, session_id, worker_id, correlation_id,
      event_type, redaction, created_at, payload_json, recorded_at
    ) VALUES (?, '/repo', 'session', 'worker', ?, ?, 'public', ?, ?, ?)`);
    for (let index = 1; index <= 3; index += 1) {
      const timestamp = `2026-09-11T00:00:0${index}Z`;
      insert.run(`packet-${index}`, `correlation-${index}`, `phase-${index}`, timestamp, JSON.stringify({ index }), timestamp);
    }

    expect(() => initDb(db)).toThrow(/copy-on-write.*source has not been changed/i);
    expect(db.prepare("SELECT COUNT(*) AS count FROM worker_lifecycle_events").get()).toEqual({ count: 3 });
    expect(db.prepare("SELECT COUNT(*) AS count FROM event_outbox WHERE event_type='worker.lifecycle'").get()).toEqual({ count: 0 });
    db.close();
  });
});
