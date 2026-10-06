import fs from 'node:fs';
import path from 'node:path';
import { atomicWriteFile } from '../shared/atomic.js';
import { PRIVATE_FILE_MODE, privateDir } from '../shared/home.js';

/** The checkpoint store on disk: the append-only journal and fork inheritance (the sessions sweep handles retention). */

export interface CheckpointEntry {
  turn: number;
  /** The user message the turn answered (session entry id), so a fork can find the turns after its point. */
  anchor?: string;
  path: string;
  /** SHA-256 of the bytes before the turn's first change; null when the file did not exist. */
  before: string | null;
  /** SHA-256 after the turn's last change; null when deleted; undefined while the change is running. */
  after?: string | null;
  mode?: number;
  bytes: number;
}

const key = (entry: CheckpointEntry) => `${entry.turn}\0${entry.path}`;
export const serialize = (entries: CheckpointEntry[]) => entries.map((entry) => `${JSON.stringify(entry)}\n`).join('');
export const journalOf = (dir: string) => path.join(dir, 'journal.jsonl');

/** The settled entries of a journal; `dirty` when it needs a rewrite (torn last line, unreadable or superseded lines). */
export function readJournal(dir: string): { entries: CheckpointEntry[]; dirty: boolean } {
  let text: string;
  try {
    text = fs.readFileSync(journalOf(dir), 'utf8');
  } catch {
    return { entries: [], dirty: false };
  }
  const entries: CheckpointEntry[] = [];
  const index = new Map<string, number>();
  let lines = 0;
  let dirty = text.length > 0 && !text.endsWith('\n');
  for (const line of text.split('\n')) {
    if (!line.trim()) continue;
    lines++;
    let entry: CheckpointEntry;
    try {
      entry = JSON.parse(line) as CheckpointEntry;
    } catch {
      dirty = true;
      continue;
    }
    if (entry.after === undefined) continue;
    const at = index.get(key(entry));
    if (at === undefined) {
      index.set(key(entry), entries.length);
      entries.push(entry);
    } else entries[at] = entry;
  }
  // Superseded lines (a turn's later changes to the same file) are compacted once they dominate.
  if (lines > 2 * entries.length + 16) dirty = true;
  return { entries, dirty };
}

/**
 * Seed a forked session's checkpoint directory with the parent's entries whose turn is on the fork's branch, so
 * /octocode rewind reaches across the fork. Blobs are content-addressed: hard-linked (copied where links fail). Does nothing
 * when the fork already has a journal. Returns how many entries it inherited.
 */
export async function inheritCheckpoints(from: string, to: string, branch: Iterable<string>): Promise<number> {
  if (fs.existsSync(journalOf(to))) return 0;
  const ids = new Set(branch);
  const entries = readJournal(from).entries.filter((entry) => entry.anchor !== undefined && ids.has(entry.anchor));
  if (entries.length === 0) return 0;
  privateDir(to);
  privateDir(path.join(to, 'blobs'));
  for (const digest of new Set(entries.flatMap((entry) => (entry.before ? [entry.before] : [])))) {
    const source = path.join(from, 'blobs', digest);
    const target = path.join(to, 'blobs', digest);
    // A missing source blob only makes /octocode rewind skip that file.
    await fs.promises.link(source, target).catch(() => fs.promises.copyFile(source, target)).catch(() => undefined);
  }
  await atomicWriteFile(journalOf(to), serialize(entries), undefined, { mode: PRIVATE_FILE_MODE });
  return entries.length;
}

/** The id in a Pi session file's header (its first line), or undefined. */
export async function sessionIdOf(file: string): Promise<string | undefined> {
  let handle: fs.promises.FileHandle | undefined;
  try {
    handle = await fs.promises.open(file, 'r');
    const { buffer, bytesRead } = await handle.read(Buffer.alloc(16 * 1024), 0, 16 * 1024, 0);
    const header = JSON.parse(buffer.subarray(0, bytesRead).toString('utf8').split('\n')[0] ?? '') as { id?: unknown };
    return typeof header.id === 'string' ? header.id : undefined;
  } catch {
    return undefined;
  } finally {
    await handle?.close();
  }
}

