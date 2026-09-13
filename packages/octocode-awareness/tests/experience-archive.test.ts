import { mkdtemp, mkdir, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import type { DatabaseSync } from 'node:sqlite';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { connectDb } from '../src/db-runtime.js';
import { createHistoryContext } from '../src/history-store.js';
import { executeExperience } from '../src/experience.js';
import { archiveExperience, readExperienceArchive, type ExperienceArchiveReceipt } from '../src/experience-archive.js';
import * as nativeFiles from '../src/native-files.js';

const roots: string[] = [];
const databases: DatabaseSync[] = [];
afterEach(async () => { vi.restoreAllMocks(); databases.splice(0).forEach(db => db.close());
  await Promise.all(roots.splice(0).map(root => rm(root, { recursive: true, force: true }))); });
async function fixture() {
  const root = await realpath(await mkdtemp(join(tmpdir(), 'experience-archive-'))); roots.push(root);
  const db = connectDb(join(root, 'awareness.sqlite3')); databases.push(db);
  const binding = { workspace: root, actorId: 'recorder', sessionId: 'session' };
  await executeExperience(db, binding, { action: 'record', trace_id: 'trial', event_id: 'e1',
    kind: 'verification', title: 'Retry experiment', summary: 'Observed a passing check', outcome: 'success',
    evidence: [{ title: 'Command output', text: '1 test passed\nNo failures.' }] });
  return { root, db, binding };
}
describe('private experience evidence archive', () => {
  it('roundtrips durable manifest and evidence without touching source Git', async () => {
    const { root, db, binding } = await fixture();
    await mkdir(join(root, '.git')); await writeFile(join(root, '.git', 'HEAD'), 'source-head\n');
    const sealed = await executeExperience(db, binding, { action: 'seal', trace_id: 'trial' });
    expect(sealed.archive).toEqual(expect.objectContaining({ status: 'available', durable: true }));
    const receipt = sealed.archive as ExperienceArchiveReceipt;
    const store = await createHistoryContext(db, root, { readOnly: true }).store();
    expect(await store.resolveRef(receipt.ref)).toBe(receipt.commit);
    const entries = await store.readTree((await store.readCommit(receipt.commit)).tree);
    const evidence = entries.find(entry => entry.path === 'evidence/0/0.txt')!;
    expect(Buffer.from(await store.readBlob(evidence.oid)).toString()).toBe('1 test passed\nNo failures.');
    expect((await store.inspectOrphanObjects({ retainedOids: [], graceMs: 0, limit: 20 })).objects).toEqual([]);
    const archived = await executeExperience(db, binding, { action: 'get', trace_id: 'trial', source: 'archive' });
    const journal = await executeExperience(db, binding, { action: 'get', trace_id: 'trial' });
    expect(archived.events).toEqual(journal.events);
    expect(await readFile(join(root, '.git', 'HEAD'), 'utf8')).toBe('source-head\n');
    expect((await executeExperience(db, binding, { action: 'recover' })).traces).toEqual([]);
    await expect(store.resolveRef('refs/octocode/experiences/../outside')).rejects.toThrow(/invalid history ref/i);
  });
  it('converges concurrent sealers and rejects new events after freeze', async () => {
    const { root, db, binding } = await fixture();
    const peer = connectDb(join(root, 'awareness.sqlite3')); databases.push(peer);
    const results = await Promise.all([
      executeExperience(db, binding, { action: 'seal', trace_id: 'trial' }),
      executeExperience(peer, { ...binding, actorId: 'peer' }, { action: 'seal', trace_id: 'trial' }),
    ]);
    expect(results.map(result => result.archive?.status)).toEqual(['available', 'available']);
    expect(results[0]!.archive).toEqual(results[1]!.archive);
    expect(db.prepare("SELECT count(*) AS n FROM event_outbox WHERE event_type = 'experience.archive'").get()?.n).toBe(1);
    await expect(executeExperience(db, binding, { action: 'record', trace_id: 'trial', event_id: 'late',
      title: 'Late', summary: 'Late write', kind: 'attempt' })).rejects.toThrow(/sealed/i);
  });
  it('repairs the durable-ref-before-SQL crash window without claiming a receipt on SQL failure', async () => {
    const { db, binding } = await fixture();
    db.exec(`CREATE TRIGGER fail_archive BEFORE INSERT ON event_outbox WHEN NEW.event_type = 'experience.archive'
      BEGIN SELECT RAISE(ABORT, 'simulated archive receipt failure'); END`);
    const failed = await executeExperience(db, binding, { action: 'seal', trace_id: 'trial' });
    expect(failed.archive?.durable).toBe(false);
    expect(db.prepare("SELECT count(*) AS n FROM event_outbox WHERE event_type = 'experience.archive'").get()?.n).toBe(0);
    expect((await executeExperience(db, binding, { action: 'recover' })).traces).toHaveLength(1);
    db.exec('DROP TRIGGER fail_archive');
    const repaired = await executeExperience(db, binding, { action: 'seal', trace_id: 'trial' });
    expect(repaired.archive?.durable).toBe(true);
    expect((await executeExperience(db, binding, { action: 'get', trace_id: 'trial', source: 'archive' })).events).toHaveLength(1);
  });
  it('does not accept a failed flush and repairs an existing ref on retry', async () => {
    const { root, db, binding } = await fixture();
    const events = (await executeExperience(db, binding, { action: 'get', trace_id: 'trial' })).events!;
    const store = await createHistoryContext(db, root).store();
    const flush = vi.spyOn(store, 'flush').mockResolvedValueOnce({ durable: false, warnings: ['fixture failure'] });
    await expect(archiveExperience(db, root, 'trial', events, store)).rejects.toThrow(/NOT_DURABLE/);
    const receipt = await archiveExperience(db, root, 'trial', events, store);
    expect(flush).toHaveBeenCalledTimes(2);
    expect(await readExperienceArchive(db, root, 'trial', receipt, store)).toEqual(events);
    await expect(readExperienceArchive(db, '/different-workspace', 'trial', receipt, store)).rejects.toThrow(/identity|digest/);
  });
  it('keeps an offline native archive explicitly unavailable and repairs it after native returns', async () => {
    const { db, binding } = await fixture();
    const native = vi.spyOn(nativeFiles, 'loadNativeFiles').mockRejectedValueOnce(new Error('native unavailable'));
    const sealed = await executeExperience(db, binding, { action: 'seal', trace_id: 'trial' });
    expect(sealed.archive).toEqual(expect.objectContaining({ status: 'unavailable', durable: false }));
    native.mockRestore();
    expect((await executeExperience(db, binding, { action: 'get', trace_id: 'trial' })).events).toHaveLength(1);
    expect((await executeExperience(db, binding, { action: 'seal', trace_id: 'trial' })).archive?.status).toBe('available');
  });
  it('detects corrupt archive evidence and retains readable SQLite events', async () => {
    const { root, db, binding } = await fixture();
    const sealed = await executeExperience(db, binding, { action: 'seal', trace_id: 'trial' });
    const receipt = sealed.archive as ExperienceArchiveReceipt;
    const store = await createHistoryContext(db, root).store();
    const entries = await store.readTree((await store.readCommit(receipt.commit)).tree);
    const manifest = entries.find(entry => entry.path === 'manifest.json')!;
    await writeFile(join(store.gitdir, 'objects', manifest.oid.slice(0, 2), manifest.oid.slice(2)), 'corrupt');
    expect((await executeExperience(db, binding, { action: 'get', trace_id: 'trial', source: 'archive' })).archive?.status).toBe('unavailable');
    expect((await executeExperience(db, binding, { action: 'get', trace_id: 'trial' })).events).toHaveLength(1);
  });
});
