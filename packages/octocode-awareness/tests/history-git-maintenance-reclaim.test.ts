import { afterEach, expect, it } from 'vitest';
import { existsSync } from 'node:fs';
import { mkdtemp, realpath, rm, utimes, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { connectDb } from '../src/db-runtime.js';
import { runAwarenessHistoryOperation } from '../src/history-api.js';
import { openHistoryGitStore, type HistoryGitStore } from '../src/history-git.js';
import { createHistoryContext } from '../src/history-store.js';
import { executeExperience } from '../src/experience.js';
import type { ExperienceArchiveReceipt } from '../src/experience-archive.js';

const roots: string[] = [];
afterEach(async () => Promise.all(roots.splice(0).map(root => rm(root, { recursive: true, force: true }))));

async function fixture(): Promise<{ root: string; store: HistoryGitStore }> {
  const root = await realpath(await mkdtemp(join(tmpdir(), 'history-reclaim-')));
  roots.push(root);
  const store = await openHistoryGitStore({ historyRoot: root, storeId: 'v1', workspaceId: 'a'.repeat(64) });
  return { root, store };
}

function objectPath(store: HistoryGitStore, oid: string): string {
  return join(store.gitdir, 'objects', oid.slice(0, 2), oid.slice(2));
}

async function age(store: HistoryGitStore, ...oids: string[]): Promise<void> {
  const old = new Date('2020-01-01T00:00:00Z');
  await Promise.all(oids.map(oid => utimes(objectPath(store, oid), old, old)));
}

it('reclaims only grace-aged unreachable objects and preserves SQLite and ref reachability', async () => {
  const { store } = await fixture();
  const retained = await store.writeBlob(Buffer.from('retained by SQLite'));
  const refBlob = await store.writeBlob(Buffer.from('retained through a ref'));
  const refTree = await store.writeTree([{ path: 'ref.txt', oid: refBlob.oid, mode: '100644' }]);
  const refCommit = await store.writeCommit({ tree: refTree, message: 'retained' });
  const ref = `refs/octocode/${'b'.repeat(64)}/before`;
  await store.publishRef(ref, refCommit);
  const orphan = await store.writeBlob(Buffer.from('old orphan'));
  const fresh = await store.writeBlob(Buffer.from('fresh orphan'));
  await store.flush();
  await age(store, retained.oid, refBlob.oid, refTree, refCommit, orphan.oid);
  await writeFile(join(store.gitdir, 'packed-refs'), `# pack-refs with: peeled fully-peeled\n${refCommit} ${ref}\n`);
  await rm(join(store.gitdir, ...ref.split('/')));

  const report = await store.inspectOrphanObjects({
    retainedOids: [retained.oid], graceMs: 3_600_000, limit: 20,
  });
  expect(report).toMatchObject({ reclaimed_objects: 0, reclaimed_bytes: 0 });
  expect(report.objects.map(object => object.oid)).toEqual([orphan.oid]);
  expect(existsSync(objectPath(store, orphan.oid))).toBe(true);

  const result = await store.reclaimOrphanObjects({
    retainedOids: [retained.oid], graceMs: 3_600_000, limit: 20,
  });

  expect(result).toMatchObject({ partial: false, reclaimed_objects: 1, reclaimed_bytes: expect.any(Number) });
  expect(result.objects.map(object => object.oid)).toEqual([orphan.oid]);
  expect(existsSync(objectPath(store, orphan.oid))).toBe(false);
  expect(await store.verifyObject(retained.oid, 'blob')).toBe(true);
  expect(await store.verifyObject(refBlob.oid, 'blob')).toBe(true);
  expect(await store.verifyObject(fresh.oid, 'blob')).toBe(true);
});

it('returns executable reclaim continuations without exceeding the requested item cap', async () => {
  const { store } = await fixture();
  const orphans = await Promise.all(['one', 'two', 'three'].map(value => store.writeBlob(Buffer.from(value))));
  await store.flush();
  await age(store, ...orphans.map(object => object.oid));

  const reclaimed: string[] = [];
  let cursor: string | undefined;
  for (let page = 0; page < 4; page++) {
    const result = await store.reclaimOrphanObjects({ retainedOids: [], graceMs: 3_600_000, limit: 1, cursor });
    expect(result.objects.length).toBeLessThanOrEqual(1);
    reclaimed.push(...result.objects.map(object => object.oid));
    if (!result.partial) break;
    expect(result.next_cursor).toBeTruthy();
    cursor = result.next_cursor!;
  }
  expect(reclaimed.sort()).toEqual(orphans.map(object => object.oid).sort());
  expect(reclaimed.every(oid => !existsSync(objectPath(store, oid)))).toBe(true);
});

it('advertises confirmed manual reclamation and refuses it while a capture is in flight', async () => {
  const workspace = await realpath(await mkdtemp(join(tmpdir(), 'history-reclaim-api-')));
  roots.push(workspace);
  const database = join(workspace, 'ledger.sqlite3');
  const db = connectDb(database);
  try {
    const status = await runAwarenessHistoryOperation(db, 'status', { workspace });
    expect(status.retention).toMatchObject({
      automatic_object_pruning: false,
      manual_object_reclamation: true,
      minimum_grace_seconds: 3_600,
    });
    db.prepare(`INSERT INTO local_history_operations
      (operation_id,workspace_path,agent_id,kind,status,request_hash,created_at,updated_at)
      VALUES ('in-flight',?,'maintenance-test','edit','capturing','hash','2020-01-01','2020-01-01')`).run(workspace);
    await expect(runAwarenessHistoryOperation(db, 'evidence', {
      workspace, action: 'reclaim', confirm: 'reclaim', grace_seconds: 3_600,
    })).rejects.toMatchObject({ code: 'HISTORY_EVIDENCE_BUSY' });
  } finally {
    db.close();
  }
});

it('reclaims an old orphan through the public History operation', async () => {
  const workspace = await realpath(await mkdtemp(join(tmpdir(), 'history-reclaim-api-')));
  roots.push(workspace);
  const database = join(workspace, 'ledger.sqlite3');
  const db = connectDb(database);
  try {
    const store = await createHistoryContext(db, workspace).store();
    const orphan = await store.writeBlob(Buffer.from('public orphan'));
    await store.flush();
    await age(store, orphan.oid);

    const result = await runAwarenessHistoryOperation(db, 'evidence', {
      workspace, action: 'reclaim', confirm: 'reclaim', grace_seconds: 3_600,
    });

    expect(result).toMatchObject({
      ok: true,
      action: 'reclaim',
      dry_run: false,
      safety: 'exclusive_metadata_handshake',
      reclaimed_objects: 1,
      next: null,
    });
    expect(existsSync(objectPath(store, orphan.oid))).toBe(false);
  } finally {
    db.close();
  }
});

it('refuses reclamation while a sealed experience archive is pending', async () => {
  const workspace = await realpath(await mkdtemp(join(tmpdir(), 'history-reclaim-pending-')));
  roots.push(workspace);
  const db = connectDb(join(workspace, 'ledger.sqlite3'));
  try {
    const binding = { workspace, actorId: 'maintenance-test', sessionId: 'session' };
    await executeExperience(db, binding, {
      action: 'record', trace_id: 'pending', event_id: 'event', kind: 'attempt',
      title: 'Pending archive', summary: 'Keep the maintenance handshake closed',
    });
    db.exec(`CREATE TRIGGER fail_archive BEFORE INSERT ON event_outbox WHEN NEW.event_type = 'experience.archive'
      BEGIN SELECT RAISE(ABORT, 'simulated pending archive'); END`);
    const sealed = await executeExperience(db, binding, { action: 'seal', trace_id: 'pending' });
    expect(sealed.archive?.status).toBe('unavailable');

    await expect(runAwarenessHistoryOperation(db, 'evidence', {
      workspace, action: 'reclaim', confirm: 'reclaim', grace_seconds: 3_600,
    })).rejects.toMatchObject({ code: 'HISTORY_EVIDENCE_BUSY' });
  } finally {
    db.close();
  }
});

it('retains archived experience objects from the durable SQLite receipt when its ref is missing', async () => {
  const workspace = await realpath(await mkdtemp(join(tmpdir(), 'history-reclaim-archive-')));
  roots.push(workspace);
  const db = connectDb(join(workspace, 'ledger.sqlite3'));
  try {
    const binding = { workspace, actorId: 'maintenance-test', sessionId: 'session' };
    await executeExperience(db, binding, {
      action: 'record', trace_id: 'trace', event_id: 'event', kind: 'verification',
      title: 'Retain archive', summary: 'Archived evidence remains reachable',
      evidence: [{ title: 'receipt', text: 'durable archive' }],
    });
    const sealed = await executeExperience(db, binding, { action: 'seal', trace_id: 'trace' });
    const receipt = sealed.archive as ExperienceArchiveReceipt;
    const store = await createHistoryContext(db, workspace).store();
    const commit = await store.readCommit(receipt.commit);
    const entries = await store.readTree(commit.tree);
    await age(store, receipt.commit, commit.tree, ...entries.map(entry => entry.oid));
    await rm(join(store.gitdir, ...receipt.ref.split('/')));

    const result = await runAwarenessHistoryOperation(db, 'evidence', {
      workspace, action: 'reclaim', confirm: 'reclaim', grace_seconds: 3_600,
    });

    expect(result).toMatchObject({ ok: true, reclaimed_objects: 0 });
    expect(await store.verifyObject(receipt.commit, 'commit')).toBe(true);
    expect(await store.verifyObject(commit.tree, 'tree')).toBe(true);
    expect(await Promise.all(entries.map(entry => store.verifyObject(entry.oid, 'blob')))
      .then(values => values.every(Boolean))).toBe(true);
  } finally {
    db.close();
  }
});
