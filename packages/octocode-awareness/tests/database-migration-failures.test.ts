import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { AGENT_APPLICATION_ID } from '@octocodeai/agent-contracts/schema';
import { afterEach, describe, expect, it } from 'vitest';
import { copyLegacyHandoffSignals, legacyHandoffEvents } from '../src/db-consolidation-handoffs.js';
import {
  consolidateDatabase,
  previewDatabaseMigration,
  verifyDatabaseMigration,
} from '../src/db-consolidation.js';
import {
  assertLogicalDestination,
  copyMappedTables,
  copySequenceHighWaterMarks,
  scalar,
  verifyLocalGitObjects,
} from '../src/db-consolidation-validation.js';
import { EVENT_OUTBOX_V1_DDL, EVENT_OUTBOX_V1_INDEX_DDL } from '../src/db-continuity-schema.js';
import { initDb } from '../src/db-init.js';
import {
  inspectSchemaState,
  readAwarenessMeta,
  resolveAwarenessStoreIdentity,
} from '../src/db-introspection.js';
import {
  migrationSourceVersion,
  schemaStateForMigrationVersion,
} from '../src/db-migration-contracts.js';

const roots: string[] = [];

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function temporaryRoot(label: string): string {
  const root = realpathSync(mkdtempSync(join(tmpdir(), `${label}-`)));
  roots.push(root);
  return root;
}

function digest(path: string): string {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function verificationFixture() {
  const root = temporaryRoot('awareness-verification-failures');
  const sourcePath = join(root, 'source.sqlite3');
  const destinationPath = join(root, 'destination.sqlite3');
  const source = new DatabaseSync(sourcePath);
  initDb(source);
  source.exec('DROP TABLE event_outbox');
  source.exec(EVENT_OUTBOX_V1_DDL);
  source.exec(EVENT_OUTBOX_V1_INDEX_DDL);
  source.prepare('UPDATE awareness_meta SET schema_version=?, store_id=?')
    .run(2, '11111111-1111-4111-8111-111111111111');
  expect(inspectSchemaState(source)).toBe('event-envelope-upgrade');
  source.close();

  const destination = new DatabaseSync(destinationPath);
  initDb(destination);
  const storeId = readAwarenessMeta(destination).storeId;
  destination.close();
  return {
    root,
    sourcePath,
    destinationPath,
    request: {
      sourcePath,
      destinationPath,
      sourceVersion: 'event-envelope-upgrade' as const,
      expectedStoreId: storeId,
      expectedCounts: {},
      expectedEventIds: [],
      expectedEventSequences: [],
      expectedEventHighWater: 0,
    },
  };
}

describe('migration verifier failure evidence', () => {
  it('distinguishes missing source and destination files', () => {
    const root = temporaryRoot('awareness-missing-verification-files');
    const absentSource = join(root, 'absent-source.sqlite3');
    const absentDestination = join(root, 'absent-destination.sqlite3');
    const request = {
      sourcePath: absentSource,
      destinationPath: absentDestination,
      sourceVersion: 'event-envelope-upgrade' as const,
      expectedStoreId: '11111111-1111-4111-8111-111111111111',
      expectedCounts: {}, expectedEventIds: [], expectedEventSequences: [], expectedEventHighWater: 0,
    };
    expect(() => verifyDatabaseMigration(request)).toThrow(/source no longer exists/);
    writeFileSync(absentSource, '');
    expect(() => verifyDatabaseMigration(request)).toThrow(/destination does not exist/);
  });

  it('rejects source-state, destination-state, and store identity mismatches', () => {
    const fixture = verificationFixture();
    expect(() => verifyDatabaseMigration({
      ...fixture.request, sourceVersion: 'worker-lifecycle-upgrade',
    })).toThrow(/source no longer matches worker-lifecycle-upgrade/);

    const nonCanonical = join(fixture.root, 'not-canonical.sqlite3');
    new DatabaseSync(nonCanonical).close();
    expect(() => verifyDatabaseMigration({
      ...fixture.request, destinationPath: nonCanonical,
    })).toThrow(/destination is not canonical/);

    expect(() => verifyDatabaseMigration({
      ...fixture.request, expectedStoreId: '22222222-2222-4222-8222-222222222222',
    })).toThrow(/store_id does not match/);
    expect(verifyDatabaseMigration(fixture.request)).toMatchObject({
      integrity: 'ok', eventOrderVerified: true, localGitRootReachable: null,
    });
  });

  it('rejects changed counts, replay order, and sequence high-water', () => {
    const fixture = verificationFixture();
    expect(() => verifyDatabaseMigration({
      ...fixture.request, expectedCounts: { signals: 1 },
    })).toThrow(/source row-count changed for signals/);
    expect(() => verifyDatabaseMigration({
      ...fixture.request, expectedEventIds: ['invented-event'], expectedEventSequences: [1],
    })).toThrow(/event replay order/);
    expect(() => verifyDatabaseMigration({
      ...fixture.request, expectedEventHighWater: 9,
    })).toThrow(/sequence high-water mark/);
  });
});

describe('migration primitive edge cases', () => {
  it('only accepts explicit copy-on-write predecessor states', () => {
    expect(migrationSourceVersion('legacy-renamed-predecessor')).toBe('legacy-renamed-v1');
    expect(schemaStateForMigrationVersion('legacy-renamed-v1')).toBe('legacy-renamed-predecessor');
    expect(migrationSourceVersion('event-stream-convergence-upgrade')).toBe('event-stream-convergence-upgrade');
    expect(schemaStateForMigrationVersion('event-envelope-upgrade')).toBe('event-envelope-upgrade');
    for (const unsupported of ['fresh', 'canonical', 'history-durability-upgrade', 'history-durability-path-identity-upgrade'] as const) {
      expect(() => migrationSourceVersion(unsupported)).toThrow(`does not support source state ${unsupported}`);
    }
  });

  it('preserves sequence high-water through empty destinations and never lowers it', () => {
    const source = new DatabaseSync(':memory:');
    const destination = new DatabaseSync(':memory:');
    source.exec('CREATE TABLE event_outbox(sequence INTEGER PRIMARY KEY AUTOINCREMENT)');
    destination.exec('CREATE TABLE event_outbox(sequence INTEGER PRIMARY KEY AUTOINCREMENT)');
    source.exec('INSERT INTO event_outbox DEFAULT VALUES; DELETE FROM event_outbox');
    source.exec("UPDATE sqlite_sequence SET seq=17 WHERE name='event_outbox'");
    copySequenceHighWaterMarks(source, destination);
    expect(destination.prepare("SELECT seq FROM sqlite_sequence WHERE name='event_outbox'").get()).toEqual({ seq: 17 });

    destination.exec("UPDATE sqlite_sequence SET seq=23 WHERE name='event_outbox'");
    copySequenceHighWaterMarks(source, destination);
    expect(destination.prepare("SELECT seq FROM sqlite_sequence WHERE name='event_outbox'").get()).toEqual({ seq: 23 });
    source.close();
    destination.close();

    const noSequence = new DatabaseSync(':memory:');
    const noDestinationTable = new DatabaseSync(':memory:');
    noSequence.exec('CREATE TABLE plain(id TEXT)');
    expect(() => copySequenceHighWaterMarks(noSequence, noDestinationTable)).not.toThrow();
    noSequence.close();
    noDestinationTable.close();
  });

  it('rejects missing required mappings and copies empty/source-orderable tables safely', () => {
    const source = new DatabaseSync(':memory:');
    const destination = new DatabaseSync(':memory:');
    source.exec('CREATE TABLE sample(id TEXT PRIMARY KEY)');
    destination.exec('CREATE TABLE sample(id TEXT PRIMARY KEY, required TEXT NOT NULL)');
    expect(() => copyMappedTables(source, destination)).toThrow(/lacks required column required/);
    destination.exec('DROP TABLE sample; CREATE TABLE sample(id TEXT PRIMARY KEY)');
    expect(copyMappedTables(source, destination)).toEqual({ sample: 0 });
    expect(scalar(4n, 'sample', 'id')).toBe(4n);
    expect(scalar(new Uint8Array([1, 2]), 'sample', 'id')).toEqual(new Uint8Array([1, 2]));
    source.close();
    destination.close();
  });

  it('reports every logical coordination invariant in one failure', () => {
    const db = new DatabaseSync(':memory:');
    db.exec(`
      CREATE TABLE awareness_plans(plan_id TEXT, workspace_path TEXT);
      CREATE TABLE awareness_tasks(task_id TEXT, plan_id TEXT);
      CREATE TABLE task_dependencies(task_id TEXT, depends_on_task_id TEXT);
      CREATE TABLE task_runs(run_id TEXT, task_id TEXT, agent_id TEXT, workspace_path TEXT);
      CREATE TABLE task_claims(task_id TEXT, run_id TEXT, agent_id TEXT);
      INSERT INTO awareness_plans VALUES ('plan-a','/a'), ('plan-b','/b');
      INSERT INTO awareness_tasks VALUES ('task-a','plan-a'), ('task-b','plan-b');
      INSERT INTO task_dependencies VALUES ('task-a','task-b'), ('task-b','task-b');
      INSERT INTO task_runs VALUES ('run-a','task-a','agent-a','/b');
      INSERT INTO task_claims VALUES ('task-a','run-a','agent-b');
    `);
    expect(() => assertLogicalDestination(db)).toThrow(
      /cross-plan dependency task-a->task-b; cyclic dependency at task-b; claim\/run mismatch task-a\/run-a; run workspace mismatch run-a/,
    );
    db.close();
  });
});

describe('legacy handoff conversion boundaries', () => {
  function sourceWithHandoffs(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec(`CREATE TABLE handoffs(
      handoff_id TEXT, workspace_path TEXT, agent_id TEXT, summary TEXT,
      files_json TEXT, created_at TEXT, cleared_at TEXT
    )`);
    return db;
  }

  it('rejects missing identifiers, malformed JSON, and non-string file arrays', () => {
    for (const [id, files, message] of [
      [null, '[]', /handoffs\.handoff_id is required/],
      ['handoff-bad-json', '{', /invalid JSON/],
      ['handoff-bad-files', '[1]', /files_json is invalid/],
    ] as const) {
      const db = sourceWithHandoffs();
      db.prepare('INSERT INTO handoffs VALUES (?,?,?,?,?,?,NULL)')
        .run(id, '/workspace', 'agent-a', 'continue', files, '2026-01-01T00:00:00Z');
      expect(() => legacyHandoffEvents(db)).toThrow(message);
      db.close();
    }
  });

  it('maps a cleared handoff to a resolved signal and delivery event', () => {
    const source = sourceWithHandoffs();
    source.prepare('INSERT INTO handoffs VALUES (?,?,?,?,?,?,?)').run(
      'handoff-1', '/workspace', 'agent-a', 'continue', '["src/a.ts"]',
      '2026-01-01T00:00:00Z', '2026-01-02T00:00:00Z',
    );
    const events = legacyHandoffEvents(source);
    expect(events).toMatchObject([{
      event_id: 'legacy.handoff:handoff-1', event_type: 'peer.message', retention_class: 'delivery',
    }]);
    const destination = new DatabaseSync(':memory:');
    destination.exec(`CREATE TABLE signals(
      signal_id TEXT, workspace_path TEXT, from_agent TEXT, to_agent TEXT, kind TEXT,
      subject TEXT, body TEXT, files_json TEXT, refs_json TEXT, thread_id TEXT,
      reply_to TEXT, importance INTEGER, status TEXT, resolved_at TEXT, created_at TEXT,
      expires_at TEXT NOT NULL
    )`);
    copyLegacyHandoffSignals(source, destination);
    expect(destination.prepare('SELECT signal_id,status,resolved_at FROM signals').get()).toEqual({
      signal_id: 'handoff-1', status: 'resolved', resolved_at: '2026-01-02T00:00:00Z',
    });
    source.close();
    destination.close();
  });
});

describe('schema identity and LocalGit failure boundaries', () => {
  it('fails closed for unrecognized, Agent, and foreign stores', () => {
    const empty = new DatabaseSync(':memory:');
    expect(inspectSchemaState(empty)).toBe('fresh');
    empty.exec('CREATE TABLE unrelated(id TEXT)');
    expect(() => inspectSchemaState(empty)).toThrow(/unrecognized application_id=0/);
    empty.exec(`PRAGMA application_id=${AGENT_APPLICATION_ID}`);
    expect(() => inspectSchemaState(empty)).toThrow(/Agent SQLite store/);
    empty.exec('PRAGMA application_id=7654321');
    expect(() => inspectSchemaState(empty)).toThrow(/foreign Awareness application_id/);
    empty.close();

    const inMemory = new DatabaseSync(':memory:');
    expect(() => resolveAwarenessStoreIdentity(inMemory)).toThrow(/no stable identity/);
    inMemory.close();
  });

  it('verifies reachable loose LocalGit objects and rejects invalid or missing object IDs', () => {
    const root = temporaryRoot('awareness-localgit-verification');
    const db = new DatabaseSync(':memory:');
    db.exec('CREATE TABLE local_history_operations(before_commit_oid TEXT, after_commit_oid TEXT)');
    db.exec('CREATE TABLE local_history_versions(before_oid TEXT, after_oid TEXT)');
    expect(verifyLocalGitObjects(db, undefined)).toEqual({ reachable: null, objects: null });

    db.prepare('INSERT INTO local_history_operations VALUES (?,NULL)').run('not-an-oid');
    expect(() => verifyLocalGitObjects(db, root)).toThrow(/missing or invalid: not-an-oid/);
    db.exec('DELETE FROM local_history_operations');

    const oid = 'ab'.repeat(20);
    db.prepare('INSERT INTO local_history_versions VALUES (?,?)').run(oid, oid);
    expect(() => verifyLocalGitObjects(db, root)).toThrow(`missing or invalid: ${oid}`);
    const objectPath = join(root, 'repo.git', 'objects', oid.slice(0, 2), oid.slice(2));
    mkdirSync(join(root, 'repo.git', 'objects', oid.slice(0, 2)), { recursive: true });
    writeFileSync(objectPath, 'object');
    expect(verifyLocalGitObjects(db, root)).toEqual({ reachable: true, objects: 1 });
    db.close();
  });
});

describe('copy-on-write source preservation', () => {
  it('keeps canonical source bytes intact for dry-run consolidation and migration preview', () => {
    const root = temporaryRoot('awareness-source-digest');
    const sourcePath = join(root, 'source.sqlite3');
    const destinationPath = join(root, 'destination.sqlite3');
    const source = new DatabaseSync(sourcePath);
    initDb(source);
    source.close();
    const before = digest(sourcePath);
    expect(consolidateDatabase(sourcePath, destinationPath, { dryRun: true })).toMatchObject({ dryRun: true });
    expect(existsSync(destinationPath)).toBe(false);
    expect(digest(sourcePath)).toBe(before);

    const migration = verificationFixture();
    const migrationBefore = digest(migration.sourcePath);
    const preview = previewDatabaseMigration(migration.sourcePath, join(migration.root, 'preview.sqlite3'));
    expect(preview).toMatchObject({ dryRun: true, sourceUnchanged: true, localGit: null });
    expect(digest(migration.sourcePath)).toBe(migrationBefore);
  });

  it('rejects same, missing, and occupied consolidation destinations without writing', () => {
    const root = temporaryRoot('awareness-consolidation-boundaries');
    const sourcePath = join(root, 'source.sqlite3');
    const destinationPath = join(root, 'destination.sqlite3');
    expect(() => consolidateDatabase(sourcePath, sourcePath)).toThrow(/destination must differ/);
    expect(() => consolidateDatabase(sourcePath, destinationPath)).toThrow(/source database does not exist/);
    writeFileSync(sourcePath, 'source');
    writeFileSync(destinationPath, 'destination');
    expect(() => consolidateDatabase(sourcePath, destinationPath)).toThrow(/destination already exists/);
  });
});
