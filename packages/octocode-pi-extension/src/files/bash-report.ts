import fs from 'node:fs';
import { formatDuration, shortPath } from '../shared/format.js';
import { visibleControls, withoutColors } from '../shared/sanitize.js';
import { cutText, firstLine } from '../shared/util.js';
import type { BashJob, JobExit } from './bash-jobs.js';

/** Output tail a finished job reports back; the report counts the lines before it and names the log that holds them. */
const TAIL_BYTES = 4 * 1024;
const TAIL_LINES = 40;

/** Lines in `fd` (a last line without a newline counts too), read in 1 MB chunks. */
function countLines(fd: number, size: number): number {
  const chunk = Buffer.alloc(Math.min(size, 1024 * 1024));
  let lines = 0;
  let last = 0x0a;
  for (let position = 0; position < size; ) {
    const read = fs.readSync(fd, chunk, 0, chunk.length, position);
    if (read <= 0) break;
    for (let at = chunk.indexOf(0x0a); at !== -1 && at < read; at = chunk.indexOf(0x0a, at + 1)) lines++;
    last = chunk[read - 1]!;
    position += read;
  }
  return last === 0x0a ? lines : lines + 1;
}

/**
 * The end of a job's log: its last whole lines, at most `maxLines` within `maxBytes`, and how many lines come before
 * them (they stay in the log; a rotated log names its `.1` file on its first line).
 */
export function logTail(file: string, maxBytes = TAIL_BYTES, maxLines = TAIL_LINES): { text: string; earlier: number } {
  try {
    const fd = fs.openSync(file, 'r');
    try {
      const size = fs.fstatSync(fd).size;
      const length = Math.min(size, maxBytes);
      const buffer = Buffer.alloc(length);
      fs.readSync(fd, buffer, 0, length, size - length);
      let text = buffer.toString('utf8');
      if (size > length) text = text.slice(text.indexOf('\n') + 1);
      const lines = text.split('\n');
      if (lines.at(-1) === '') lines.pop();
      const kept = lines.slice(-maxLines);
      return { text: kept.join('\n'), earlier: countLines(fd, size) - kept.length };
    } finally {
      fs.closeSync(fd);
    }
  } catch {
    return { text: '', earlier: 0 };
  }
}

/** The message a finished job sends: how it ended, how long it ran, and the tail of its output. */
export function jobReport(job: BashJob, exit: JobExit): string {
  const how =
    job.stopped === 'timeout'
      ? `stopped after its ${formatDuration((job.timeout ?? 0) * 1000)} timeout`
      : job.stopped
        ? `stopped (${job.stopped})`
        : exit.code === 0
          ? 'finished'
          : exit.code !== null
            ? `failed with exit code ${exit.code}`
            : `ended by ${exit.signal ?? 'an error'}`;
  const { text, earlier } = logTail(job.log);
  // Logs hold raw terminal output. Colors are dropped (as Pi's bash does; the log keeps them); every other control
  // character shows as a visible escape, so nothing is hidden from the model.
  const tail = visibleControls(withoutColors(text)).trimEnd();
  const before = earlier > 0 ? `[+${earlier} earlier line${earlier === 1 ? '' : 's'} in the log]` : '';
  const stopped = exit.logStopped ? `log stopped at ${exit.logStopped.bytes} bytes: ${exit.logStopped.error}; later output was not recorded` : '';
  // Rotation keeps one older file: the first rotation leaves the earlier output in `.1`, a later one replaces it.
  const rotated = exit.logRotations ? `log rotated at its size cap${exit.logRotations > 1 ? ` ${exit.logRotations} times` : ''}: ${shortPath(`${job.log}.1`, job.cwd)} holds the part before this log${exit.logRotations > 1 ? '; output before that was dropped and cannot be recovered' : ''}` : '';
  return [`Background bash ${job.id} ${how} after ${formatDuration(exit.seconds * 1000)}: ${cutText(firstLine(job.command), 120, { chars: true })}`, `Log: ${shortPath(job.log, job.cwd)}`, rotated, stopped, before, tail || (before ? '' : '(no output)')].filter(Boolean).join('\n');
}
