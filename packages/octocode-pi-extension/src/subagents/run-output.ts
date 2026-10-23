import fs from 'node:fs';
import path from 'node:path';
import { tailNote } from '../shared/format.js';
import { PRIVATE_FILE_MODE, privateDir, sessionOutputDir } from '../shared/home.js';
import { visibleControls } from '../shared/sanitize.js';
import { cutText } from '../shared/util.js';
import { noticeText, type ChildNotices, type ChildState } from './events.js';

/** Characters of a child's stderr a failure shows (its end); a longer stderr is saved whole. */
const STDERR_TAIL_CHARS = 4_000;
const STDERR_FILE = 'stderr.txt';
const NOTICES_FILE = 'notices.txt';

/**
 * A child's stderr, bounded in memory: its last `STDERR_TAIL_CHARS` characters (control characters made visible), and,
 * once it outgrows that, all of it streamed to `stderr.txt` in `dir`, so a failure that shows only the end can name the whole.
 */
export class StderrTail {
  private tail = '';
  private total = 0;
  private fd: number | undefined;
  private file: string | undefined;
  private failed = false;

  constructor(private readonly dir: string | undefined) {}

  write(chunk: string): void {
    const text = visibleControls(chunk);
    const before = this.tail;
    this.total += text.length;
    this.tail = cutText(before + text, STDERR_TAIL_CHARS, { chars: true, tail: true });
    if (this.total <= STDERR_TAIL_CHARS || this.failed) return;
    try {
      if (this.fd === undefined) {
        if (!this.dir) throw new Error('no folder');
        const file = path.join(this.dir, STDERR_FILE);
        this.fd = fs.openSync(file, 'w', PRIVATE_FILE_MODE);
        this.file = file;
        // Nothing was cut before the first overflow, so the tail is everything that came earlier.
        fs.writeSync(this.fd, before);
      }
      fs.writeSync(this.fd, text);
    } catch {
      this.failed = true;
      this.file = undefined;
      this.close();
    }
  }

  close(): void {
    if (this.fd === undefined) return;
    try {
      fs.closeSync(this.fd);
    } catch {
      // Nothing to do: the file is a convenience copy.
    }
    this.fd = undefined;
  }

  /** What an error shows: the whole stderr when it fit, else a note on the cut and where the whole is, then the end. */
  text(): string {
    const shown = this.tail.trim() || 'no output';
    return this.total > STDERR_TAIL_CHARS ? `${tailNote('stderr', this.tail.length, this.total, this.file)}\n${shown}` : shown;
  }
}

/**
 * The run's notices as the parent gets them at the end. When some did not fit the inline page, every notice (whole, one
 * per line, repeats folded with their count) is saved to `notices.txt` in `dir` and the notices name it; when it cannot
 * be saved, every notice goes inline instead, so none is dropped. Undefined when the inline page already holds them all.
 */
export function savedNotices(state: ChildState, dir: string | undefined): ChildNotices | undefined {
  if (state.notices.more === 0) return undefined;
  const all = [...state.noticeLog.values()];
  try {
    if (!dir) throw new Error('no folder');
    const file = path.join(dir, NOTICES_FILE);
    fs.writeFileSync(file, `${all.map(noticeText).join('\n')}\n`, { mode: PRIVATE_FILE_MODE });
    return { ...state.notices, file };
  } catch {
    return { ...state.notices, kept: all, more: 0 };
  }
}

/** Where a child without a scratch folder (an unwritable workspace) spills a long report, so a cut report always has its whole copy. */
export function reportDir(id: string): string | undefined {
  try {
    return privateDir(path.join(sessionOutputDir('output'), `agent-${id.replace(/[^\w.-]/g, '_')}`));
  } catch {
    return undefined;
  }
}
