import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { shortPath } from '../shared/format.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { capChars, capOutput } from '../shared/util.js';

export const REPORT_MAX_BYTES = 8 * 1024;
const REPORT_MAX_LINES = 200;
const SEPARATOR = '\n\n---\n\n';
const preview = (text: string) => capOutput(text, REPORT_MAX_BYTES, REPORT_MAX_LINES);

/** A bounded preview and an incremental disk report. Retries replace the current section; incoming messages start a new one. */
export class ReportTracker {
  private text = '';
  private before = '';
  private lastText = '';
  private signature = '';
  private sections = 0;
  private bytes = 0;
  private sectionStart = 0;
  private fresh = true;
  private failure: string | undefined;
  private file: string | undefined;
  private partialFile: string | undefined;
  private spillFailed = false;

  constructor(private readonly scratch?: string) {}

  incoming(role: unknown): void {
    if (role === 'user' || role === 'custom') this.fresh = true;
  }

  answer(raw: string | undefined, stopReason: unknown, errorMessage?: unknown): void {
    const failed = stopReason === 'error' || stopReason === 'aborted';
    this.failure = failed ? capChars(sanitizeTerminalText(String(errorMessage ?? stopReason)), 500) : undefined;
    if (!raw) return;
    const text = sanitizeTerminalText(raw);
    this.lastText = preview(text);
    if (stopReason === 'toolUse' || failed) {
      this.partialFile = undefined;
      if (this.sections === 0 && this.lastText !== text && this.scratch) {
        try {
          const file = path.join(this.scratch, 'partial-report.md');
          fs.writeFileSync(file, text);
          this.partialFile = file;
        } catch { /* Keep the bounded partial answer when disk is unavailable. */ }
      }
      return;
    }
    const signature = createHash('sha512').update(text).digest('hex');
    const append = this.fresh || this.sections === 0;
    if (append && signature === this.signature) {
      this.fresh = false;
      return;
    }
    if (append) {
      this.before = this.text;
      this.sectionStart = this.bytes;
      this.sections++;
    }
    const section = `${this.sections > 1 ? SEPARATOR : ''}${text}`;
    const whole = this.before + section;
    const capped = preview(whole);
    try {
      if (this.file) {
        if (!append) fs.truncateSync(this.file, this.sectionStart);
        fs.appendFileSync(this.file, section);
      } else if (!this.spillFailed && capped !== whole && this.scratch) {
        const file = path.join(this.scratch, 'report.md');
        fs.writeFileSync(file, whole);
        this.file = file;
      }
    } catch {
      // A disk failure must not discard the bounded answer or advertise an incomplete report.
      this.file = undefined;
      this.spillFailed = true;
    }
    this.bytes = this.sectionStart + Buffer.byteLength(section);
    this.text = capped;
    this.signature = signature;
    this.fresh = false;
  }

  result(): { text: string; error?: string; answered: boolean } {
    if (this.sections === 0) return {
      text: `${this.lastText}${this.partialFile ? `\nFull partial report: ${shortPath(this.partialFile)} (read only the parts you need).` : ''}`,
      answered: false,
      ...(this.failure ? { error: this.failure } : {}),
    };
    const text = this.failure ? `${this.text}${SEPARATOR}(A later follow-up turn failed: ${this.failure})` : this.text;
    return {
      text: `${text}${this.file ? `\nFull report: ${shortPath(this.file)} (read only the parts you need).` : ''}`,
      answered: true,
    };
  }
}
