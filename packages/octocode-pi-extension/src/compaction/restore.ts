import { randomUUID } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import type { ExtensionAPI, ExtensionContext, SessionBeforeCompactEvent } from '@earendil-works/pi-coding-agent';
import { firstGap, lineCount, type LineRange } from '../files/coverage.js';
import { contextReads, seenFiles, RESTORED_FILES_TYPE, type WorkedFile } from '../files/reads.js';
import type { FileGuard } from '../files/guard.js';
import { sha256 } from '../shared/atomic.js';
import { canonicalPath } from '../shared/home.js';
import { cutText, isRecord, isStaleSessionError } from '../shared/util.js';

/**
 * After a compaction, the files the agent was working on are read again from disk and put back in its context, as
 * Claude Code does: the summary names them, but editing one would otherwise need a fresh `read` first (the file guard
 * refuses a change to a file whose read left the context). The few most recently read or changed in the summarized
 * part come back, each within a per-file budget (a longer file shows the lines from where the agent last read or changed
 * it, with a note naming them), within a total budget sized to the window, and count as read once the message is in the model's context (Pi holds a context-only message until the running turn
 * ends, so the guard records them when a model call first carries it, not when it is sent). A file the messages kept
 * after the summary already hold, as it is on disk now (`heldFiles`), is not read back.
 */

/** At most this many files come back (the few the work centered on; more would refill the context the summary freed)... */
export const RESTORE_MAX_FILES = 5;
/** ...each up to this many characters (about 6.7k tokens at `CHARS_PER_TOKEN`: a working file's relevant part; longer ones show whole lines from the last range worked on, with a note)... */
export const RESTORE_FILE_CHARS = 20_000;
/** ...and all of them together up to this many (about 16.7k tokens at `CHARS_PER_TOKEN`), but at most `RESTORE_SHARE` of the window left after Pi's reserve (`windowBudget`)... */
export const RESTORE_TOTAL_CHARS = 50_000;
/** ...the share of that window, so a small model is not refilled to its threshold. */
export const RESTORE_SHARE = 0.1;
/** A file that would get fewer characters than this is not read back: a stub counted as read helps no one. */
export const RESTORE_MIN_FILE_CHARS = 2_000;
/** At most this many candidates that cannot be read back (binary, too large, missing, not UTF-8) are tried: a run of them must not stall the compaction. */
export const RESTORE_MAX_SKIPPED = 20;
/** Files larger than this are never read back (a generated or data file is no working context, and reading it costs time). */
const RESTORE_MAX_BYTES = 2_000_000;

type Preparation = Pick<SessionBeforeCompactEvent['preparation'], 'messagesToSummarize' | 'turnPrefixMessages'>;

/**
 * The files read or changed in the summarized messages, most recently worked on first, each with the line the agent
 * last read from or changed. A file read back by an earlier compaction ranks by the work it stands for, not by when it
 * was read back, so it does not keep coming back ahead of newer work.
 */
export function recentFiles(preparation: Preparation, cwd: string): WorkedFile[] {
  const messages = [...preparation.messagesToSummarize, ...preparation.turnPrefixMessages].map((message, index) => ({ id: String(index), message }));
  return [...seenFiles(messages, cwd).values()]
    .sort((a, b) => (b.workedAt ?? 0) - (a.workedAt ?? 0))
    .map(({ file, workedAt, line }) => ({ file, ...(workedAt === undefined ? {} : { workedAt }), ...(line ? { line } : {}) }));
}

/** How much of a file is looked at before the rest is read: binary data shows in its first bytes. */
const SNIFF_BYTES = 8_192;

/**
 * The text of `file` when it is a small UTF-8 text file, else undefined. The first `SNIFF_BYTES` are checked (no NUL,
 * valid UTF-8) before the whole file is read, so a large binary file costs one small read.
 */
function textOf(file: string): { text: string; stamp: Stamp } | undefined {
  try {
    // `stat` first: opening a FIFO or device would block.
    const stat = fs.statSync(file);
    if (!stat.isFile() || stat.size > RESTORE_MAX_BYTES) return undefined;
    const fd = fs.openSync(file, 'r');
    try {
      const head = Buffer.alloc(Math.min(stat.size, SNIFF_BYTES));
      const read = fs.readSync(fd, head, 0, head.length, 0);
      const sniffed = head.subarray(0, read);
      // `stream`: a character cut by the end of the sniffed bytes is not an error.
      if (sniffed.includes(0) || !isUtf8(sniffed)) return undefined;
      const text = fs.readFileSync(fd, 'utf8');
      return text.includes('\uFFFD') ? undefined : { text, stamp: stampOf(stat) };
    } finally {
      fs.closeSync(fd);
    }
  } catch {
    return undefined;
  }
}

/** Whether `head`, the start of a file, is UTF-8 up to a character cut by its end. */
function isUtf8(head: Uint8Array): boolean {
  try {
    new TextDecoder('utf-8', { fatal: true }).decode(head, { stream: true });
    return true;
  } catch {
    return false;
  }
}

/** What a file was on disk when it was read back: a later stat that differs means the model's copy is stale. */
interface Stamp {
  mtimeMs: number;
  ctimeMs: number;
  size: number;
  ino: number;
}

const stampOf = ({ mtimeMs, ctimeMs, size, ino }: Stamp): Stamp => ({ mtimeMs, ctimeMs, size, ino });

function sameStamp(file: string, stamp: Stamp): boolean {
  try {
    const now = fs.statSync(file);
    return now.mtimeMs === stamp.mtimeMs && now.ctimeMs === stamp.ctimeMs && now.size === stamp.size && now.ino === stamp.ino;
  } catch {
    return false;
  }
}

/** A path as an XML attribute value: a file name cannot close the attribute or the tag. */
const attribute = (text: string): string => text.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

interface Window {
  text: string;
  /** First and last line shown (1-based), and the file's line count. */
  first: number;
  last: number;
  total: number;
  cut: boolean;
}

/**
 * Whole lines of `text` within `max` characters, starting at `line` (else the first line); a window that reaches the
 * end of the file with room left starts earlier. A start line longer than `max` alone is cut.
 */
function windowOf(text: string, max: number, line = 1): Window {
  const lines = text.match(/[^\n]*\n|[^\n]+$/g) ?? [];
  const total = lines.length;
  if (text.length <= max) return { text, first: 1, last: total, total, cut: false };
  const start = Math.min(Math.max(line, 1), total) - 1;
  let first = start;
  let last = start;
  let used = 0;
  while (last < total && used + lines[last]!.length <= max) used += lines[last++]!.length;
  if (last === start) return { text: cutText(lines[start]!, max, { chars: true }), first: start + 1, last: start + 1, total, cut: true };
  while (last === total && first > 0 && used + lines[first - 1]!.length <= max) used += lines[--first]!.length;
  return { text: lines.slice(first, last).join(''), first: first + 1, last, total, cut: true };
}

interface Restored {
  /** The message body: each file in a `<file>` block. */
  content: string;
  /** `stamp`: the file's stat when it was read back (`settleRestoredFiles` checks it is unchanged). */
  files: Array<{ file: string; whole: boolean; sha256?: string; size?: number; stamp: Stamp }>;
  /** The work each restored file stands for, so the next compaction ranks it by that (`seenFiles`). */
  worked: WorkedFile[];
}

/**
 * The files (canonical paths) the context holds in full as they are on disk now: shown whole, or by reads whose line
 * ranges together cover every line, by a result not older than the file's last change (`FileGuard.restore`'s rule). A
 * file shown only in part, with a stale view, or by a cleared result is not held, nor one over `RESTORE_MAX_BYTES`.
 */
export function heldFiles(reads: ReadonlyArray<{ file: string; at: number; partial?: boolean; lines?: readonly LineRange[] }>): Set<string> {
  const held = new Set<string>();
  for (const { file, at, partial, lines } of reads) {
    const real = canonicalPath(file);
    try {
      const stat = fs.statSync(real);
      // A file over the size restore reads back is never restored, so whether the context holds it does not matter: no read for it.
      if (!stat.isFile() || stat.size > RESTORE_MAX_BYTES || Math.max(stat.mtimeMs, stat.ctimeMs) > at) continue;
      if (partial && (!lines || firstGap(lines, lineCount(fs.readFileSync(real))) !== undefined)) continue;
      held.add(real);
    } catch {
      // A file that cannot be read now is not held in any useful sense: restoring it fails the same way and is skipped.
    }
  }
  return held;
}

/**
 * Reads back up to `RESTORE_MAX_FILES` of `candidates` (most recent first) except those in `held` (`heldFiles`: the
 * context has their current content), within the budgets, and gives up after `RESTORE_MAX_SKIPPED` candidates that
 * cannot be read back. Undefined when none qualifies.
 */
export function restoreFiles(candidates: readonly WorkedFile[], held: ReadonlySet<string>, cwd: string, total = RESTORE_TOTAL_CHARS): Restored | undefined {
  const blocks: string[] = [];
  const files: Restored['files'] = [];
  const worked: WorkedFile[] = [];
  let used = 0;
  let skipped = 0;
  // One file read by two paths (a symlink and its target) comes back once.
  const taken = new Set<string>();
  for (const candidate of candidates) {
    const { file } = candidate;
    const room = Math.min(RESTORE_FILE_CHARS, total - used);
    if (files.length >= RESTORE_MAX_FILES || room < RESTORE_MIN_FILE_CHARS || skipped >= RESTORE_MAX_SKIPPED) break;
    const real = canonicalPath(file);
    if (held.has(real) || taken.has(real)) continue;
    const read = textOf(file);
    const text = read?.text;
    const view = text === undefined ? undefined : windowOf(text, room, candidate.line);
    if (read === undefined || text === undefined || !view || (view.text.length === 0 && text.length > 0)) {
      skipped++;
      continue;
    }
    taken.add(real);
    const relative = path.relative(cwd, file);
    const display = relative.startsWith('..') ? file : relative;
    const from = candidate.line && view.first > 1 ? ', from where you last read or changed it' : '';
    const note = view.cut ? `\n[… shown lines ${view.first}–${view.last} of ${view.total}${from}; read the file with an offset for the rest]` : '';
    // The file's text stays exact; a file that itself contains `</file>` gets a tag it cannot close by accident.
    const tag = view.text.includes('</file') ? `file-${sha256(text).slice(0, 8)}` : 'file';
    blocks.push(`<${tag} path="${attribute(display)}">\n${view.text}${view.text.endsWith('\n') || view.text === '' ? '' : '\n'}</${tag}>${note}`);
    files.push(view.cut ? { file, whole: false, stamp: read.stamp } : { file, whole: true, sha256: sha256(text), size: Buffer.byteLength(text), stamp: read.stamp });
    worked.push(candidate);
    used += view.text.length;
  }
  if (files.length === 0) return undefined;
  const intro = '[After compaction] The files you were working on, read again from disk just now. They count as read: you may edit them without reading them first.';
  return { content: `${intro}\n\n${blocks.join('\n\n')}`, files, worked };
}

/** Files sent back to the context, recorded in the guard once a model call carries the message (`settleRestoredFiles`). */
export interface PendingRestore {
  id: string;
  files: Restored['files'];
}

/**
 * Puts `pending` (from `recentFiles`, taken before the compaction) back into the context right after it, as a hidden
 * message, within `total` characters. Run it after the guard was rebuilt from the compacted context; returns what to
 * record once the message is delivered.
 */
export function deliverRestoredFiles(pi: Pick<ExtensionAPI, 'sendMessage'>, pending: readonly WorkedFile[], ctx: Pick<ExtensionContext, 'cwd' | 'sessionManager'>, total: number): PendingRestore | undefined {
  const restored = restoreFiles(pending, heldFiles(contextReads(ctx)), ctx.cwd, total);
  if (!restored) return undefined;
  const id = randomUUID();
  try {
    const partial = restored.files.filter((entry) => !entry.whole).map((entry) => entry.file);
    const details = { id, files: restored.files.map((entry) => entry.file), ...(partial.length > 0 ? { partial } : {}), worked: restored.worked };
    pi.sendMessage({ customType: RESTORED_FILES_TYPE, content: restored.content, display: false, details }, { triggerTurn: false });
  } catch (error) {
    // Pi's ctx and pi throw this once the session was replaced; any other failure is a real error and goes on.
    if (isStaleSessionError(error)) return undefined;
    throw error;
  }
  return { id, files: restored.files };
}

/**
 * Records `pending`'s files in the guard when `messages` (a model call's) carry its message; true when they did. A file
 * that changed on disk since it was read back is recorded stale: the model holds the old text, so a write or delete
 * needs a fresh read.
 */
export function settleRestoredFiles(messages: readonly unknown[], pending: PendingRestore, guard: FileGuard): boolean {
  const carried = messages.some((message) => isRecord(message) && message['role'] === 'custom' && message['customType'] === RESTORED_FILES_TYPE && isRecord(message['details']) && message['details']['id'] === pending.id);
  if (!carried) return false;
  for (const entry of pending.files) {
    const known = entry.whole && entry.sha256 !== undefined && entry.size !== undefined ? { sha256: entry.sha256, size: entry.size } : undefined;
    const unchanged = sameStamp(entry.file, entry.stamp);
    guard.record(entry.file, unchanged ? known : undefined, !entry.whole);
    if (!unchanged) guard.stale(entry.file);
  }
  return true;
}
