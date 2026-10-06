import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { ReportTracker } from '../src/subagents/report.js';
import { tmp } from './helpers.js';

describe('incremental subagent reports', () => {
  it('preserves a large partial answer when a child fails before completing a report', () => {
    const scratch = tmp();
    const report = new ReportTracker(scratch);
    const partial = 'partial detail '.repeat(2000);
    report.answer(partial, 'error', 'provider failed');
    expect(report.result()).toMatchObject({ answered: false, error: 'provider failed' });
    expect(report.result().text).toContain('Full partial report:');
    expect(Buffer.byteLength(report.result().text)).toBeLessThan(10 * 1024);
    expect(fs.readFileSync(path.join(scratch, 'partial-report.md'), 'utf8')).toBe(partial);
  });

  it('preserves all follow-ups on disk with a bounded preview', () => {
    const scratch = tmp();
    const report = new ReportTracker(scratch);
    const sections = Array.from({ length: 100 }, (_, i) => `Section ${i}: ${'detail '.repeat(2000)}`);
    for (const section of sections) {
      report.incoming('custom');
      report.answer(section, 'stop');
      expect(Buffer.byteLength(report.result().text)).toBeLessThan(10 * 1024);
    }
    expect(fs.readFileSync(path.join(scratch, 'report.md'), 'utf8')).toBe(sections.join('\n\n---\n\n'));
  });

  it('replaces a retry at its UTF-8 byte offset without keeping the superseded answer', () => {
    const scratch = tmp();
    const report = new ReportTracker(scratch);
    const first = 'שלום '.repeat(2000);
    report.answer(first, 'stop');
    report.incoming('custom');
    report.answer('obsolete '.repeat(2000), 'stop');
    report.answer('replacement', 'stop');
    expect(fs.readFileSync(path.join(scratch, 'report.md'), 'utf8')).toBe(`${first}\n\n---\n\nreplacement`);
    report.incoming('custom');
    report.answer('replacement', 'stop');
    expect(fs.readFileSync(path.join(scratch, 'report.md'), 'utf8')).toBe(`${first}\n\n---\n\nreplacement`);
  });

  it('keeps a bounded answer when scratch is unavailable and sanitizes the disk report', () => {
    const scratch = tmp();
    const text = `\u001b[2J${'detail '.repeat(2000)}\u202e`;
    const disk = new ReportTracker(scratch);
    disk.answer(text, 'stop');
    expect(fs.readFileSync(path.join(scratch, 'report.md'), 'utf8')).toBe('detail '.repeat(2000));
    const unavailable = new ReportTracker(path.join(scratch, 'missing'));
    unavailable.answer(text, 'stop');
    expect(unavailable.result().text).not.toContain('Full report:');
    expect(Buffer.byteLength(unavailable.result().text)).toBeLessThan(9 * 1024);
  });
});
