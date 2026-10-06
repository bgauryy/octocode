import fs from 'node:fs';
import path from 'node:path';
import { DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, formatSize, truncateHead, truncateTail } from '@earendil-works/pi-coding-agent';
import { PRIVATE_FILE_MODE, sessionOutputDir } from './home.js';
import { shortPath } from './format.js';
import { capOutput, utf8Head } from './util.js';

/** Bytes and lines kept back from the budget for the omission and pointer notes. */
const NOTE_BYTES = 512;
const NOTE_LINES = 6;

let written = 0;

/** `label` as a file-name part: anything outside `[A-Za-z0-9_-]` becomes `_`. */
export function spillLabel(label: string): string {
  return label.replace(/[^A-Za-z0-9_-]/g, '_').slice(0, 80) || 'output';
}

/**
 * Writes `text` to `<session folder>/output/<label>-<timestamp>-<n>.txt` (`sessionOutputDir`) and returns the path, or
 * undefined when it could not be written. The session sweep removes old files. Never throws.
 */
export function saveFullOutput(text: string, label: string): string | undefined {
  try {
    const file = path.join(sessionOutputDir('output'), `${spillLabel(label)}-${Date.now()}-${++written}.txt`);
    fs.writeFileSync(file, text, { flag: 'wx', mode: PRIVATE_FILE_MODE });
    return file;
  } catch {
    return undefined;
  }
}

/**
 * Keeps a model-facing result inside Pi's per-result budget without losing it: text that fits comes back unchanged;
 * longer text is saved whole to a file and replaced by its head (about 70% of the budget) and tail (about 30%) plus a
 * note naming the file. When the file cannot be written, falls back to `capOutput` and says so. Never throws.
 */
export function capOutputToFile(text: string, options: { label: string; maxBytes?: number; maxLines?: number }): string {
  const maxBytes = options.maxBytes ?? DEFAULT_MAX_BYTES;
  const maxLines = options.maxLines ?? DEFAULT_MAX_LINES;
  const whole = truncateHead(text, { maxBytes, maxLines });
  if (!whole.truncated) return text;
  const file = saveFullOutput(text, options.label);
  if (!file) return `${capOutput(text, maxBytes, maxLines)}\n[full output could not be saved]`;
  const bytes = Math.max(maxBytes - NOTE_BYTES, 64);
  const lines = Math.max(maxLines - NOTE_LINES, 2);
  const head = headPreview(text, Math.floor(bytes * 0.7), Math.max(Math.floor(lines * 0.7), 1));
  const tail = tailPreview(text, Math.floor(bytes * 0.3), Math.max(Math.floor(lines * 0.3), 1));
  const shownLines = head.lines + tail.lines;
  const shownBytes = Buffer.byteLength(head.text) + Buffer.byteLength(tail.text);
  const omitted = Math.max(whole.totalLines - shownLines, 0);
  const gap = omitted > 0 ? `[… ${omitted} line${omitted === 1 ? '' : 's'} omitted …]` : `[… ${formatSize(Math.max(whole.totalBytes - shownBytes, 0))} omitted …]`;
  return (
    `${head.text}\n\n${gap}\n\n${tail.text}\n\n` +
    `[Output truncated: showing ${shownLines} of ${whole.totalLines} lines (${formatSize(shownBytes)} of ${formatSize(whole.totalBytes)}). Full output: ${shortPath(file)}; search it or read it by line range.]`
  );
}

/** Whole leading lines within the budget; the head of the first line when it alone is over the byte budget. */
function headPreview(text: string, maxBytes: number, maxLines: number): { text: string; lines: number } {
  const cut = truncateHead(text, { maxBytes, maxLines });
  if (cut.content !== '') return { text: cut.content, lines: cut.outputLines };
  return { text: utf8Head(text, maxBytes), lines: 1 };
}

/** Trailing lines within the budget; the end of the last line when it alone is over the byte budget. */
function tailPreview(text: string, maxBytes: number, maxLines: number): { text: string; lines: number } {
  const cut = truncateTail(text, { maxBytes, maxLines });
  if (cut.content !== '') return { text: cut.content, lines: cut.outputLines };
  const buffer = Buffer.from(text, 'utf8');
  const tail = buffer.subarray(Math.max(buffer.length - maxBytes, 0)).toString('utf8').replace(/^\uFFFD+/, '');
  return { text: tail, lines: 1 };
}
