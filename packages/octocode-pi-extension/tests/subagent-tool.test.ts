import fs from 'node:fs';
import path from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MAX_SUBAGENTS_ENV } from '../src/subagents/claim.js';
import { archiveSearch, existingLibrary, writeSegment } from '../src/shared/archive.js';
import { Team } from '../src/team/session.js';
import { registerCollab } from '../src/team/tools.js';
import { fakePi, rendered, theme } from './fake-pi.js';
import { tmp } from './helpers.js';
import { profiles, refused, reportOf, setup, useScriptedPi } from './subagent-tool-harness.js';

useScriptedPi();

describe('agent tool with a scripted pi child', () => {
  let s: ReturnType<typeof setup>;
  beforeEach(() => {
    s = setup();
  });

  it('runs a foreground subagent, streams progress and returns its final answer', async () => {
    const updates: Array<{ content: Array<{ type: string }>; details: { toolCalls: number; activity: string[]; shot?: string } }> = [];
    const result = await s.run({ task: 'report back\nsecond line', profile: 'researcher' }, undefined, (update) => updates.push(update as never));
    const text = result.content[0].text as string;
    const facts = reportOf(text);
    expect(facts.subagent).toBe('1');
    expect(facts.collaborate).toBeUndefined();
    expect(facts.id).toMatch(/^[a-z]+-[a-z]+$/);
    expect(facts).toMatchObject({ role: 'researcher', goal: 'check things' });
    expect(facts.parent).toBe(s.team.id);
    expect(facts.cwd).toBe(fs.realpathSync(s.cwd));
    // Each subagent gets a handoff folder in the workspace, which ignores itself in git.
    expect(facts.scratch).toBe(path.join(fs.realpathSync(s.cwd), '.octocode', 'tmp', 'agents', facts.id));
    // It saved nothing there, so the folder is removed when the run ends.
    expect(fs.existsSync(facts.scratch)).toBe(false);
    expect(fs.readFileSync(path.join(s.cwd, '.octocode', 'tmp', '.gitignore'), 'utf8')).toBe('*\n');
    expect(facts.args).toEqual(expect.arrayContaining(['--mode', 'rpc', '--session-dir', path.join(facts.scratch, 'session'), '--session-id', facts.id, '--model', 'faux/model', '--tools', 'read,bash', '--exclude-tools', 'file,edit,write']));
    expect(text).toContain('(Last screenshot: ');
    expect(result.details).toMatchObject({ profile: 'researcher', toolCalls: 1, input: 100, cacheRead: 0, output: 20, cost: 0.01, unpriced: false, model: 'faux/model' });
    // The child's own facts ride in its task (after the task text), not in its system prompt.
    const [given, ...you] = facts.task.split('\n\n## You\n');
    expect(given).toBe('report back\nsecond line');
    expect(you).toHaveLength(1);
    for (const fact of [facts.id, 'researcher', 'check things', s.team.id, facts.scratch]) expect(you[0]).toContain(fact);
    expect(result.usage.cost.total).toBeCloseTo(0.01);
    expect(fs.existsSync(result.details.shot)).toBe(true);
    expect(updates.some((update) => update.details.activity.includes('→ bash ls -la'))).toBe(true);
    const late = (await s.team.send(facts.id, 'one more thing'));
    // Its empty handoff folder is gone, so the receipt does not point at it.
    expect('error' in late && late.error).toContain(`${facts.id} has finished and takes no more messages; its report (or failure) already reached you. To continue`);
    expect(updates.some((update) => update.content.some((part) => part.type === 'image'))).toBe(true);

    const done = rendered(s.agent.renderResult(result, { isPartial: false, expanded: false }, theme, { isError: false, durationMs: 250 }));
    expect(done).toMatch(/^ {2}⎿ {2}Done \([a-z]+-[a-z]+ · 1 tool call · ↑100 ↓20 · ≈\$0\.01 · \d+(ms|\.\d+s|s) · faux\/model\)\n/);
    const partial = rendered(s.agent.renderResult({ content: [{ type: 'text', text: 'working' }], details: { ...result.details, seconds: undefined } }, { isPartial: true, expanded: false }, theme, { isError: false }));
    expect(partial).toContain('→ bash ls -la');
    expect(partial).toContain('📷');
    expect(partial).toMatch(/^ {2}⎿ {2}[a-z]+-[a-z]+ · 1 tool call · ↑100 ↓20 · ≈\$0\.01 · faux\/model\n/);
    const failed = rendered(s.agent.renderResult({ content: [{ type: 'text', text: 'Subagent x failed\ndetail' }] }, { isPartial: false, expanded: false }, theme, { isError: true }));
    // The text already says it failed: no `Error:` in front of it.
    expect(failed).toBe('  ⎿  Subagent x failed\n     detail');
    expect(rendered(s.agent.renderCall({ task: 'look\nmore', profile: 'researcher', goal: 'map the auth flow' }, theme, {}))).toBe('○ Agent(map the auth flow)');
    expect(rendered(s.agent.renderCall({ task: 'bg', background: true }, theme, { executionStarted: true, isPartial: false, isError: false }))).toMatch(/^● Agent · background/);
    expect(rendered(s.agent.renderCall({}, theme, {}))).toBe('○ Agent');
  });

  it('routes the model: profile preference, parent provider first, fallback to the parent\'s model, strict explicit', async () => {
    const run = async (params: Record<string, unknown>) => reportOf(((await s.run(params)).content[0].text as string));
    const modelArg = (facts: { args: string[] }) => facts.args[facts.args.indexOf('--model') + 1];
    profiles.set('scout', { name: 'scout', description: '', prompt: '', model: 'shared:high' });
    profiles.set('absent', { name: 'absent', description: '', prompt: '', model: 'claude-haiku-5-5' });
    try {
      // A bare id is resolved against the available models and keeps its :thinking suffix.
      expect(modelArg(await run({ task: 'report', profile: 'scout' }))).toBe('alt/shared:high');
      // An unavailable preference runs on the parent's current model, pinned with --model; the render and details say so.
      const fallback = await s.run({ task: 'report', profile: 'absent' });
      expect(modelArg(reportOf(fallback.content[0].text))).toBe('faux/model');
      expect(fallback.details).toMatchObject({ model: 'faux/model', fallback: 'claude-haiku-5-5' });
      expect(rendered(s.agent.renderResult(fallback, { isPartial: false, expanded: false }, theme, { isError: false }))).toContain('faux/model (claude-haiku-5-5 unavailable)');
      // The explicit parameter beats the profile and is strict.
      expect(modelArg(await run({ task: 'report', profile: 'scout', model: 'other' }))).toBe('faux/other');
      await expect(s.run({ task: 'report', profile: 'scout', model: 'faux/mdoel' })).rejects.toThrow(/Model "faux\/mdoel" is not available\. .*faux\/model, faux\/other/);
    } finally {
      profiles.delete('scout');
      profiles.delete('absent');
    }
  });

  it('reports a failed child, a provider error with its partial answer, and an empty answer', async () => {
    await expect(s.run({ task: 'fail now' })).rejects.toThrow(/Subagent exited with code 3: boom/);
    await expect(s.run({ task: 'narrate then crash' })).rejects.toThrow(/Subagent exited with code 3: segfault\nLast progress: Let me look at src\/a\.ts/);
    const failed = await s.run({ task: 'error please' });
    expect(failed).toMatchObject({ isError: true, details: { status: 'failed' }, usage: { input: 100, output: 20 } });
    expect(failed.content[0].text).toMatch(/failed: provider down\nRetry with a narrower task or do the work yourself\. The partial answer is unverified\.\n\nPartial answer:\npartial findings/);
    const limited = await s.run({ task: 'length please' });
    expect(limited).toMatchObject({ isError: true, details: { status: 'failed' }, usage: { input: 100, output: 20 } });
    expect(limited.content[0].text).toContain('Model output limit reached; the report is incomplete');
    expect(limited.content[0].text).toContain('Partial answer:\nunfinished findings');
    const silent = await s.run({ task: 'silent run' });
    expect(silent.content[0].text).toBe('(subagent returned no text)');
  });

  it("names the parent's archive library in the child's task, once the parent has one, and not before", async () => {
    const savedHome = process.env['OCTOCODE_HOME'];
    process.env['OCTOCODE_HOME'] = tmp();
    try {
      const s = setup();
      const line = (task: string) => task.split('\n').filter((text) => text.startsWith('Earlier history of your parent session'));
      const without = reportOf((await s.run({ task: 'report back' })).content[0].text as string);
      expect(line(without.task)).toEqual([]);
      writeSegment({ session: 'parent', entry: 'c1', tokensBefore: 1, method: 'pi-default', tldr: 'earlier work', topics: [], readFiles: [], modifiedFiles: [], messages: 1 }, 'body');
      const library = existingLibrary()!;
      const earlier = `Earlier history of your parent session, compacted out of its context, is archived in ${path.dirname(library)} (read-only; LIBRARY.md indexes it; the repository shows today's state, the archive what was said and found then). ${archiveSearch()}`;
      const withLibrary = reportOf((await s.run({ task: 'report back' })).content[0].text as string);
      // One line after the identity, in the task and not in the system prompt: the child's facts stay the same shape.
      expect(line(withLibrary.task)).toEqual([earlier]);
      expect(withLibrary.task.startsWith('report back\n\n## You\nYour name is')).toBe(true);
      expect(withLibrary.task.endsWith(earlier)).toBe(true);
      expect(withLibrary.args.join(' ')).not.toContain(library);
      // Every child keeps the archive tool (a core tool), so a profile without MCP servers gets the same line.
      profiles.set('offline', { name: 'offline', description: 'no MCP', prompt: 'No servers.', mcp: false });
      const offline = reportOf((await s.run({ task: 'report back', profile: 'offline' })).content[0].text as string);
      expect(line(offline.task)).toEqual([earlier]);
    } finally {
      profiles.delete('offline');
      if (savedHome === undefined) delete process.env['OCTOCODE_HOME'];
      else process.env['OCTOCODE_HOME'] = savedHome;
    }
  });

  it('still runs the child, alone, when the team database is unusable', async () => {
    const s = setup();
    vi.spyOn(s.team, 'join').mockImplementation(() => {
      throw new Error('Team database unavailable: locked');
    });
    const result = await s.run({ task: 'report back', collaborate: true });
    const text = result.content[0].text as string;
    const facts = reportOf(text);
    expect(facts.parent).toBeUndefined();
    expect(facts.collaborate).toBeUndefined();
    expect(text).toContain('Started without the team: Team database unavailable: locked');
  });

  it('shows terminal escapes and bidi controls in the child\'s report, error and stderr as visible escapes', async () => {
    await expect(s.run({ task: 'hostile fail' })).rejects.toThrow('Subagent exited with code 3: \\u{1b}]0;pwned\\u{7}boom\\u{1b}[31m red\\u{202e}');
    const error = (await s.run({ task: 'hostile report' })).content[0].text as string;
    expect(error).toContain('failed: bad\\u{1b}]0;title\\u{7} provider\nRetry with a narrower task or do the work yourself. The partial answer is unverified.\n\nPartial answer:\n\\u{1b}]8;;https://evil.example\\u{7}click\\u{1b}]8;;\\u{7} \\u{1b}[2Jdone\\u{202e}');
    expect(error).not.toMatch(/[\u001b\u0007\u202e]/u);
  });

  it('hands a task starting with - or @ to the child as its message, never as an argument', async () => {
    for (const task of ['- report the dash', '@src/a.ts report']) {
      const facts = reportOf((await s.run({ task })).content[0].text as string);
      expect(facts.task.trim().startsWith(task)).toBe(true);
      expect(facts.args).not.toContain(task);
      expect(facts.args).not.toContain('--');
    }
  });

  it('keeps a long report short in context and saves it whole as a doc', async () => {
    const result = await s.run({ task: 'long report' });
    const text = result.content[0].text as string;
    const file = path.join(fs.realpathSync(s.cwd), '.octocode', 'tmp', 'agents', result.details.id, 'report.md');
    expect(text).toMatch(/^summary first\n/);
    expect(text).toContain('[Output trimmed:');
    expect(text).toContain(`Full report: ${file} (read only the parts you need).`);
    expect(Buffer.byteLength(text)).toBeLessThan(9 * 1024);
    expect(fs.readFileSync(file, 'utf8')).toMatch(/the end$/);
    // A short report is not written to disk.
    const short = await s.run({ task: 'report' });
    expect(fs.existsSync(path.join(s.cwd, '.octocode', 'tmp', 'agents', short.details.id, 'report.md'))).toBe(false);
  });

  it('names why a profile was refused at load when the unknown profile is that one', async () => {
    refused.set('blind', 'Profile "blind" removes sendMessage with excludeTools. Remove it from the profile\'s excludeTools.');
    try {
      await expect(s.run({ task: 'x', profile: 'blind' })).rejects.toThrow(/Unknown profile "blind"\. Available: researcher, webLive\n"blind" was refused when profiles loaded: Profile "blind" removes sendMessage/);
      // A name that was never a profile file gets no reason.
      await expect(s.run({ task: 'x', profile: 'nope' })).rejects.not.toThrow(/refused/);
    } finally {
      refused.clear();
    }
  });

  it('refuses unknown profiles and makes runs over the limit wait for a slot', async () => {
    await expect(s.run({ task: 'x', profile: 'nope' })).rejects.toThrow(/Unknown profile "nope". Available: researcher, webLive/);
    process.env[MAX_SUBAGENTS_ENV] = '1';
    try {
      const controller = new AbortController();
      const first = s.run({ task: 'slow one' }, controller.signal);
      // The second waits, and its row says why; a third gives up its wait when its turn is aborted.
      const updates: Array<{ content: Array<{ text: string }>; details: { status: string } }> = [];
      const second = s.run({ task: 'report' }, undefined, (update) => updates.push(update as never));
      const gaveUp = new AbortController();
      const third = s.run({ task: 'report' }, gaveUp.signal);
      await vi.waitFor(() => expect(updates[0]).toMatchObject({ content: [{ text: 'waiting for a slot (1 running)' }], details: { status: 'waiting' } }));
      const waiting = rendered(s.agent.renderResult(updates[0], { isPartial: true, expanded: false }, theme, { args: { task: 'report' }, isError: false }));
      expect(waiting).toContain('waiting for a slot (1 running)');
      gaveUp.abort();
      await expect(third).rejects.toThrow('Cancelled while waiting for a subagent slot.');
      controller.abort();
      await expect(first).rejects.toThrow('Subagent cancelled');
      // The slot the cancelled run gave back starts the waiting one.
      await expect(second).resolves.toMatchObject({ details: { status: 'done' } });
    } finally {
      delete process.env[MAX_SUBAGENTS_ENV];
    }
    await expect(s.run({ task: 'report' })).resolves.toMatchObject({ details: { status: 'done' } });
  });

});

describe('agent tool with a scripted pi child, in the background', () => {
  let s: ReturnType<typeof setup>;
  beforeEach(() => {
    s = setup();
  });

  it('runs in the background and delivers its fenced answer as a follow-up message', async () => {
    const started = await s.run({ task: 'report from the background', background: true });
    expect(started.content[0].text).toMatch(/^Started [a-z]+-[a-z]+ \(general: check things\) in the background/);
    expect(started.content[0].text).toMatch(/its report arrives as a message\.$/);
    await vi.waitFor(() => expect(s.sent.length).toBeGreaterThan(0), { timeout: 10_000 });
    const answer = s.sent[0]!;
    expect(answer.options).toEqual({ triggerTurn: true, deliverAs: 'followUp' });
    expect(answer.message.content).toMatch(/^Background subagent [a-z]+-[a-z]+ \(general: check things\) finished \(\d+s · 1 tool call · ≈\$0\.01\):\n<<<untrusted-[0-9a-f]{16} \(data from the sender[^\n]*\)\nreport /);
    // The child's words end inside the fence; the footer after it is the parent's.
    expect(String(answer.message.content)).toMatch(/\nuntrusted-[0-9a-f]{16}>>>\n\(Last screenshot: /);
    const renderer = s.renderers.get('octocode-agent-result');
    // The report starts with no outcome word: its outcome is unknown, which is not a failure.
    expect(answer.message.details).not.toHaveProperty('outcome');
    expect(answer.message.details).toMatchObject({ status: 'done', summary: expect.stringMatching(/^Done \([a-z]+-[a-z]+ · 1 tool call · ↑100 ↓20 · ≈\$0\.01 · /) });
    const shown = rendered(renderer(answer.message, { expanded: false }, theme));
    expect(shown).toMatch(/^ ● Agent\([a-z]+-[a-z]+ · check things\) · background · \S+\n {3}⎿ {2}Done \([a-z]+-[a-z]+ · 1 tool call/);
    expect(shown).toContain('report');
  });

  it('reports a background failure with its error inside the fence and a retry hint', async () => {
    const renderer = s.renderers.get('octocode-agent-result');
    await s.run({ task: 'error in background', background: true });
    await vi.waitFor(() => expect(s.sent.length).toBe(1), { timeout: 10_000 });
    expect(s.sent[0]!.message.content).toMatch(/failed:\n<<<untrusted-[0-9a-f]{16} [\s\S]*\nError: provider down[\s\S]*>>>\nRetry with a narrower task or do the work yourself/);
    expect(s.sent[0]!.message.details).toMatchObject({ status: 'failed', outcome: 'failed', summary: 'Failed: provider down' });
    expect(rendered(renderer(s.sent[0]!.message, { expanded: false }, theme))).toMatch(/^ ✗ Agent\([a-z]+-[a-z]+[^\n]*\n {3}⎿ {2}Failed: provider down/);
  });

  it('/agents kill completes running ids, refuses unknown ones, and cancels a background run, which settle() waits for', async () => {
    const renderer = s.renderers.get('octocode-agent-result');
    const kill = { handler: (args: string, ctx: unknown) => s.commands.get('agents').handler(`kill ${args}`, ctx), getArgumentCompletions: (prefix: string) => s.commands.get('agents').getArgumentCompletions(`kill ${prefix}`) };
    await kill.handler('', s.ctx);
    expect(s.ctx.ui.notes.at(-1)).toEqual({ message: 'Usage: /agents kill <id>. None is running.', type: 'warning' });
    const slow = await s.run({ task: 'slow background', background: true });
    const id = slow.details.id as string;
    expect(await kill.getArgumentCompletions(id.slice(0, 3))).toEqual([expect.objectContaining({ value: `kill ${id}`, label: id })]);
    await kill.handler('ghost', s.ctx);
    expect(s.ctx.ui.notes.at(-1)!.message).toBe(`No running subagent "ghost". Running: ${id}`);
    // settle() resolves once every background subagent has reported (index.ts holds a headless run on it).
    let settled = false;
    const waiting = s.control.settle().then(() => (settled = true));
    // No fixed sleep: flush pending callbacks; settle cannot resolve before the kill below.
    await new Promise((resolve) => setImmediate(resolve));
    expect(settled).toBe(false);
    await kill.handler(id, s.ctx);
    expect(s.ctx.ui.notes.at(-1)).toEqual({ message: `Stopping ${id}…`, type: 'info' });
    await waiting;
    await vi.waitFor(() => expect(s.sent.length).toBe(1), { timeout: 10_000 });
    expect(s.sent[0]!.message.content).toBe(`Background subagent ${id} (general: check things) cancelled by the user`);
    // A stop the user asked for is an interruption, not a failure; the prompt is one expand away.
    expect(rendered(renderer(s.sent[0]!.message, { expanded: false }, theme))).toMatch(/^ ◼ Agent.*\n {3}⎿ {2}Cancelled by the user\n {6}… prompt \(ctrl\+o to expand\)$/);
    expect(s.sent[0]!.options).toEqual({ triggerTurn: false, deliverAs: 'followUp' });
  });

  it('lets the parent agent stop only the background subagents it started, with coordinate stop', async () => {
    registerCollab(s.pi, s.team, { stopAgent: (id, by) => s.control.stop(id, by) });
    const coordinate = s.tools.get('coordinate');
    const stop = (id?: string) => coordinate.execute('c1', { action: 'stop', ...(id ? { id } : {}) }, undefined, undefined, s.ctx);
    await expect(stop('ghost')).rejects.toThrow('No background subagent "ghost". None is running.');
    const slow = await s.run({ task: 'slow background', background: true });
    const id = slow.details.id as string;
    expect((await stop(id)).content[0].text).toBe(`Stopping ${id}: its process tree is killed and its report arrives as a message.`);
    await vi.waitFor(() => expect(s.sent.at(-1)?.message.content).toBe(`Background subagent ${id} (general: check things) stopped by ${s.team.id}`), { timeout: 10_000 });
    // The parent asked for the stop: the report is queued without waking it for another turn.
    expect(s.sent.at(-1)!.options).toEqual({ triggerTurn: false, deliverAs: 'followUp' });
    await expect(stop(id)).rejects.toThrow(/No background subagent/);
    // A subagent has no children to stop: `stop` is not offered to it at all (a stray call is still refused).
    const child = fakePi();
    registerCollab(child.pi, new Team(child.pi));
    const childTool = child.tools.get('coordinate');
    expect(childTool.parameters.properties.action.enum).not.toContain('stop');
    expect(childTool.parameters.properties.id).toBeUndefined();
    expect(childTool.description).not.toContain('`stop`');
    await expect(childTool.execute('c2', { action: 'stop', id }, undefined, undefined, s.ctx)).rejects.toThrow(/Only the agent that started a subagent can stop it/);
  });

  it('stops background runs at session shutdown and waits for them to file their reports', async () => {
    const slow = await s.run({ task: 'slow at shutdown', background: true });
    await s.emit('session_shutdown', {}, s.ctx);
    // Shutdown returns only after the stopped child exited and its report was filed.
    expect(s.sent.at(-1)?.message.content).toBe(`Background subagent ${slow.details.id} (general: check things) stopped by session shutdown`);
    expect(s.sent.at(-1)!.options.triggerTurn).toBe(false);
  });

  it('tells parallel collaborating subagents about each other', async () => {
    const first = await s.run({ task: 'slow collaborator', goal: 'slow collaborator', background: true, collaborate: true });
    const second = await s.run({ task: 'report as collaborator', collaborate: true });
    const facts = reportOf(second.content[0].text);
    expect(facts.collaborate).toBe('1');
    expect(facts.task).toContain('## Teammates');
    expect(facts.task).toContain(`\`${first.details.id}\`: general: slow collaborator`);
    await s.commands.get('agents').handler(`kill ${first.details.id}`, s.ctx);
    await vi.waitFor(() => expect(s.sent.at(-1)?.message.content).toContain('cancelled by the user'), { timeout: 10_000 });
  });
});
