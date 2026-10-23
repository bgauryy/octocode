import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { registerBashTool } from '../src/files/bash.js';
import { BashJobs } from '../src/files/bash-jobs.js';
import { jobReport, logTail } from '../src/files/bash-report.js';
import { RERUN_HINT } from '../src/files/render.js';
import { Subcommands } from '../src/shared/commands.js';
import { fakePi, rendered, theme } from './fake-pi.js';
import { tmp } from './helpers.js';

describe('bash job report', () => {
  it('reports a finished job with its last lines and counts the earlier ones the log keeps', () => {
    const dir = tmp();
    const log = path.join(dir, 'job.log');
    fs.writeFileSync(log, `${'x'.repeat(5000)}\nlast line\n`);
    expect(logTail(log)).toEqual({ text: 'last line', earlier: 1 });
    expect(logTail(path.join(dir, 'missing.log'))).toEqual({ text: '', earlier: 0 });
    const job = { id: 'bash-9', command: 'make', cwd: dir, pid: 1, log, startedAt: 0, done: Promise.resolve({ code: 0, signal: null, seconds: 75 }) };
    expect(jobReport(job, { code: 0, signal: null, seconds: 75 })).toBe(`Background bash bash-9 finished after 1m15s: make\nLog: job.log\n[+1 earlier line in the log]\nlast line`);
    // One rotation keeps the earlier output in `.1`; nothing is said to be dropped.
    expect(jobReport(job, { code: 0, signal: null, seconds: 75, logRotations: 1 })).toBe(`Background bash bash-9 finished after 1m15s: make\nLog: job.log\nlog rotated at its size cap: job.log.1 holds the part before this log\n[+1 earlier line in the log]\nlast line`);
    // The tail is the end of the log (not the start of its last few KB), and nothing before it goes uncounted.
    fs.writeFileSync(log, Array.from({ length: 100 }, (_, index) => `line ${index + 1}`).join('\n'));
    const report = jobReport(job, { code: 0, signal: null, seconds: 1 }).split('\n');
    expect(report.slice(2, 4)).toEqual(['[+60 earlier lines in the log]', 'line 61']);
    expect(report.at(-1)).toBe('line 100');
  });

  it('drops colors from the reported log tail and shows other terminal escapes as visible escapes', () => {
    const dir = tmp();
    const log = path.join(dir, 'job.log');
    fs.writeFileSync(log, '\u001b[32mPASS\u001b[0m tests\r\n\u001b]0;title\u0007done\n');
    const job = { id: 'bash-8', command: 'yarn test', cwd: dir, pid: 1, log, startedAt: 0, done: Promise.resolve({ code: 0, signal: null, seconds: 1 }) };
    expect(jobReport(job, { code: 0, signal: null, seconds: 1 }).split('\n').slice(2)).toEqual(['PASS tests\r', '\\u{1b}]0;title\\u{7}done']);
  });
});

const setup = () => {
  const { pi, tools, renderers } = fakePi();
  registerBashTool(pi, new Subcommands(), new BashJobs(tmp()));
  return { tool: tools.get('bash'), renderers };
};
const done = (extra: Record<string, unknown> = {}) => ({ lastComponent: undefined, isPartial: false, isError: false, state: {}, ...extra });

describe('bash rendering', () => {
  it('heads the call with the command and shows a timeout only when the model set one', () => {
    const { tool } = setup();
    expect(rendered(tool.renderCall({ command: 'yarn test' }, theme, { lastComponent: undefined, isPartial: true, executionStarted: false }))).toBe('○ Bash(yarn test)');
    expect(rendered(tool.renderCall({ command: 'sleep 1', timeout: 30 }, theme, { lastComponent: undefined, isPartial: true }))).toBe('○ Bash(sleep 1) · timeout 30s');
    expect(rendered(tool.renderCall({ command: 'serve', background: true }, theme, done({ durationMs: 40 })))).toBe('● Bash(serve) · background · 40ms');
  });

  it('summarizes exit, line count and the last lines of output', () => {
    const { tool } = setup();
    const text = Array.from({ length: 12 }, (_, index) => `line ${index + 1}`).join('\n');
    const lines = rendered(tool.renderResult({ content: [{ type: 'text', text }] }, { expanded: false, isPartial: false }, theme, done({ durationMs: 5 }))).split('\n');
    expect(lines).toEqual(['  ⎿  exit 0 · 12 lines', '     … +9 lines (ctrl+o to expand)', '     line 10', '     line 11', '     line 12']);
    const expanded = rendered(tool.renderResult({ content: [{ type: 'text', text }] }, { expanded: true, isPartial: false }, theme, done()));
    expect(expanded.split('\n')).toHaveLength(13);
    expect(rendered(tool.renderResult({ content: [{ type: 'text', text: '(no output)' }] }, { expanded: false, isPartial: false }, theme, done()))).toBe('  ⎿  exit 0 · no output');
  });

  it('reads only the tail of a collapsed output, with the same counts as the whole', () => {
    const { tool } = setup();
    const many = Array.from({ length: 500 }, (_, index) => `line ${index + 1}`).join('\n');
    const texts = [
      `\n\n${many}\n\nCommand exited with code 0`,
      `head\n\n\n${many}\n\n\n`,
      `a\rprogress 50%\rprogress 100%\n\u2028${many}\n\u001b[31mred\u001b[0m\n   \n`,
      `${many}\n\n[Showing lines 1-500 of 9000. Full output: /tmp/out.log]\n\nCommand exited with code 0`,
      'x\n\n\n\ny\n\n\nz',
    ];
    for (const text of texts) {
      for (const isPartial of [true, false]) {
        const view = (expanded: boolean) => rendered(tool.renderResult({ content: [{ type: 'text', text }] }, { expanded, isPartial }, theme, done())).split('\n');
        const collapsed = view(false);
        const expanded = view(true);
        expect(collapsed[0].replace(' (ctrl+o to expand)', '')).toBe(expanded[0]);
        const hidden = Number(/\+(\d+) lines?/.exec(collapsed[1])?.[1] ?? 0);
        const body = expanded.slice(1).filter((line) => !line.trimStart().startsWith('saved:'));
        expect(hidden + collapsed.slice(2).length).toBe(body.length);
        expect(collapsed.slice(-3)).toEqual(body.filter((line) => line.trim() !== '').slice(-3));
      }
    }
  });

  it('draws failures as errors and keeps the spill path', () => {
    const { tool } = setup();
    const text = 'a\nb\n\n[Showing lines 1-2 of 900. Full output: /tmp/out.log]\n\nCommand exited with code 2';
    const lines = rendered(tool.renderResult({ content: [{ type: 'text', text }] }, { expanded: false, isPartial: false }, theme, done({ isError: true }))).split('\n');
    expect(lines).toEqual(['  ⎿  Error: exit 2 · 900 lines (ctrl+o to expand)', '     a', '     b']);
    expect(rendered(tool.renderResult({ content: [{ type: 'text', text }] }, { expanded: true, isPartial: false }, theme, done({ isError: true })))).toMatch(/\n {5}saved: \/tmp\/out\.log$/);
    const timeout = rendered(tool.renderResult({ content: [{ type: 'text', text: `Command timed out after 120 seconds.\n${RERUN_HINT} with a longer timeout, or with background: true and check its log.` }] }, { expanded: false, isPartial: false }, theme, done({ isError: true })));
    expect(timeout).toBe('  ⎿  Error: timed out after 2m · no output');
    const killed = rendered(tool.renderResult({ content: [{ type: 'text', text: 'build line\n\nCommand terminated without an exit code' }] }, { expanded: false, isPartial: false }, theme, done({ isError: true })));
    expect(killed.split('\n')[0]).toBe('  ⎿  Error: terminated (no exit code) · 1 line');
    expect(rendered(tool.renderResult({ content: [{ type: 'text', text: 'Working directory does not exist' }] }, { expanded: false, isPartial: false }, theme, done({ isError: true })))).toBe('  ⎿  Error: Working directory does not exist');
  });

  it('shows a started job and a running tail', () => {
    const { tool, renderers } = setup();
    expect(rendered(tool.renderResult({ content: [{ type: 'text', text: 'Started' }], details: { job: 'bash-1', pid: 42, log: '/tmp/j.log' } }, { expanded: false, isPartial: false }, theme, done()))).toBe('  ⎿  job bash-1 started (pid 42) (ctrl+o to expand)');
    expect(rendered(tool.renderResult({ content: [{ type: 'text', text: 'Started' }], details: { job: 'bash-1', pid: 42, log: '/tmp/j.log' } }, { expanded: true, isPartial: false }, theme, done()))).toBe('  ⎿  job bash-1 started (pid 42)\n     log: /tmp/j.log');
    expect(rendered(tool.renderResult({ content: [{ type: 'text', text: 'one\ntwo' }] }, { expanded: false, isPartial: true }, theme, { lastComponent: undefined, isPartial: true, executionStarted: true, state: {} }))).toBe('  ⎿  running · 2 lines\n     one\n     two');
    const job = [...renderers.values()][0];
    const message = { content: 'Background bash bash-1 finished after 3s: yarn build\nLog: /tmp/j.log\nok' };
    expect(rendered(job(message, { expanded: false }, theme))).toBe(' ● Job(bash-1: yarn build)\n   ⎿  finished after 3s (ctrl+o to expand)\n      ok');
    expect(rendered(job(message, { expanded: true }, theme))).toBe(' ● Job(bash-1: yarn build)\n   ⎿  finished after 3s\n      ok\n      log: /tmp/j.log');
    // A job the user killed is interrupted, not failed.
    const colored = { ...(theme as object), fg: (color: string, text: string) => `<${color}>${text}` };
    const killed = rendered(job({ content: 'Background bash bash-1 stopped (user) after 3s: yarn build\nLog: /tmp/j.log\n(no output)' }, { expanded: false }, colored));
    expect(killed).toContain('<warning>◼');
    expect(killed).not.toContain('<error>');
  });
});
