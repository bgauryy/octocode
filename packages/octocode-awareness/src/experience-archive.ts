import type { DatabaseSync } from 'node:sqlite';
import { resolve } from 'node:path';
import { createHistoryContext } from './history-store.js';
import { loadNativeFiles } from './native-files.js';
import type { HistoryGitStore, HistoryTreeEntry } from './history-git.js';
import { experienceEventSchema, MAX_EXPERIENCE_EVENTS, type ExperienceEvent } from './experience-contract.js';
import { experienceHash } from './experience-journal.js';

export interface ExperienceArchiveReceipt { status: 'available'; durable: true; ref: string; commit: string; digest: string }
type ArchivedEvent = Omit<ExperienceEvent, 'evidence'> & { evidence: Array<{ title: string; oid: string; bytes: number }> };
interface Manifest { version: 1; workspace: string; trace_id: string; digest: string; events: ArchivedEvent[] }
const MAX_MANIFEST_BYTES = 2 * 1024 * 1024;

/** Git evidence never advances SQLite truth; the caller publishes the receipt after this flush. */
export async function archiveExperience(db: DatabaseSync, workspace: string, trace: string,
  events: ExperienceEvent[], storeOverride?: HistoryGitStore): Promise<ExperienceArchiveReceipt> {
  const store = storeOverride ?? await createHistoryContext(db, workspace).store();
  const digest = experienceHash({ workspace, trace_id: trace, events });
  const ref = `refs/octocode/experiences/${digest}`;
  const manifest: Manifest = { version: 1, workspace, trace_id: trace, digest, events: [] };
  const entries: HistoryTreeEntry[] = [];
  for (const [index, event] of events.entries()) {
    const archived: ArchivedEvent = { ...event, evidence: [] };
    for (const [ordinal, evidence] of event.evidence.entries()) {
      const blob = await store.writeBlob(Buffer.from(evidence.text));
      entries.push({ path: `evidence/${index}/${ordinal}.txt`, oid: blob.oid, mode: '100644' });
      archived.evidence.push({ title: evidence.title, oid: blob.oid, bytes: blob.size });
    }
    manifest.events.push(archived);
  }
  const manifestBlob = await store.writeBlob(Buffer.from(JSON.stringify(manifest)));
  entries.push({ path: 'manifest.json', oid: manifestBlob.oid, mode: '100644' });
  const tree = await store.writeTree(entries);
  // Stable time and message make retries and concurrent sealers converge on one commit.
  const commit = await store.writeCommit({ tree, message: `Experience ${digest}`, timestampMs: Date.parse(events[0]!.created_at) });
  const existing = await store.resolveRef(ref);
  if (existing && existing !== commit) throw new Error('EXPERIENCE_ARCHIVE_CONFLICT: immutable ref differs from evidence');
  if (!existing) {
    try { await store.publishRef(ref, commit); }
    catch (error) { if (await store.resolveRef(ref) !== commit) throw error; }
  }
  // A retry verifies an existing ref too. Rewriting immutable objects above enrolls all bytes in flush.
  const durability = await store.flush();
  if (!durability.durable) throw new Error('EXPERIENCE_ARCHIVE_NOT_DURABLE: native flush did not guarantee persistence');
  // Existing immutable refs are not in a newly opened store's pending flush set.
  // Flush that exact ref as well so a retry repairs the ref-before-flush crash window.
  const refDurability = await (await loadNativeFiles()).flushFile(resolve(store.gitdir, ref));
  if (!refDurability.durable) throw new Error('EXPERIENCE_ARCHIVE_NOT_DURABLE: retained ref is not durable');
  return { status: 'available', durable: true, ref, commit, digest };
}

export async function readExperienceArchive(db: DatabaseSync, workspace: string, trace: string,
  receipt: ExperienceArchiveReceipt, storeOverride?: HistoryGitStore): Promise<ExperienceEvent[]> {
  const store = storeOverride ?? await createHistoryContext(db, workspace, { readOnly: true }).store();
  if (receipt.ref !== `refs/octocode/experiences/${receipt.digest}` || await store.resolveRef(receipt.ref) !== receipt.commit) {
    throw new Error('EXPERIENCE_ARCHIVE_INVALID: retained ref does not match receipt');
  }
  const commit = await store.readCommit(receipt.commit);
  const entries = await store.readTree(commit.tree);
  const manifestEntry = entries.find(entry => entry.path === 'manifest.json');
  if (!manifestEntry) throw new Error('EXPERIENCE_ARCHIVE_INVALID: manifest missing');
  const manifest = JSON.parse(Buffer.from(await store.readBlob(manifestEntry.oid, MAX_MANIFEST_BYTES)).toString('utf8')) as Manifest;
  if (manifest.version !== 1 || manifest.workspace !== workspace || manifest.trace_id !== trace
    || manifest.digest !== receipt.digest || !Array.isArray(manifest.events) || !manifest.events.length || manifest.events.length > MAX_EXPERIENCE_EVENTS) {
    throw new Error('EXPERIENCE_ARCHIVE_INVALID: manifest identity or digest mismatch');
  }
  const events: ExperienceEvent[] = [];
  for (const [index, archived] of manifest.events.entries()) {
    if (!Array.isArray(archived.evidence) || archived.evidence.length > 4) throw new Error('EXPERIENCE_ARCHIVE_INVALID: evidence exceeds bound');
    const event: ExperienceEvent = { ...archived, evidence: [] };
    for (const [ordinal, evidence] of archived.evidence.entries()) {
      const entry = entries.find(item => item.path === `evidence/${index}/${ordinal}.txt`);
      if (!entry || entry.oid !== evidence.oid) throw new Error('EXPERIENCE_ARCHIVE_INVALID: evidence object does not match manifest');
      const bytes = Buffer.from(await store.readBlob(entry.oid, 4096));
      if (bytes.length !== evidence.bytes) throw new Error('EXPERIENCE_ARCHIVE_INVALID: evidence length differs');
      event.evidence.push({ title: evidence.title, text: bytes.toString('utf8') });
    }
    const { sequence: _sequence, actor_id: _actor, session_id: _session, created_at: _created, ...content } = event;
    experienceEventSchema.parse(content);
    events.push(event);
  }
  if (experienceHash({ workspace, trace_id: trace, events }) !== receipt.digest) throw new Error('EXPERIENCE_ARCHIVE_INVALID: evidence digest mismatch');
  return events;
}
