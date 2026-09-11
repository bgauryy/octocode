import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { afterEach, describe, expect, it } from 'vitest';
import { applyDatabaseMigration, previewDatabaseMigration, verifyDatabaseMigration } from '../src/db-consolidation.js';
import { inspectSchemaState, readAwarenessMeta, type SchemaState } from '../src/db-introspection.js';
import { FTS_SCHEMA_DDL, SCHEMA_DDL, SCHEMA_INDEX_DDL } from '../src/db-schema.js';
import { EVENT_OUTBOX_V1_DDL, EVENT_OUTBOX_V1_INDEX_DDL } from '../src/db-continuity-schema.js';
import { LEGACY_RENAMED_V1_SCHEMA_DDL, PREDECESSOR_EVENT_RELATIONS_DDL, PREDECESSOR_REFINEMENTS_DDL } from '../src/db-predecessor-schema.js';
import { WORKER_LIFECYCLE_DDL } from '../src/db-worker-schema.js';
import { captureHistory } from '../src/history-capture.js';
import { previewHistoryRestore } from '../src/history-restore.js';
import { createHistoryContext, historyHash, historyStoragePaths } from '../src/history-store.js';
import { AWARENESS_APPLICATION_ID, AWARENESS_MIGRATABLE_SCHEMA_VERSIONS, AWARENESS_SCHEMA_VERSION } from '../src/storage-scope.js';
import { initDb } from '../src/db-init.js';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });
const digest = (path: string) => createHash('sha256').update(readFileSync(path)).digest('hex');

function legacyFixture() {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-database-migration-')));
  roots.push(root);
  const workspace = join(root, 'workspace');
  mkdirSync(workspace);
  const sourcePath = join(root, 'legacy.sqlite3');
  const destinationPath = join(root, 'canonical.sqlite3');
  const db = new DatabaseSync(sourcePath);
  db.exec(LEGACY_RENAMED_V1_SCHEMA_DDL);
  db.exec(FTS_SCHEMA_DDL);
  db.exec(`PRAGMA application_id=${AWARENESS_APPLICATION_ID}`);
  return { root, workspace, sourcePath, destinationPath, db };
}

interface PredecessorOptions {
  eventV1?: boolean;
  worker?: boolean;
  missingMeta?: boolean;
  missingDurability?: boolean;
  refinements?: boolean;
  schemaVersion?: number;
}

function exactPredecessorFixture(options: PredecessorOptions) {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-exact-migration-')));
  roots.push(root);
  const workspace = join(root, 'workspace');
  mkdirSync(workspace);
  const sourcePath = join(root, 'source.sqlite3');
  const destinationPath = join(root, 'destination.sqlite3');
  const db = new DatabaseSync(sourcePath);
  db.exec(SCHEMA_DDL);
  db.exec(SCHEMA_INDEX_DDL);
  if (options.refinements) db.exec(PREDECESSOR_REFINEMENTS_DDL);
  if (options.eventV1) {
    db.exec('DROP TABLE event_outbox');
    db.exec(EVENT_OUTBOX_V1_DDL);
    db.exec(EVENT_OUTBOX_V1_INDEX_DDL);
  }
  if (options.worker) db.exec(WORKER_LIFECYCLE_DDL);
  if (options.missingMeta) db.exec('DROP TABLE awareness_meta');
  else {
    const schemaVersion = options.schemaVersion ?? (options.eventV1 ? 2 : AWARENESS_SCHEMA_VERSION);
    db.prepare(`INSERT INTO awareness_meta
      (application_id,schema_version,store_id,created_at,last_migrated_at)
      VALUES (?,?,?,?,NULL)`).run(AWARENESS_APPLICATION_ID, schemaVersion, '11111111-1111-4111-8111-111111111111', '2026-01-01T00:00:00Z');
  }
  if (options.missingDurability) db.exec('DROP TABLE local_history_durability');
  db.prepare(`INSERT INTO event_outbox
    (event_id,workspace_path,event_type,aggregate_kind,aggregate_id,aggregate_revision,actor_json,provenance_json,payload_json,session_id,correlation_id,created_at,expires_at)
    VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)`).run('event-existing', workspace, 'plan.updated', 'plan', 'plan-1', '7',
      '{"kind":"agent","id":"agent-1"}', '{"source":"awareness","trust":"verified"}', '{"value":1}',
      'session-1', 'correlation-1', '2026-01-01T00:00:01Z', null);
  db.prepare("UPDATE sqlite_sequence SET seq=9 WHERE name='event_outbox'").run();
  if (options.worker) {
    const insert = db.prepare(`INSERT INTO worker_lifecycle_events
      (packet_id,workspace_path,session_id,worker_id,correlation_id,event_type,redaction,created_at,payload_json,recorded_at)
      VALUES (?,?,?,?,?,?,?,?,?,?)`);
    insert.run('packet-2', workspace, 'session-1', 'worker-1', 'correlation-2', 'completed', 'internal',
      '2026-01-01T00:00:03Z', '{"result":"second"}', '2026-01-01T00:00:04Z');
    insert.run('packet-1', workspace, 'session-1', 'worker-1', 'correlation-1', 'started', 'public',
      '2026-01-01T00:00:01Z', '{"task":"first"}', '2026-01-01T00:00:02Z');
  }
  db.exec(`PRAGMA application_id=${AWARENESS_APPLICATION_ID}`);
  return { root, workspace, sourcePath, destinationPath, db };
}

describe('legacy-renamed-v1 copy-on-write migration', () => {
  it('copies representative related rows, persists the compatible identity, and retains LocalGit mapping', () => {
    const fixture = legacyFixture();
    const { db, workspace, sourcePath, destinationPath } = fixture;
    const createdAt = '2026-01-02T03:04:05Z';
    db.prepare(`INSERT INTO agents
      (agent_id, agent_name, workspace_path, artifact, context, registered_at, last_seen_at)
      VALUES ('agent-1', 'Agent', ?, NULL, NULL, ?, ?)`).run(workspace, createdAt, createdAt);
    db.prepare(`INSERT INTO plans
      (plan_id,name,objective,lead_agent_id,status,workspace_path,artifact,doc_dir,created_at,updated_at)
      VALUES ('plan-1','Migration','Preserve rows','agent-1','ACTIVE',?,NULL,'.octocode/plans/plan-1',?,?)`)
      .run(workspace, createdAt, createdAt);
    db.prepare(`INSERT INTO tasks
      (task_id,plan_id,title,reasoning,acceptance_criteria,status,priority,created_by,created_at,updated_at,completed_at)
      VALUES ('task-1','plan-1','Copy','Exact values','Rows survive','IN_PROGRESS',1,'agent-1',?,?,NULL)`)
      .run(createdAt, createdAt);
    db.prepare(`INSERT INTO task_runs
      (run_id,task_id,origin,agent_id,session_id,rationale,test_plan,context_ref,status,workspace_path,artifact,created_at,updated_at)
      VALUES ('run-1','task-1','TASK','agent-1',NULL,'migration','test',NULL,'ACTIVE',?,NULL,?,?)`)
      .run(workspace, createdAt, createdAt);
    db.prepare(`INSERT INTO run_files
      (run_id,file_path,reason_override,source,started_at,heartbeat_at,expires_at,ended_at)
      VALUES ('run-1','src/a.ts',NULL,'EXPLICIT',?,?,?,NULL)`)
      .run(createdAt, createdAt, '2026-01-02T03:14:05Z');
    db.prepare(`INSERT INTO task_events
      (event_id,task_id,run_id,agent_id,event_type,message,created_at)
      VALUES ('task-event-1','task-1','run-1','agent-1','CLAIMED','claimed',?)`).run(createdAt);
    db.prepare(`INSERT INTO run_log
      (event_id,run_id,agent_id,event_type,message,created_at)
      VALUES ('run-event-1','run-1','agent-1','STARTED','started',?)`).run(createdAt);
    db.prepare(`INSERT INTO edit_log
      (edit_id,session_id,run_id,agent_id,file_path,operation,workspace_path,created_at)
      VALUES ('edit-1',NULL,'run-1','agent-1','src/a.ts','update',?,?)`).run(workspace, createdAt);
    db.prepare(`INSERT INTO harness_log
      (harness_id,session_id,agent_id,workspace_path,event_type,payload_json,memory_id,run_id,created_at)
      VALUES ('harness-1',NULL,'agent-1',?,'capture','{"ok":true}',NULL,'run-1',?)`).run(workspace, createdAt);
    db.close();
    const storeId = historyHash(realpathSync(sourcePath));
    const localGitRoot = join(workspace, '.octocode', '.localGit', storeId, 'awareness-v1', historyHash(workspace));
    mkdirSync(localGitRoot, { recursive: true });
    writeFileSync(join(localGitRoot, 'sentinel'), 'retained');
    const sourceBefore = digest(sourcePath);

    const report = applyDatabaseMigration(sourcePath, destinationPath, { workspace });

    expect(digest(sourcePath)).toBe(sourceBefore);
    expect(readFileSync(join(localGitRoot, 'sentinel'), 'utf8')).toBe('retained');
    expect(report).toMatchObject({
      sourceVersion: 'legacy-renamed-v1', sourceUnchanged: true, published: true,
      copiedTables: { agents: 1, plans: 1, tasks: 1, task_runs: 1, run_files: 1 },
      localGit: { storeId, sourceRoot: localGitRoot, retained: true },
      verification: { integrity: 'ok', foreignKeyViolations: 0, tableCountsVerified: 23 },
      next: { cutover: { requiresApproval: true, databasePath: destinationPath } },
    });
    const destination = new DatabaseSync(destinationPath, { readOnly: true });
    try {
      expect(readAwarenessMeta(destination)).toMatchObject({ storeId, schemaVersion: AWARENESS_SCHEMA_VERSION });
      expect(destination.prepare('SELECT agent_id,status,metadata_json,registered_at FROM awareness_agents').get())
        .toEqual({ agent_id: 'agent-1', status: 'ACTIVE', metadata_json: '{}', registered_at: createdAt });
      expect(destination.prepare('SELECT task_id,source_step_key,check_command,created_at FROM awareness_tasks').get())
        .toEqual({ task_id: 'task-1', source_step_key: null, check_command: null, created_at: createdAt });
      expect(destination.prepare('SELECT run_id,file_path,started_at FROM run_files').get())
        .toEqual({ run_id: 'run-1', file_path: 'src/a.ts', started_at: createdAt });
      expect(destination.prepare('SELECT event_id,event_type,retention_class FROM event_outbox ORDER BY sequence').all())
        .toEqual([
          { event_id: 'legacy.task:task-event-1', event_type: 'task.claimed', retention_class: 'delivery' },
          { event_id: 'legacy.run:run-event-1', event_type: 'run.started', retention_class: 'delivery' },
          { event_id: 'legacy.edit:edit-1', event_type: 'edit.update', retention_class: 'operational' },
          { event_id: 'legacy.harness:harness-1', event_type: 'harness.capture', retention_class: 'operational' },
        ]);
      expect(destination.prepare(`SELECT name FROM sqlite_schema WHERE type = 'table'
        AND name IN ('task_events','run_log','edit_log','harness_log','handoffs','refinements')`).all()).toEqual([]);
      expect(historyStoragePaths(createHistoryContext(destination, workspace))).toMatchObject({
        root: localGitRoot, layout: 'awareness-v1', current_path_preserved: true,
      });
    } finally { destination.close(); }
    expect(verifyDatabaseMigration(report.next.verify)).toMatchObject({ integrity: 'ok', foreignKeyViolations: 0 });
  });

  it('migrates each classifiable refinement to its canonical owner and drops the legacy table', () => {
    const { db, sourcePath, destinationPath, workspace } = legacyFixture();
    const insert = db.prepare(`INSERT INTO refinements
      (refinement_id,agent_id,workspace_path,artifact,repo,ref,files_json,reasoning,remember,quality,state,created_at,updated_at)
      VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)`);
    const createdAt = '2026-01-02T03:04:05Z';
    insert.run('ref-handoff', 'agent-1', workspace, null, null, null, '[]', 'Continue the migration', '', 'handoff', 'open', createdAt, createdAt);
    insert.run('ref-work', 'agent-2', workspace, null, null, null, '["src/a.ts"]', 'Fix the migration', '', 'good', 'ongoing', createdAt, createdAt);
    insert.run('ref-memory', 'agent-3', workspace, null, null, null, '[]', 'Migration result', 'Schema fingerprints prevent drift', 'good', 'done', createdAt, createdAt);
    db.close();

    const report = applyDatabaseMigration(sourcePath, destinationPath, { workspace });
    expect(report.omissions).toEqual([]);
    const destination = new DatabaseSync(destinationPath, { readOnly: true });
    try {
      expect(destination.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name='refinements'").get()).toBeUndefined();
      expect(destination.prepare("SELECT signal_id,kind,subject FROM signals WHERE signal_id='migrated.refinement:ref-handoff'").get())
        .toEqual({ signal_id: 'migrated.refinement:ref-handoff', kind: 'handoff', subject: 'Migrated refinement ref-handoff' });
      expect(destination.prepare("SELECT run_id,origin,status,context_ref FROM task_runs WHERE run_id='migrated.refinement:ref-work'").get())
        .toEqual({ run_id: 'migrated.refinement:ref-work', origin: 'WORK', status: 'ACTIVE', context_ref: 'legacy-refinement:ref-work' });
      expect(destination.prepare("SELECT run_id,file_path,source FROM run_files WHERE run_id='migrated.refinement:ref-work'").get())
        .toEqual({ run_id: 'migrated.refinement:ref-work', file_path: 'src/a.ts', source: 'EXPLICIT' });
      expect(destination.prepare("SELECT memory_id,task_context,observation,source_digest,verified_at FROM awareness_memories WHERE memory_id='migrated.refinement:ref-memory'").get())
        .toEqual({ memory_id: 'migrated.refinement:ref-memory', task_context: 'Migration result', observation: 'Schema fingerprints prevent drift', source_digest: 'legacy-refinement:ref-memory', verified_at: createdAt });
    } finally { destination.close(); }
  });

  it('reports every ambiguous refinement before creating a destination', () => {
    const { db, sourcePath, destinationPath, workspace } = legacyFixture();
    const createdAt = '2026-01-02T03:04:05Z';
    const insert = db.prepare(`INSERT INTO refinements
      (refinement_id,agent_id,workspace_path,artifact,repo,ref,files_json,reasoning,remember,quality,state,created_at,updated_at)
      VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)`);
    insert.run('ref-mixed', 'agent-1', workspace, null, null, null, '["src/a.ts"]', 'Mixed row', 'Also remember this', 'good', 'open', createdAt, createdAt);
    insert.run('ref-instructions', 'agent-1', workspace, null, null, null, '[]', 'Change system policy', '', 'instructions', 'open', createdAt, createdAt);
    insert.run('ref-insufficient', 'agent-1', workspace, null, null, null, '[]', 'No destination evidence', '', 'bad', 'open', createdAt, createdAt);
    insert.run('ref-invalid-json', 'agent-1', workspace, null, null, null, '{', 'Broken files', '', 'good', 'open', createdAt, createdAt);
    db.close();
    const before = digest(sourcePath);

    let message = '';
    try { previewDatabaseMigration(sourcePath, destinationPath, { workspace }); }
    catch (error) { message = (error as Error).message; }
    expect(message).toMatch(/^AMBIGUOUS_REFINEMENT:/);
    expect(message).toContain('ref-mixed MIXED_ACTION_AND_MEMORY_PAYLOAD');
    expect(message).toContain('ref-instructions INSTRUCTIONS_REQUIRE_AUTHORITY_DESTINATION');
    expect(message).toContain('ref-insufficient INSUFFICIENT_DESTINATION_EVIDENCE');
    expect(message).toContain('ref-invalid-json INVALID_FILES_JSON');
    expect(digest(sourcePath)).toBe(before);
    expect(existsSync(destinationPath)).toBe(false);
  });

  it('rejects a near-match with a changed legacy index fingerprint', () => {
    const { db, sourcePath, destinationPath, workspace } = legacyFixture();
    db.exec('DROP INDEX idx_agents_scope');
    db.exec('CREATE INDEX idx_agents_scope ON agents(artifact, workspace_path)');
    db.close();
    const before = digest(sourcePath);

    expect(() => previewDatabaseMigration(sourcePath, destinationPath, { workspace })).toThrow(/unrecognized|fingerprint/i);
    expect(digest(sourcePath)).toBe(before);
    expect(existsSync(destinationPath)).toBe(false);
  });

  it('rejects unsupported source shapes and leaves no partial destination', () => {
    const { db, sourcePath, destinationPath, workspace } = legacyFixture();
    db.exec('ALTER TABLE agents ADD COLUMN unknown TEXT');
    db.close();
    const before = digest(sourcePath);
    expect(() => applyDatabaseMigration(sourcePath, destinationPath, { workspace })).toThrow(/unrecognized|unsupported|fingerprint/i);
    expect(digest(sourcePath)).toBe(before);
    expect(existsSync(destinationPath)).toBe(false);
  });

  it('refuses an existing partial destination without changing either file', () => {
    const { db, sourcePath, destinationPath, workspace } = legacyFixture();
    db.close();
    writeFileSync(destinationPath, 'partial destination');
    const sourceBefore = digest(sourcePath);
    const destinationBefore = digest(destinationPath);
    expect(() => applyDatabaseMigration(sourcePath, destinationPath, { workspace })).toThrow(/destination already exists/);
    expect(digest(sourcePath)).toBe(sourceBefore);
    expect(digest(destinationPath)).toBe(destinationBefore);
  });
});

describe('event-stream convergence predecessor migration', () => {
  it('preserves the source and maps one legacy handoff to a signal and typed peer event', () => {
    const root = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-stream-predecessor-')));
    roots.push(root);
    const workspace = join(root, 'workspace');
    mkdirSync(workspace);
    const sourcePath = join(root, 'source.sqlite3');
    const destinationPath = join(root, 'destination.sqlite3');
    const source = new DatabaseSync(sourcePath);
    initDb(source);
    source.exec(PREDECESSOR_EVENT_RELATIONS_DDL);
    source.prepare(`INSERT INTO handoffs(handoff_id,workspace_path,agent_id,summary,files_json,created_at)
      VALUES ('handoff-1',?,'agent-a','Resume parser work','["src/parser.ts"]','2026-01-01T00:00:00Z')`).run(workspace);
    expect(inspectSchemaState(source)).toBe('event-stream-convergence-upgrade');
    source.close();
    const before = digest(sourcePath);

    const preview = previewDatabaseMigration(sourcePath, destinationPath);
    expect(preview).toMatchObject({ sourceVersion: 'event-stream-convergence-upgrade', sourceUnchanged: true });
    expect(preview.transformations).toContainEqual(expect.objectContaining(
      { source: 'handoffs', destination: 'event_outbox', rows: 1 }));
    const report = applyDatabaseMigration(sourcePath, destinationPath);
    expect(digest(sourcePath)).toBe(before);
    expect(report.sourceVersion).toBe('event-stream-convergence-upgrade');
    const destination = new DatabaseSync(destinationPath, { readOnly: true });
    expect(destination.prepare(`SELECT signal_id,kind,subject,status FROM signals WHERE signal_id='handoff-1'`).get())
      .toEqual({ signal_id: 'handoff-1', kind: 'handoff', subject: 'Resume parser work', status: 'open' });
    expect(destination.prepare(`SELECT event_type,aggregate_id,retention_class FROM event_outbox
      WHERE event_id='legacy.handoff:handoff-1'`).get())
      .toEqual({ event_type: 'peer.message', aggregate_id: 'handoff-1', retention_class: 'delivery' });
    expect(destination.prepare(`SELECT name FROM sqlite_schema WHERE type='table'
      AND name IN ('handoffs','task_events','run_log','edit_log','harness_log')`).all()).toEqual([]);
    destination.close();
  });
});

describe('exact canonical predecessor copy-on-write migration', () => {
  const cases: ReadonlyArray<{ state: SchemaState; options: PredecessorOptions; label?: string }> = [
    { state: 'schema-generation-upgrade', options: { schemaVersion: AWARENESS_MIGRATABLE_SCHEMA_VERSIONS[0] } },
    { state: 'refinements-upgrade', options: { refinements: true } },
    { state: 'canonical-path-identity', options: { missingMeta: true } },
    { state: 'event-envelope-upgrade', options: { eventV1: true } },
    { state: 'event-envelope-upgrade', label: 'with refinements', options: { eventV1: true, refinements: true } },
    { state: 'event-envelope-path-identity-upgrade', options: { eventV1: true, missingMeta: true } },
    { state: 'event-envelope-history-durability-upgrade', options: { eventV1: true, missingDurability: true } },
    { state: 'event-envelope-history-durability-path-identity-upgrade', options: { eventV1: true, missingMeta: true, missingDurability: true } },
    { state: 'event-envelope-upgrade', label: 'with worker projection', options: { eventV1: true, worker: true } },
    { state: 'event-envelope-path-identity-upgrade', label: 'with worker projection', options: { eventV1: true, worker: true, missingMeta: true } },
    { state: 'event-envelope-history-durability-upgrade', label: 'with worker projection', options: { eventV1: true, worker: true, missingDurability: true } },
    { state: 'event-envelope-history-durability-path-identity-upgrade', label: 'with worker projection', options: { eventV1: true, worker: true, missingMeta: true, missingDurability: true } },
    { state: 'worker-lifecycle-upgrade', options: { worker: true } },
    { state: 'worker-lifecycle-upgrade', label: 'with refinements', options: { worker: true, refinements: true } },
    { state: 'worker-lifecycle-path-identity-upgrade', options: { worker: true, missingMeta: true } },
    { state: 'worker-lifecycle-history-durability-upgrade', options: { worker: true, missingDurability: true } },
    { state: 'worker-lifecycle-history-durability-path-identity-upgrade', options: { worker: true, missingMeta: true, missingDurability: true } },
  ];

  it.each([2, AWARENESS_SCHEMA_VERSION + 1])('rejects unsupported schema generation %i with the current fingerprint', (schemaVersion) => {
    const { db } = exactPredecessorFixture({ schemaVersion });
    expect(() => inspectSchemaState(db)).toThrow(`unsupported Awareness schema_version ${schemaVersion}`);
    db.close();
  });

  for (const testCase of cases) {
    it(`previews and applies ${testCase.state}${testCase.label ? ` ${testCase.label}` : ''} without mutating the source`, () => {
      const fixture = exactPredecessorFixture(testCase.options);
      const { db, workspace, sourcePath, destinationPath } = fixture;
      const sourceBefore = digest(sourcePath);
      expect(inspectSchemaState(db)).toBe(testCase.state);
      if (testCase.state === 'schema-generation-upgrade') {
        expect(() => initDb(db)).toThrow(/recognized schema-generation-upgrade Awareness store/);
        expect(digest(sourcePath)).toBe(sourceBefore);
      }
      db.close();

      const preview = previewDatabaseMigration(sourcePath, destinationPath, { workspace });
      expect(preview).toMatchObject({ dryRun: true, sourceVersion: testCase.state, sourceUnchanged: true });
      expect(digest(sourcePath)).toBe(sourceBefore);
      const report = applyDatabaseMigration(sourcePath, destinationPath, { workspace });

      expect(digest(sourcePath)).toBe(sourceBefore);
      expect(report.sourceVersion).toBe(testCase.state);
      expect(report.next.cutover).toMatchObject({ requiresApproval: true, retainedSourcePath: sourcePath });
      const destination = new DatabaseSync(destinationPath, { readOnly: true });
      try {
        expect(inspectSchemaState(destination)).toBe('canonical');
        const expectedStoreId = testCase.options.missingMeta
          ? historyHash(realpathSync(sourcePath))
          : '11111111-1111-4111-8111-111111111111';
        expect(readAwarenessMeta(destination)).toMatchObject({
          storeId: expectedStoreId,
          schemaVersion: AWARENESS_SCHEMA_VERSION,
          ...(testCase.options.missingMeta ? {} : { createdAt: '2026-01-01T00:00:00Z' }),
        });
        const events = destination.prepare(`SELECT sequence,event_id,event_type,schema_version,retention_class,payload_json
          FROM event_outbox ORDER BY sequence`).all() as Array<Record<string, unknown>>;
        expect(events[0]).toMatchObject({ sequence: 1, event_id: 'event-existing', event_type: 'plan.updated', schema_version: 1, retention_class: 'delivery' });
        if (testCase.options.worker) {
          expect(events.map(({ event_id }) => event_id)).toEqual([
            'event-existing', 'worker_lifecycle:packet-2', 'worker_lifecycle:packet-1',
          ]);
          expect(events.map(({ sequence }) => sequence)).toEqual([1, 10, 11]);
          expect(events[1]).toMatchObject({ event_type: 'worker.lifecycle', schema_version: 1, retention_class: 'operational' });
          expect(JSON.parse(String(events[1]!.payload_json))).toEqual({
            event_type: 'completed', redaction: 'internal', payload: { result: 'second' }, recorded_at: '2026-01-01T00:00:04Z',
          });
          expect(destination.prepare("SELECT name FROM sqlite_schema WHERE name='worker_lifecycle_events'").get()).toBeUndefined();
        } else expect(events).toHaveLength(1);
      } finally { destination.close(); }
      expect(verifyDatabaseMigration(report.next.verify)).toMatchObject({
        integrity: 'ok', foreignKeyViolations: 0, eventOrderVerified: true,
      });
    });
  }

  it('keeps captured LocalGit objects reachable and supports restore preview after path-identity migration', async () => {
    const fixture = exactPredecessorFixture({ missingMeta: true });
    const { db, workspace, sourcePath, destinationPath } = fixture;
    mkdirSync(join(workspace, 'src'));
    writeFileSync(join(workspace, 'src/a.ts'), 'captured contents');
    const sourceContext = createHistoryContext(db, workspace);
    const captured = await captureHistory(sourceContext, {
      workspace, agent_id: 'agent-1', operation_id: 'checkpoint-1', file: ['src/a.ts'], label: 'before migration',
    });
    const sourceStorage = historyStoragePaths(sourceContext)!;
    db.close();

    const report = applyDatabaseMigration(sourcePath, destinationPath, { workspace });
    const destination = new DatabaseSync(destinationPath);
    try {
      const destinationContext = createHistoryContext(destination, workspace);
      expect(historyStoragePaths(destinationContext)).toMatchObject({
        store_id: historyHash(realpathSync(sourcePath)), root: sourceStorage.root, git_dir: sourceStorage.git_dir,
        layout: 'awareness-v1', current_path_preserved: true,
      });
      const store = await destinationContext.store();
      expect(await store.verifyObject(captured.operation.after_commit_oid!, 'commit')).toBe(true);
      const restore = await previewHistoryRestore(destinationContext, {
        workspace, agent_id: 'agent-1', operation_id: 'checkpoint-1', side: 'after',
      });
      expect(restore).toMatchObject({ ok: true, status: 'ready', files: ['src/a.ts'], changed_files: 0 });
      expect(report.verification).toMatchObject({ localGitRootReachable: true });
    } finally { destination.close(); }
  });
});
