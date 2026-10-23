import fs from 'node:fs';
import { sha256 } from '../shared/atomic.js';
import { canonicalPath } from '../shared/home.js';
import { describeRanges, firstGap, lineCount, mergeRanges, type LineRange } from './coverage.js';

/**
 * What the `file` tool knows about each file the agent read (tool.ts): a read is recorded by stat (size, mtime, ctime,
 * inode) of the canonical path, without reading the file again; its digest is known only when the read returned the
 * whole file (Pi's read) or the file tool wrote it, and is then checked against the bytes the change reads anyway.
 * Residual race: a file changed between the read tool reading it and the stat here is taken as read (unless its digest
 * was known from the returned text).
 */

interface Snapshot {
  mtimeMs: number;
  ctimeMs: number;
  size: number;
  ino: number;
  mode: number;
  sha256?: string;
  /** The read showed a range or a truncated view, not the whole file. */
  partial?: boolean;
  /** For a partial view: the lines the reads showed so far (merged); a write needs them to cover the file. */
  lines?: LineRange[];
}

/** Content known without reading the file again (e.g. what was just written). */
export interface KnownContent {
  sha256: string;
  size: number;
}

/** What this tool's last write put on disk: the digest and size, and the line count. */
export interface WrittenContent extends KnownContent {
  lines: number;
}

interface Inspection {
  refusal?: string;
  /** The file's stat now; undefined when it does not exist. */
  current?: Snapshot;
  /** The digest the file's bytes must still have (known from the read or the last write), if any. */
  expected?: string;
  /** A write only: the lines the reads showed, when they did not show the whole file (see `unseenRefusal`). */
  lines?: readonly LineRange[];
  /** An edit only: the file changed since the read, so each oldText must be re-validated against its current content. */
  stale?: true;
}

/**
 * Why an edit's anchors do not fit `content` exactly: the first oldText that is gone or matches more than once, named,
 * with the instruction to read the file again. Undefined when each matches exactly once.
 */
export function lostAnchor(display: string, edits: ReadonlyArray<{ oldText: string }>, content: string): string | undefined {
  for (const [index, { oldText }] of edits.entries()) {
    const count = oldText === '' ? 0 : content.split(oldText).length - 1;
    if (count === 1) continue;
    const shown = JSON.stringify(oldText.length > 60 ? `${oldText.slice(0, 60)}…` : oldText);
    return `${display} changed on disk since you last read it and edits[${index}] (oldText ${shown}) ${count === 0 ? 'is no longer in it' : `now matches ${count} times`}. Read it again with \`read\` before editing it.`;
  }
  return undefined;
}

function staleRefusal(display: string): string {
  return `${display} changed on disk since you last read it. Read it again with \`read\` before modifying it.`;
}

function unreadRefusal(display: string): string {
  return `${display} already exists and was not read in this session. Read it with \`read\` before modifying it.`;
}

/**
 * Why a full write over `data` is refused when the reads showed only `lines`: the first line never seen, and what to do
 * (`read` reaches any line with an offset, so this never asks for a whole-file read a big file cannot give). Undefined
 * when the reads covered every line.
 */
function unseenRefusal(display: string, lines: readonly LineRange[], data: Buffer): string | undefined {
  const count = lineCount(data);
  const gap = firstGap(lines, count);
  if (gap === undefined) return undefined;
  return `${display}: a write replaces the whole file, but you have seen ${describeRanges(lines)} of its ${count} lines. Read the rest with \`read\` (offset=${gap}) until every line has been read, or use an edit to change part of it.`;
}

/** The bytes a change read from disk, with their digest. */
export interface ReadBytes {
  data: Buffer;
  sha256: string;
}

/**
 * What the bytes about to change say against what the guard knows (`inspection`, from `FileGuard.inspect`): `changed`
 * when they differ from what the read saw, and a refusal when a write or delete follows such a change, or a write
 * replaces lines the reads never showed. An edit over changes is not refused here: its anchors are re-validated.
 * `read` is undefined when the file does not exist.
 */
export function judgeChange(display: string, mode: 'edit' | 'write' | 'delete', inspection: Inspection, read: ReadBytes | undefined): { changed: boolean; refusal?: string } {
  const { stale, expected, lines } = inspection;
  const changed = stale === true || (expected !== undefined && expected !== (read?.sha256 ?? null));
  if (changed && mode !== 'edit') return { changed, refusal: staleRefusal(display) };
  const unseen = lines && read ? unseenRefusal(display, lines, read.data) : undefined;
  return unseen ? { changed, refusal: unseen } : { changed };
}

function rereadRefusal(display: string): string {
  return `${display}: your earlier read of it is no longer in your context. Read it again with \`read\` before modifying it.`;
}

export class FileGuard {
  readonly #seen = new Map<string, Snapshot>();
  /** Files whose read left the context: changing them needs a fresh read first. */
  readonly #reread = new Set<string>();
  /** Files whose read is in the context but that were modified after it (known only by time, not by a snapshot). */
  readonly #changed = new Set<string>();
  /** Set by the team layer: why another agent's reservation forbids changing `file`, if it does. */
  reservedBy: ((file: string) => string | undefined) | undefined;

  /**
   * Remember the file as read now (stat only). `known`: its content digest when the caller has it (kept if the size
   * agrees). `partial`: the read showed a range or a cut view, so a write needs the reads to cover every line (`lines`
   * is what this read showed, when known); an edit or a delete does not. Reads of an unchanged file add up: a later
   * range joins the earlier ones, and never takes back a whole read.
   */
  record(file: string, known?: KnownContent, partial = false, lines?: LineRange): void {
    const real = canonicalPath(file);
    const stat = statOf(real);
    this.#reread.delete(real);
    // A read lifts the stale flag, but a range of a file that changed after its recorded read shows only that range of
    // the current file: the earlier ranges describe other bytes, so they are not carried over.
    const wasStale = this.#changed.delete(real);
    if (!stat) {
      this.#seen.delete(real);
      return;
    }
    const before = wasStale ? undefined : this.#seen.get(real);
    const same = before !== undefined && sameStat(before, stat);
    if (partial && same && !before.partial) return;
    const covered = partial ? mergeRanges(same ? (before.lines ?? []) : [], lines) : undefined;
    this.#seen.set(real, { ...stat, ...knownDigest(known, stat), ...(covered ? { partial: true, lines: covered } : {}) });
  }

  /**
   * Record the file after this tool's own edit: its content digest is known (`known`), but the agent has seen only what
   * its earlier reads showed, so a partial read stays partial. The ranges stay when the edit kept the line count
   * (`lineDelta` 0); a different count moved the lines below the edit, so they are dropped and the reads start over.
   * A file with no recorded read is not known to be seen at all.
   */
  edited(file: string, known: KnownContent | undefined, lineDelta: number | undefined): void {
    const real = canonicalPath(file);
    const before = this.#seen.get(real);
    this.record(real, known);
    const now = this.#seen.get(real);
    if (!now || (before && !before.partial)) return;
    this.#seen.set(real, { ...now, partial: true, lines: lineDelta === 0 ? (before?.lines ?? []) : [] });
  }

  /** The file's bytes changed after its read that the guard records (an edit applied over outside changes): a write or delete needs a fresh read. */
  stale(file: string): void {
    this.#changed.add(canonicalPath(file));
  }

  /** Forget one file, e.g. after /octocode rewind replaced it: the model must read it again before writing it. */
  forget(file: string): void {
    const real = canonicalPath(file);
    this.#seen.delete(real);
    this.#changed.delete(real);
    this.#reread.add(real);
  }

  /**
   * Rebuild the memory from what the context shows now (see `contextReads`), after a session start (resume, reload,
   * fork), compaction or branch switch. A file read earlier but no longer in view must be read again. One in view
   * counts as read when it was not modified after the read (`at`, ms); otherwise it is stale.
   */
  restore(inContext: Iterable<{ file: string; at: number; partial?: boolean; lines?: readonly LineRange[] }>): void {
    for (const real of this.#seen.keys()) this.#reread.add(real);
    this.#seen.clear();
    this.#changed.clear();
    for (const { file, at, partial, lines } of inContext) {
      const real = canonicalPath(file);
      const stat = statOf(real);
      if (!stat) continue;
      this.#reread.delete(real);
      if (Math.max(stat.mtimeMs, stat.ctimeMs) <= at) this.#seen.set(real, { ...stat, ...(partial ? { partial, lines: mergeRanges(lines ?? []) } : {}) });
      else this.#changed.add(real);
    }
  }

  /**
   * Returns an error message when the mutation must be refused, else undefined. An existing file must have been read
   * (or written by this tool) in this session, and not changed on disk or left the context since; a new file is free.
   * A write also needs every line of the file read (in one read or several); an edit does not, because its exact match
   * proves the edited region was seen, and a delete needs only proof that the read was of the current file.
   * `runQuery` applies the same two steps (`inspect`, then `judgeChange` over the bytes it reads); this runs them for
   * callers that only ask (the guard's tests).
   */
  check(file: string, mode: 'edit' | 'write' | 'delete', display: string): string | undefined {
    const inspection = this.inspect(file, mode, display);
    if (inspection.refusal) return inspection.refusal;
    try {
      const data = inspection.current ? fs.readFileSync(file) : undefined;
      return judgeChange(display, mode, inspection, data ? { data, sha256: sha256(data) } : undefined).refusal;
    } catch {
      return staleRefusal(display);
    }
  }

  /**
   * Like check, by stat alone (no read). Also returns the current stat and the digest the file must still have; the
   * caller verifies that against the bytes it reads for the change.
   */
  inspect(file: string, mode: 'edit' | 'write' | 'delete', display: string): Inspection {
    const reserved = this.reservedBy?.(file);
    if (reserved) return { refusal: reserved };
    const real = canonicalPath(file);
    const current = statOf(real);
    if (!current) return mode === 'edit' || mode === 'delete' ? { refusal: `${display} does not exist.` } : {};
    if (this.#reread.has(real)) return { refusal: rereadRefusal(display) };
    if (this.#changed.has(real)) return changedSinceRead(mode, display, current);
    const seen = this.#seen.get(real);
    if (!seen) return { refusal: unreadRefusal(display) };
    if (movedSinceRead(seen, current)) return changedSinceRead(mode, display, current);
    return { current, ...(seen.sha256 ? { expected: seen.sha256 } : {}), ...(seen.partial && mode === 'write' ? { lines: seen.lines ?? [] } : {}) };
  }
}

/** The file changed after its read: an edit goes on to re-validate its anchors, a write or delete is refused. */
function changedSinceRead(mode: 'edit' | 'write' | 'delete', display: string, current: Snapshot): Inspection {
  return mode === 'edit' ? { current, stale: true } : { refusal: staleRefusal(display) };
}

/** Two stats of the same unchanged file: mtime, ctime, size and inode all agree. */
function sameStat(a: Snapshot, b: Snapshot): boolean {
  return a.mtimeMs === b.mtimeMs && a.ctimeMs === b.ctimeMs && a.size === b.size && a.ino === b.ino;
}

/**
 * Whether the stat moved since the read in a way that makes it stale. ctime and inode also move on a same-size rewrite
 * that restored the mtime, or a rename over the file. With a known digest a same-size move is left to the content check
 * (a touch or a no-op rewrite is not a change).
 */
function movedSinceRead(seen: Snapshot, current: Snapshot): boolean {
  return !sameStat(seen, current) && (!seen.sha256 || seen.size !== current.size);
}

/** The digest to keep for a read: the known content's, when its size agrees with the file's. */
function knownDigest(known: KnownContent | undefined, stat: Snapshot): { sha256?: string } {
  return known && known.size === stat.size ? { sha256: known.sha256 } : {};
}

function statOf(file: string): Snapshot | undefined {
  try {
    const stat = fs.statSync(file);
    return stat.isFile() ? { mtimeMs: stat.mtimeMs, ctimeMs: stat.ctimeMs, size: stat.size, ino: stat.ino, mode: stat.mode } : undefined;
  } catch {
    return undefined;
  }
}
