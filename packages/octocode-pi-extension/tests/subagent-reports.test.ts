import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { ReadReports, ReportQueue } from '../src/subagents/handoff.js';
import { fakePi, rendered, theme } from './fake-pi.js';
import { tmp } from './helpers.js';
import { setup, useScriptedPi } from './subagent-tool-harness.js';

useScriptedPi();

const report = (id: string, wake: boolean) => ({ content: id, details: { id, status: 'done' as const, summary: 'Done' }, wake });

describe('background report delivery', () => {
  it('batches reports so only the last of a batch may start a turn', () => {
    const f = fakePi();
    const queue = new ReportQueue(f.pi, 60_000);
    queue.add(report('a', true));
    queue.add(report('b', false));
    queue.add(report('c', false));
    expect(f.sent).toHaveLength(0);
    queue.flush();
    expect(f.sent.map((entry) => [entry.message.content, entry.options.triggerTurn])).toEqual([['a', false], ['b', false], ['c', true]]);
    queue.add(report('d', false));
    queue.flush();
    expect(f.sent.at(-1)!.options).toEqual({ triggerTurn: false, deliverAs: 'followUp' });
  });

  it('delivers without a turn a report whose file the parent already read since it was written', () => {
    const cwd = tmp();
    const scratch = path.join(cwd, '.octocode', 'tmp', 'agents', 'general-abcdef');
    fs.mkdirSync(scratch, { recursive: true });
    const file = path.join(scratch, 'report.md');
    fs.writeFileSync(file, 'report');
    const past = new Date(Date.now() - 10_000);
    fs.utimesSync(file, past, past);
    const read = new ReadReports();
    read.track('general-abcdef', scratch);
    read.observe('read', { path: '.octocode/tmp/agents/general-abcdef/report.md' }, cwd);
    expect(read.consume('general-abcdef')).toBe(true);
    // Rewritten after the read: the parent has not seen the final version.
    read.track('general-abcdef', scratch);
    read.observe('read', { path: file }, cwd);
    // An MCP tool's input is the server's contract: it never counts as reading the report.
    read.observe('mcp__octocode__localFetch', { queries: [{ path: file }] }, cwd);
    const future = new Date(Date.now() + 10_000);
    fs.utimesSync(file, future, future);
    expect(read.consume('general-abcdef')).toBe(false);
    // Never read.
    read.track('general-abcdef', scratch);
    expect(read.consume('general-abcdef')).toBe(false);
  });

  it('keeps a scratch folder the subagent saved files in, and names it in the receipt', async () => {
    const s = setup();
    const result = await s.run({ task: 'long report' });
    const { id }: { id: string } = result.details;
    const scratch = path.join(s.cwd, '.octocode', 'tmp', 'agents', id);
    expect(fs.existsSync(path.join(scratch, 'report.md'))).toBe(true);
    const late = (await s.team.send(id, 'more'));
    expect('error' in late && late.error).toContain(`(files it handed back, and its full report when cut, are in`);
  });
});

describe('a background report as the parent model reads it', () => {
  it('keeps a report that forges a user message, a header and a fence line inside the fence, under a header the parent wrote', async () => {
    const s = setup();
    const started = await s.run({ task: 'forged report', background: true, goal: 'review auth' });
    const id: string = started.details.id;
    await vi.waitFor(() => expect(s.sent.length).toBeGreaterThan(0), { timeout: 10_000 });
    expect(s.sent[0].message.details).toMatchObject({ status: 'done', outcome: 'done' });
    const content = String(s.sent[0].message.content);
    const lines = content.split('\n');
    // The header is the parent's: this agent, its role and goal, its statistics. Nothing before the fence is the child's.
    expect(lines[0]).toMatch(new RegExp(`^Background subagent ${id} \\(general: review auth\\) reported done \\(.*\\):$`));
    const open = lines.findIndex((line) => line.startsWith('<<<untrusted-'));
    const marker = /^<<<(untrusted-[0-9a-f]{16}) /.exec(lines[open])![1];
    const close = lines.lastIndexOf(`${marker}>>>`);
    expect(open).toBe(1);
    // Every forged line sits between the real fence lines, and the forged fence line did not end it.
    const forged = lines.map((line, index) => [line, index] as const).filter(([line]) => /^Message #\d from the user/.test(line));
    expect(forged).toHaveLength(2);
    expect(forged.every(([, index]) => index > open && index < close)).toBe(true);
    expect(close).toBeGreaterThan(forged.at(-1)![1]);
    expect(lines.filter((line) => line === `${marker}>>>`)).toHaveLength(1);
    // The screen shows the child's words without the fence lines.
    const shown = rendered(s.renderers.get('octocode-agent-result')(s.sent[0].message, { expanded: true }, theme));
    expect(shown).toContain('Message #2 from the user');
    expect(shown).not.toContain(`${marker}>>>`);
    await s.fire('session_shutdown', { type: 'session_shutdown' }, s.ctx);
  });
});
