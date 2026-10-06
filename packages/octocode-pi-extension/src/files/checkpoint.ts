import fs from 'node:fs';
import path from 'node:path';
import type { ExtensionContext } from '@earendil-works/pi-coding-agent';
import { PRIVATE_FILE_MODE as mode, privateDir } from '../shared/home.js';
import { atomicWriteFile, atomicWriteFileSync, sha256, tempSibling } from '../shared/atomic.js';
import { journalOf, readJournal, serialize, type CheckpointEntry } from './checkpoint-store.js';

/**
 * Edit checkpoints: before the `file` tool first changes a path in a turn, its current bytes are saved
 * (content-addressed) with the turn number, and after the change the new digest is recorded. `/octocode rewind [turns]`
 * restores the last turns, newest first, but only files still exactly as the agent left them; a file changed since
 * (by the user, bash or a peer) is listed and left alone. Forking the session offers the same restore, and the fork
 * inherits the parent's checkpoints on its branch.
 * Stored under `<Octocode home>/agent/pi/sessions/<session id>/checkpoints/` as `journal.jsonl` plus `blobs/<sha256>`. The journal is
 * append-only (one line per settled change; a later line for the same turn and path wins) and is rewritten only
 * when checkpoints are dropped (prune, rewind) or its last line was torn by a crash.
 */

const MAX_FILE_BYTES = 8 * 1024 * 1024;
const MAX_TOTAL_BYTES = 128 * 1024 * 1024;
const MAX_ENTRIES = 256;

/** The bytes a change is about to replace, when the caller has already read them (so they are not read again). */
interface CurrentContent {
  data: Uint8Array;
  sha256: string;
  mode: number;
}

export interface Capture {
  /** Whether this call created the entry (so a failed change can drop it). */
  fresh: boolean;
  note?: string;
}

export interface RewindResult {
  restored: string[];
  /** Changed since the agent's change (or its checkpoint is gone): left as they are. */
  skipped: string[];
}

async function digestOf(file: string): Promise<string | undefined> {
  try {
    return sha256(await fs.promises.readFile(file));
  } catch {
    return undefined;
  }
}

function errorCode(error: unknown): string | undefined {
  return (error as NodeJS.ErrnoException | undefined)?.code;
}

/**
 * Delete `file` only if it still holds `expected`: move it aside first, so a concurrent rewrite is never lost; when
 * the moved bytes differ, put them back (unless yet another file took the name, then the moved copy stays aside).
 */
async function removeIfUnchanged(file: string, expected: string): Promise<void> {
  const aside = tempSibling(file);
  await fs.promises.rename(file, aside);
  if ((await digestOf(aside)) === expected) {
    await fs.promises.rm(aside, { force: true });
    return;
  }
  try {
    await fs.promises.link(aside, file);
    await fs.promises.rm(aside, { force: true });
  } catch (error) {
    if (errorCode(error) !== 'EEXIST') await fs.promises.rename(aside, file);
  }
  throw new Error(`${file} changed during the rewind`);
}

export class Checkpoints {
  private dir: string | undefined;
  private entries: CheckpointEntry[] = [];
  private turn = 0;
  private anchor: string | undefined;
  private made = false;
  /** Blobs captures are storing now, by digest, with how many: not yet referenced by an entry, but not garbage. */
  private readonly storing = new Map<string, number>();

  /** Bind to a session's checkpoint directory, loading what an earlier run of the session recorded. */
  open(dir: string): void {
    this.dir = dir;
    this.made = false;
    const { entries, dirty } = readJournal(dir);
    this.entries = entries;
    this.turn = Math.max(0, ...entries.map((entry) => entry.turn));
    this.anchor = undefined;
    // A torn last line would swallow the next appended one: rewrite the journal from what parsed.
    if (dirty) atomicWriteFileSync(this.journal(), serialize(entries), { mode });
  }

  /** A new user turn: its first change to each file is checkpointed again. */
  beginTurn(): void {
    this.turn = Math.max(this.turn, ...this.entries.map((entry) => entry.turn)) + 1;
    this.anchor = undefined;
  }

  /** Turns with checkpoints, newest first. */
  turns(): number[] {
    return [...new Set(this.entries.map((entry) => entry.turn))].sort((a, b) => b - a);
  }

  /**
   * Turns on the session branch `branch` (ids root → leaf), newest first. A turn with no anchor (no user message was
   * found when it began) cannot be placed on any branch, so it counts as this one's rather than becoming unreachable.
   */
  turnsOn(branch: string[]): number[] {
    const on = new Set(branch);
    const turns = this.entries.filter((entry) => entry.anchor === undefined || on.has(entry.anchor)).map((entry) => entry.turn);
    return [...new Set(turns)].sort((a, b) => b - a);
  }

  /** Distinct files each anchor's settled turns changed, by anchor entry id. */
  filesByAnchor(): Map<string, Set<string>> {
    const files = new Map<string, Set<string>>();
    for (const entry of this.entries) {
      if (entry.anchor === undefined || entry.after === undefined) continue;
      const set = files.get(entry.anchor) ?? new Set<string>();
      files.set(entry.anchor, set.add(entry.path));
    }
    return files;
  }

  list(): readonly CheckpointEntry[] {
    return this.entries;
  }

  /**
   * Save `file`'s bytes before a change, once per turn. `current`: the bytes the caller already read (null = the file
   * does not exist); omitted, they are read here. Returns a note when the file is not checkpointed.
   */
  async capture(file: string, anchor?: () => string | undefined, current?: CurrentContent | null): Promise<Capture> {
    if (!this.dir) return { fresh: false };
    if (this.entries.some((entry) => entry.turn === this.turn && entry.path === file)) return { fresh: false };
    let state = current;
    if (state === undefined) {
      const stat = await fs.promises.stat(file).catch(() => undefined);
      if (stat && !stat.isFile()) return { fresh: false };
      if (stat && stat.size > MAX_FILE_BYTES) return { fresh: false, note: 'not checkpointed: over 8 MiB' };
      if (stat) {
        const data = await fs.promises.readFile(file);
        state = { data, sha256: sha256(data), mode: stat.mode };
      } else state = null;
    }
    const bytes = state?.data.length ?? 0;
    if (bytes > MAX_FILE_BYTES) return { fresh: false, note: 'not checkpointed: over 8 MiB' };
    // The in-flight turn is never pruned, so it cannot grow past the caps either.
    const own = this.entries.filter((entry) => entry.turn === this.turn);
    if (own.length >= MAX_ENTRIES || own.reduce((sum, entry) => sum + entry.bytes, 0) + bytes > MAX_TOTAL_BYTES) return { fresh: false, note: 'not checkpointed: turn over the checkpoint cap' };
    this.anchor ??= anchor?.();
    const entry: CheckpointEntry = { turn: this.turn, path: file, before: null, bytes: 0, ...(this.anchor ? { anchor: this.anchor } : {}) };
    if (state) {
      entry.before = state.sha256;
      entry.mode = state.mode & 0o7777;
      entry.bytes = bytes;
      const digest = state.sha256;
      this.storing.set(digest, (this.storing.get(digest) ?? 0) + 1);
      try {
        await this.storeBlob(state);
      } finally {
        const left = this.storing.get(digest)! - 1;
        if (left > 0) this.storing.set(digest, left);
        else this.storing.delete(digest);
      }
    }
    this.entries.push(entry);
    return { fresh: true };
  }

  /**
   * Record the file's state after a change (or forget a fresh entry when the change failed). `after`: the digest the
   * change left (null = deleted) when the caller already knows it, so the file is not hashed again.
   */
  async settle(file: string, capture: Capture, ok: boolean, after?: string | null): Promise<void> {
    if (!this.dir) return;
    const entry = this.entries.find((candidate) => candidate.turn === this.turn && candidate.path === file);
    if (!entry) return;
    if (!ok && capture.fresh) {
      this.entries.splice(this.entries.indexOf(entry), 1);
      await this.collect();
      return;
    }
    if (ok && after !== undefined) entry.after = after;
    else if (ok || entry.after === undefined) entry.after = (await digestOf(file)) ?? null;
    if (this.prune()) {
      await this.collect();
      await this.rewrite();
    } else await this.append(entry);
  }

  /** Turns whose user message is at or after `entryId` on the session branch `branch` (ids root → leaf). */
  turnsFrom(branch: string[], entryId: string): number[] {
    const start = branch.indexOf(entryId);
    if (start < 0) return [];
    const turns = this.entries.filter((entry) => entry.anchor !== undefined && branch.indexOf(entry.anchor) >= start).map((entry) => entry.turn);
    return [...new Set(turns)].sort((a, b) => b - a);
  }

  /** Turns anchored on `branch` at an entry outside `keep` (what leaving the branch for `keep` abandons), newest first. */
  turnsLeaving(branch: string[], keep: ReadonlySet<string>): number[] {
    const on = new Set(branch);
    const turns = this.entries.filter((entry) => entry.anchor !== undefined && on.has(entry.anchor) && !keep.has(entry.anchor)).map((entry) => entry.turn);
    return [...new Set(turns)].sort((a, b) => b - a);
  }

  /** Undo the given turns, newest change first; drops their checkpoints. */
  async rewind(turns: number[]): Promise<RewindResult> {
    const chosen = new Set(turns);
    const restored = new Set<string>();
    const skipped = new Set<string>();
    for (const entry of this.entries.filter((candidate) => chosen.has(candidate.turn) && candidate.after !== undefined).reverse()) {
      if (skipped.has(entry.path)) continue;
      const current = (await digestOf(entry.path)) ?? null;
      if (current !== entry.after) {
        restored.delete(entry.path);
        skipped.add(entry.path);
        continue;
      }
      try {
        await this.restore(entry, current);
        restored.add(entry.path);
      } catch {
        restored.delete(entry.path);
        skipped.add(entry.path);
      }
    }
    this.entries = this.entries.filter((entry) => !chosen.has(entry.turn));
    await this.rewrite();
    await this.collect();
    return { restored: [...restored], skipped: [...skipped] };
  }

  private async restore(entry: CheckpointEntry, current: string | null): Promise<void> {
    if (entry.before === null) {
      if (current !== null) await removeIfUnchanged(entry.path, current);
      return;
    }
    const data = await fs.promises.readFile(this.blob(entry.before));
    if (sha256(data) !== entry.before) throw new Error(`checkpoint of ${entry.path} is corrupt`);
    await fs.promises.mkdir(path.dirname(entry.path), { recursive: true });
    await atomicWriteFile(entry.path, data, current);
    if (entry.mode !== undefined) await fs.promises.chmod(entry.path, entry.mode);
  }

  /**
   * Content-addressed: an existing blob of the right size already holds these bytes (blobs are written by temp file
   * and rename, and /octocode rewind verifies the digest before restoring), so it is not read again.
   */
  private async storeBlob(state: CurrentContent): Promise<void> {
    const blob = this.blob(state.sha256);
    const size = await fs.promises.stat(blob).then((stat) => stat.size, () => -1);
    if (size === state.data.length) return;
    privateDir(path.dirname(blob));
    this.made = true;
    await atomicWriteFile(blob, state.data, undefined, { mode });
  }

  /** Keep within the caps by dropping the oldest turns, never the in-flight one. Returns whether anything was dropped. */
  private prune(): boolean {
    const over = () => this.entries.length > MAX_ENTRIES || this.totalBytes() > MAX_TOTAL_BYTES;
    let dropped = false;
    while (over()) {
      const older = this.entries.filter((entry) => entry.turn !== this.turn);
      if (older.length === 0) break;
      const oldest = Math.min(...older.map((entry) => entry.turn));
      this.entries = this.entries.filter((entry) => entry.turn !== oldest);
      dropped = true;
    }
    return dropped;
  }

  private totalBytes(): number {
    const sizes = new Map<string, number>();
    for (const entry of this.entries) if (entry.before) sizes.set(entry.before, entry.bytes);
    return [...sizes.values()].reduce((sum, size) => sum + size, 0);
  }

  /** Delete blobs no entry refers to. */
  private async collect(): Promise<void> {
    if (!this.dir) return;
    const blobs = await fs.promises.readdir(path.join(this.dir, 'blobs')).catch(() => []);
    // Decided per blob at removal time, so a capture that finished storing meanwhile keeps its blob; temp files
    // (dot names) belong to a storeBlob in flight.
    const live = (blob: string) => blob.startsWith('.') || this.storing.has(blob) || this.entries.some((entry) => entry.before === blob);
    for (const blob of blobs) if (!live(blob)) await fs.promises.rm(path.join(this.dir, 'blobs', blob), { force: true });
  }

  /** One settled change: a single appended line (a crash can tear only this line, which the next open drops). */
  private async append(entry: CheckpointEntry): Promise<void> {
    if (!this.made) {
      privateDir(this.dir ?? '');
      this.made = true;
    }
    await fs.promises.appendFile(this.journal(), `${JSON.stringify(entry)}\n`, { mode });
  }

  private async rewrite(): Promise<void> {
    if (!this.dir) return;
    privateDir(this.dir);
    this.made = true;
    await atomicWriteFile(this.journal(), serialize(this.entries.filter((entry) => entry.after !== undefined)), undefined, { mode });
  }

  private journal(): string {
    return journalOf(this.dir ?? '');
  }

  private blob(digest: string): string {
    return path.join(this.dir ?? '', 'blobs', digest);
  }
}

/** The id of the latest user message on the session branch: the turn's anchor for fork restores. */
export function lastUserEntry(ctx: ExtensionContext): string | undefined {
  const branch = ctx.sessionManager?.getBranch?.() ?? [];
  for (let index = branch.length - 1; index >= 0; index--) {
    const entry = branch[index];
    if (entry?.type === 'message' && entry.message.role === 'user') return entry.id;
  }
  return undefined;
}

