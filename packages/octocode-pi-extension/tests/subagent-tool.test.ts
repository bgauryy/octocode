import { execFileSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { exitReason, runSubagent, subagentProcessEnv, watchdogLimits } from '../src/subagents/process.js';
import { ReadReports, ReportQueue } from '../src/subagents/handoff.js';
import { MAX_SUBAGENTS_ENV, registerAgentTool } from '../src/subagents/tool.js';
import { Subcommands } from '../src/shared/commands.js';
import { createWorktree } from '../src/subagents/worktree.js';
import { Team } from '../src/team/session.js';
import { registerCollab } from '../src/team/tools.js';
import { fakeCtx, fakePi, rendered, theme } from './fake-pi.js';
import { tmp } from './helpers.js';

const FIXTURE = path.join(path.dirname(new URL(import.meta.url).pathname), 'subagent-fake-pi.cjs');
const git = (cwd: string, ...args: string[]) => execFileSync('git', args, { cwd, encoding: 'utf8' }).trim();
const savedArgv = process.argv[1];
const savedEnv = { ...process.env };

function repo(): string {
  const dir = fs.realpathSync(tmp('octocode-agent-repo-'));
  git(dir, 'init', '-q', '-b', 'main');
  git(dir, 'config', 'user.name', 'Test');
  git(dir, 'config', 'user.email', 'test@example.com');
  fs.writeFileSync(path.join(dir, 'a.txt'), 'one\n');
  git(dir, 'add', '-A');
  git(dir, 'commit', '-q', '-m', 'init');
  return dir;
}

beforeAll(() => {
  const root = tmp('octocode-agent-tool-');
  process.env['OCTOCODE_HOME'] = path.join(root, 'home');
  process.env['OCTOCODE_AGENT_DB'] = path.join(root, 'team', 'team.sqlite');
  // Tests may run inside an Octocode subagent: start from a clean team identity.
  for (const key of [MAX_SUBAGENTS_ENV, 'OCTOCODE_SUBAGENT_COLLABORATE', 'OCTOCODE_SUBAGENT', 'OCTOCODE_AGENT_ID', 'OCTOCODE_PARENT_ID', 'OCTOCODE_AGENT_TASK', 'OCTOCODE_AGENT_COLLABORATE', 'OCTOCODE_AGENT_SCRATCH', 'OCTOCODE_TEAM_WORKSPACE']) delete process.env[key];
  // runSubagent re-runs the current `pi` script: point it at the scripted stand-in.
  const bin = path.join(root, 'bin');
  fs.mkdirSync(bin);
  fs.copyFileSync(FIXTURE, path.join(bin, 'pi'));
  process.argv[1] = path.join(bin, 'pi');
});

afterAll(() => {
  process.argv[1] = savedArgv!;
  process.env = savedEnv;
});

const profiles = new Map([
  ['researcher', { name: 'researcher', description: 'reads', prompt: 'Research only.', tools: 'read,bash', excludeTools: 'file', model: 'faux/model' }],
  ['webLive', { name: 'webLive', description: 'browser', prompt: 'Drive the browser.', visibleBrowser: true }],
]);

function setup(cwd = tmp('octocode-agent-cwd-'), hasUI = true) {
  const f = fakePi();
  const team = new Team(f.pi);
  const control = registerAgentTool(f.pi, () => profiles, team);
  // As index.ts wires it: `/octocode agents`, with the `/agents` shortcut.
  const commands = new Subcommands();
  commands.add('agents', control.command);
  commands.alias(f.pi, 'agents');
  const ctx = fakeCtx({ cwd, hasUI });
  team.start(ctx);
  const agent = f.tools.get('agent');
  const run = (params: Record<string, unknown>, signal?: AbortSignal, onUpdate?: (update: unknown) => void) => agent.execute('call-1', params, signal, onUpdate, ctx);
  return { ...f, team, ctx, agent, run, cwd, control };
}

const reportOf = (text: string) => JSON.parse(text.slice(text.indexOf('{'), text.indexOf('}\n') >= 0 ? text.indexOf('}\n') + 1 : text.lastIndexOf('}') + 1));

describe('subagent scratch sweep', () => {
  it('keeps old folders whose owning Pi process is alive (another session\'s background subagent) and removes the rest', async () => {
    const s = setup();
    const root = path.join(s.cwd, '.octocode', 'tmp', 'agents');
    const old = (Date.now() - 8 * 24 * 3_600_000) / 1000;
    const folder = (name: string, owner?: number) => {
      const dir = path.join(root, name);
      fs.mkdirSync(dir, { recursive: true });
      if (owner !== undefined) fs.writeFileSync(path.join(dir, '.owner'), String(owner));
      fs.utimesSync(dir, old, old);
      return dir;
    };
    const otherSession = folder('worker-live', process.ppid);
    const dead = folder('worker-dead', 9_999_999);
    const unowned = folder('worker-unowned');
    const fresh = path.join(root, 'worker-fresh');
    fs.mkdirSync(fresh);
    await s.fire('session_start', { type: 'session_start' }, s.ctx);
    await new Promise((resolve) => setImmediate(resolve));
    expect(fs.existsSync(otherSession)).toBe(true);
    expect(fs.existsSync(fresh)).toBe(true);
    expect(fs.existsSync(dead)).toBe(false);
    expect(fs.existsSync(unowned)).toBe(false);
    await s.fire('session_shutdown', { type: 'session_shutdown' }, s.ctx);
  });
});

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
    expect(facts.id).toMatch(/^researcher-[0-9a-f]{6}$/);
    expect(facts.parent).toBe(s.team.id);
    expect(facts.cwd).toBe(fs.realpathSync(s.cwd));
    // Each subagent gets a handoff folder in the workspace, which ignores itself in git.
    expect(facts.scratch).toBe(path.join(s.cwd, '.octocode', 'tmp', 'agents', facts.id));
    // It saved nothing there, so the folder is removed when the run ends.
    expect(fs.existsSync(facts.scratch)).toBe(false);
    expect(fs.readFileSync(path.join(s.cwd, '.octocode', 'tmp', '.gitignore'), 'utf8')).toBe('*\n');
    expect(facts.args).toEqual(expect.arrayContaining(['--mode', 'json', '--no-session', '--model', 'faux/model', '--tools', 'read,bash', '--exclude-tools', 'file,edit,write']));
    expect(text).toContain('(Last screenshot: ');
    expect(result.details).toMatchObject({ profile: 'researcher', toolCalls: 1, input: 100, output: 20 });
    expect(result.usage.cost.total).toBeCloseTo(0.01);
    expect(fs.existsSync(result.details.shot)).toBe(true);
    expect(updates.some((update) => update.details.activity.includes('→ bash ls -la'))).toBe(true);
    const late = s.team.send(facts.id, 'one more thing');
    // Its empty handoff folder is gone, so the receipt does not point at it.
    expect('error' in late && late.error).toContain(`${facts.id} has finished and takes no more messages; its report (or failure) already reached you. To continue`);
    expect(updates.some((update) => update.content.some((part) => part.type === 'image'))).toBe(true);

    const done = rendered(s.agent.renderResult(result, { isPartial: false, expanded: false }, theme, { isError: false }));
    expect(done).toMatch(/^ {2}⎿ {2}Done \(1 tool call · 120 tokens · \d+(ms|\.\d+s|s)\)\n/);
    const partial = rendered(s.agent.renderResult({ content: [{ type: 'text', text: 'working' }], details: { ...result.details, seconds: undefined } }, { isPartial: true, expanded: false }, theme, { isError: false }));
    expect(partial).toContain('→ bash ls -la');
    expect(partial).toContain('📷');
    expect(partial).toMatch(/^ {2}⎿ {2}researcher-[0-9a-f]{6} · 1 tool call · ↑100 ↓20\n/);
    const failed = rendered(s.agent.renderResult({ content: [{ type: 'text', text: 'Subagent x failed\ndetail' }] }, { isPartial: false, expanded: false }, theme, { isError: true }));
    // The text already says it failed: no `Error:` in front of it.
    expect(failed).toBe('  ⎿  Subagent x failed\n     detail');
    expect(rendered(s.agent.renderCall({ task: 'look\nmore', profile: 'researcher' }, theme, {}))).toBe('○ Agent(researcher)');
    expect(rendered(s.agent.renderCall({ task: 'bg', background: true }, theme, { executionStarted: true, isPartial: false, isError: false }))).toMatch(/^● Agent\(general\) · background/);
    expect(rendered(s.agent.renderCall({}, theme, {}))).toBe('○ Agent(general)');
  });

  it('reports a failed child, a provider error with its partial answer, and an empty answer', async () => {
    await expect(s.run({ task: 'fail now' })).rejects.toThrow(/Subagent exited with code 3: boom/);
    await expect(s.run({ task: 'narrate then crash' })).rejects.toThrow(/Subagent exited with code 3: segfault\nLast progress: Let me look at src\/a\.ts/);
    const failed = await s.run({ task: 'error please' });
    expect(failed).toMatchObject({ isError: true, details: { status: 'failed' }, usage: { input: 100, output: 20 } });
    expect(failed.content[0].text).toMatch(/failed: provider down\n\nPartial answer:\npartial findings/);
    const silent = await s.run({ task: 'silent run' });
    expect(silent.content[0].text).toBe('(subagent returned no text)');
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

  it('strips terminal escapes and bidi controls from the child\'s report, error and stderr', async () => {
    await expect(s.run({ task: 'hostile fail' })).rejects.toThrow(/^Subagent exited with code 3: boom red$/m);
    const error = (await s.run({ task: 'hostile report' })).content[0].text as string;
    expect(error).toContain('failed: bad provider\n\nPartial answer:\nclick done');
    expect(error).not.toMatch(/[\u001b\u0007\u202e]/u);
  });

  it('hands a task starting with - or @ to the child as its message', async () => {
    for (const task of ['- report the dash', '@src/a.ts report']) {
      const facts = reportOf((await s.run({ task })).content[0].text as string);
      expect(facts.task.trim()).toBe(task);
      expect(facts.args.at(-1)).toBe('--');
    }
  });

  it('keeps a long report short in context and saves it whole as a doc', async () => {
    const result = await s.run({ task: 'long report' });
    const text = result.content[0].text as string;
    const file = path.join(s.cwd, '.octocode', 'tmp', 'agents', result.details.id, 'report.md');
    expect(text).toMatch(/^summary first\n/);
    expect(text).toContain('[Output truncated:');
    expect(text).toContain(`Full report: ${file} (read only the parts you need).`);
    expect(Buffer.byteLength(text)).toBeLessThan(9 * 1024);
    expect(fs.readFileSync(file, 'utf8')).toMatch(/the end$/);
    // A short report is not written to disk.
    const short = await s.run({ task: 'report' });
    expect(fs.existsSync(path.join(s.cwd, '.octocode', 'tmp', 'agents', short.details.id, 'report.md'))).toBe(false);
  });

  it('refuses unknown profiles and runs over the limit', async () => {
    await expect(s.run({ task: 'x', profile: 'nope' })).rejects.toThrow(/Unknown profile "nope". Available: researcher, webLive/);
    process.env[MAX_SUBAGENTS_ENV] = '1';
    try {
      const controller = new AbortController();
      const first = s.run({ task: 'slow one' }, controller.signal);
      await expect(s.run({ task: 'report' })).rejects.toThrow(/1 subagents are already running \(general-[0-9a-f]{6}\); the limit is 1/);
      controller.abort();
      await expect(first).rejects.toThrow('Subagent cancelled');
    } finally {
      delete process.env[MAX_SUBAGENTS_ENV];
    }
    // The slot is released after the cancelled run.
    await expect(s.run({ task: 'report' })).resolves.toBeDefined();
  });

  it('runs in the background, delivers the answer as a message, and /agents kill stops one', async () => {
    const started = await s.run({ task: 'report from the background', background: true });
    expect(started.content[0].text).toMatch(/^Started general-[0-9a-f]{6} in the background/);
    expect(started.content[0].text).toMatch(/its report arrives as a message\.$/);
    await vi.waitFor(() => expect(s.sent.length).toBeGreaterThan(0), { timeout: 10_000 });
    const answer = s.sent[0]!;
    expect(answer.options).toEqual({ triggerTurn: true, deliverAs: 'followUp' });
    expect(answer.message.content).toMatch(/^Background subagent general-[0-9a-f]{6} finished \(\d+s · 1 tool call · \$0\.01\):\nTask: report from the background\nreport /);
    const renderer = s.renderers.get('octocode-agent-result');
    expect(answer.message.details).toMatchObject({ status: 'done', summary: expect.stringMatching(/^Done \(1 tool call · \d+ tokens · /) });
    const shown = rendered(renderer(answer.message, { expanded: false }, theme));
    expect(shown).toMatch(/^ ● Agent\(general-[0-9a-f]{6}\) · background · \S+\n {3}⎿ {2}Done \(1 tool call/);
    expect(shown).toContain('report');

    await s.run({ task: 'error in background', background: true });
    await vi.waitFor(() => expect(s.sent.length).toBe(2), { timeout: 10_000 });
    expect(s.sent[1]!.message.content).toMatch(/failed: provider down/);
    expect(s.sent[1]!.message.details).toMatchObject({ status: 'failed', summary: 'Failed: provider down' });
    expect(rendered(renderer(s.sent[1]!.message, { expanded: false }, theme))).toMatch(/^ ✗ Agent\(general-[0-9a-f]{6}\)[^\n]*\n {3}⎿ {2}Failed: provider down/);

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
    await vi.waitFor(() => expect(s.sent.length).toBe(3), { timeout: 10_000 });
    expect(s.sent[2]!.message.content).toBe(`Background subagent ${id} cancelled by the user\nTask: slow background`);
    // A stop the user asked for is an interruption, not a failure; the prompt is one expand away.
    expect(rendered(renderer(s.sent[2]!.message, { expanded: false }, theme))).toMatch(/^ ◼ Agent.*\n {3}⎿ {2}Cancelled by the user\n {6}… prompt \(ctrl\+o to expand\)$/);
    expect(s.sent[2]!.options).toEqual({ triggerTurn: false, deliverAs: 'followUp' });
  });

  it('lets the parent agent stop only the background subagents it started, with coordinate stop', async () => {
    registerCollab(s.pi, s.team, { stopAgent: (id, by) => s.control.stop(id, by) });
    const coordinate = s.tools.get('coordinate');
    const stop = (id?: string) => coordinate.execute('c1', { action: 'stop', ...(id ? { id } : {}) }, undefined, undefined, s.ctx);
    await expect(stop('ghost')).rejects.toThrow('No background subagent "ghost". None is running.');
    const slow = await s.run({ task: 'slow background', background: true });
    const id = slow.details.id as string;
    expect((await stop(id)).content[0].text).toBe(`Stopping ${id}: its process tree is killed and its report arrives as a message.`);
    await vi.waitFor(() => expect(s.sent.at(-1)?.message.content).toBe(`Background subagent ${id} stopped by ${s.team.id}\nTask: slow background`), { timeout: 10_000 });
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
    expect(s.sent.at(-1)?.message.content).toBe(`Background subagent ${slow.details.id} stopped by session shutdown\nTask: slow at shutdown`);
    expect(s.sent.at(-1)!.options.triggerTurn).toBe(false);
  });

  it('tells parallel collaborating subagents about each other', async () => {
    const first = await s.run({ task: 'slow collaborator', background: true, collaborate: true });
    const second = await s.run({ task: 'report as collaborator', collaborate: true });
    const facts = reportOf(second.content[0].text);
    expect(facts.collaborate).toBe('1');
    expect(facts.task).toContain('## Teammates');
    expect(facts.task).toContain(`\`${first.details.id}\`: slow collaborator`);
    await s.commands.get('agents').handler(`kill ${first.details.id}`, s.ctx);
    await vi.waitFor(() => expect(s.sent.at(-1)?.message.content).toContain('cancelled by the user'), { timeout: 10_000 });
  });
});

describe('isolated subagents and /agents merge', () => {
  it('lands the child\'s edits on its ref and merges them with /agents merge', async () => {
    const dir = repo();
    const s = setup(dir);
    const merge = { handler: (args: string, ctx: unknown) => s.commands.get('agents').handler(`merge ${args}`, ctx) };
    await merge.handler('', s.ctx);
    expect(s.ctx.ui.notes.at(-1)).toEqual({ message: 'Usage: /agents merge <id>. No isolated results to merge.', type: 'warning' });

    const result = await s.run({ task: 'edit a file', isolate: true });
    const id = result.details.id as string;
    expect(result.content[0].text).toContain(`/agents merge ${id}`);
    expect(fs.existsSync(path.join(dir, 'child.txt'))).toBe(false);
    await merge.handler('', s.ctx);
    expect(s.ctx.ui.notes.at(-1)!.message).toBe(`Usage: /agents merge <id>. Unmerged: ${id}`);

    await merge.handler(id, s.ctx);
    expect(s.ctx.ui.notes.at(-1)!.type).toBe('info');
    expect(fs.readFileSync(path.join(dir, 'child.txt'), 'utf8')).toBe('from the child\n');
    await merge.handler('bad id!', s.ctx);
    expect(s.ctx.ui.notes.at(-1)!.type).toBe('error');

    // Background isolated runs report the outcome in their message: child.txt is already merged, so nothing changed.
    await s.run({ task: 'edit in background', isolate: true, background: true });
    await vi.waitFor(() => expect(s.sent.at(-1)?.message.content).toMatch(/finished[\s\S]*changed no files/), { timeout: 15_000 });
    await expect(s.run({ task: 'fail isolated', isolate: true })).rejects.toThrow(/Subagent exited with code 3/);
    await s.emit('session_start', {}, s.ctx);
  }, 60_000);
});

describe('trust for isolated subagents', () => {
  it('hands the repository to an isolated child only when this session trusts its project config', async () => {
    const trusted = repo();
    const s = setup(trusted);
    const facts = reportOf((await s.run({ task: 'report', isolate: true })).content[0].text);
    expect(facts).toMatchObject({ workspace: trusted, trustRoot: trusted });
    expect(reportOf((await s.run({ task: 'report' })).content[0].text).trustRoot).toBeUndefined();

    const undecided = repo();
    fs.mkdirSync(path.join(undecided, '.claude'));
    fs.writeFileSync(path.join(undecided, '.claude', 'settings.json'), '{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"node"}]}]}}');
    git(undecided, 'add', '-A');
    git(undecided, 'commit', '-q', '-m', 'hooks');
    const u = setup(undecided);
    const child = reportOf((await u.run({ task: 'report', isolate: true })).content[0].text);
    expect(child).toMatchObject({ workspace: undecided });
    expect(child.trustRoot).toBeUndefined();
  }, 30_000);

  it('never passes an inherited trust root on, and sets one only when asked', () => {
    const inherited = subagentProcessEnv(undefined, { OCTOCODE_TRUST_ROOT: '/elsewhere' }, { id: 'general-1234', workspace: '/repo' });
    expect(inherited).toMatchObject({ OCTOCODE_TEAM_WORKSPACE: '/repo' });
    expect(inherited).not.toHaveProperty('OCTOCODE_TRUST_ROOT');
    expect(subagentProcessEnv(undefined, {}, { id: 'general-1234', workspace: '/repo', trustRoot: '/repo' })).toMatchObject({ OCTOCODE_TRUST_ROOT: '/repo' });
  });
});

describe('orphaned worktrees at session start', () => {
  it('saves what it can under the actual ref name and warns about a worktree it had to keep', async () => {
    const dir = repo();
    const git = (cwd: string, ...args: string[]) => execFileSync('git', args, { cwd, encoding: 'utf8' }).trim();
    git(dir, 'update-ref', 'refs/octocode/pi/orphan-7', 'HEAD');
    const dead = spawnSync(process.execPath, ['-e', '0']).pid;
    const [saved, stuck] = [await createWorktree(dir, 'orphan-7'), await createWorktree(dir, 'stuck-7')];
    for (const worktree of [saved, stuck]) {
      fs.writeFileSync(path.join(worktree.path, `${worktree.agentId}.txt`), 'work\n');
      const meta = `${worktree.path}.json`;
      fs.writeFileSync(meta, JSON.stringify({ ...JSON.parse(fs.readFileSync(meta, 'utf8')), pids: [dead] }));
    }
    fs.writeFileSync(path.join(git(stuck.path, 'rev-parse', '--absolute-git-dir'), 'index.lock'), '');
    const s = setup(dir);
    await s.emit('session_start', {}, s.ctx);
    await vi.waitFor(() => expect(s.ctx.ui.notes.some((note) => note.type === 'warning')).toBe(true), { timeout: 10_000 });
    const note = s.ctx.ui.notes.find((entry) => entry.type === 'warning')!.message;
    expect(note).toContain('orphan-7 (changes on refs/octocode/pi/orphan-7-2)');
    expect(note).toContain(`Kept orphaned subagent worktree stuck-7`);
    expect(note).toContain(stuck.path);
    expect(fs.existsSync(path.join(stuck.path, 'stuck-7.txt'))).toBe(true);
  }, 30_000);
});

describe('runSubagent', () => {
  it('terminates the child when recording its launch fails', async () => {
    let pid = 0;
    await expect(runSubagent(['slow launch'], tmp(), undefined, { id: 'a-launch-failure' }, undefined, () => undefined, (childPid) => {
      pid = childPid;
      throw new Error('cannot record launch');
    })).rejects.toThrow('cannot record launch');
    try {
      expect(() => process.kill(pid, 0)).toThrow();
    } finally {
      try { process.kill(pid, 'SIGKILL'); } catch { /* Already exited. */ }
    }
  });

  it('rejects oversized unterminated events instead of buffering them without limit', async () => {
    await expect(runSubagent(['oversized'], tmp(), undefined, { id: 'a-oversized' }, undefined, () => undefined)).rejects.toThrow(/JSON event exceeds/);
  });

  it('bounds the returned report while preserving the complete text in scratch', async () => {
    const scratch = tmp();
    const outcome = await runSubagent(['long report'], tmp(), undefined, { id: 'a-bounded', scratch }, undefined, () => undefined);
    expect(Buffer.byteLength(outcome.text)).toBeLessThan(9 * 1024);
    expect(outcome.text).toContain('Full report:');
    expect(fs.readFileSync(path.join(scratch, 'report.md'), 'utf8')).toMatch(/the end$/);
  });

  it('rejects at once when already aborted', async () => {
    const controller = new AbortController();
    controller.abort();
    await expect(runSubagent(['x'], tmp(), undefined, { id: 'a-0000' }, controller.signal, () => undefined)).rejects.toThrow('Subagent cancelled');
  });

  it('reports the child pid and parses a final line without a newline', async () => {
    const pids: number[] = [];
    const outcome = await runSubagent(['report'], tmp(), undefined, { id: 'a-0001', task: 't', workspace: '/w' }, undefined, () => undefined, (pid) => pids.push(pid));
    expect(pids).toHaveLength(1);
    expect(reportOf(outcome.text).id).toBe('a-0001');
    expect(outcome.usage.input).toBe(100);
  });

  it('keeps the report when a late notice wakes the child after it answered', async () => {
    const outcome = await runSubagent(['late notice'], tmp(), undefined, { id: 'a-0003' }, undefined, () => undefined);
    expect(outcome.text).toBe('FULL REPORT\n\n---\n\nThat job was my coverage rerun; findings unchanged.');
    expect(outcome.error).toBeUndefined();
  });

  it('reports a run done when only a follow-up turn after its answer failed', async () => {
    const outcome = await runSubagent(['late failure'], tmp(), undefined, { id: 'a-0004' }, undefined, () => undefined);
    expect(outcome.text).toBe('FULL REPORT\n\n---\n\n(A later follow-up turn failed: overloaded)');
    expect(outcome.error).toBeUndefined();
  });

  it('never keeps a failed draft as a report section', async () => {
    const outcome = await runSubagent(['steered draft'], tmp(), undefined, { id: 'a-0005' }, undefined, () => undefined);
    expect(outcome.text).toBe('FINAL REPORT');
    expect(outcome.error).toBeUndefined();
  });

  it('keeps running when the progress callback throws', async () => {
    const outcome = await runSubagent(['report'], tmp(), undefined, { id: 'a-0002' }, undefined, () => {
      throw new Error('render gone');
    });
    expect(reportOf(outcome.text).id).toBe('a-0002');
  });
});

describe('subagent watchdog and exits', () => {
  it('kills a child that emits nothing for the idle limit and says what it last did', async () => {
    await expect(runSubagent(['hang'], tmp(), undefined, { id: 'a-hang' }, undefined, () => undefined, undefined, { idleMs: 300, maxMs: Number.POSITIVE_INFINITY }))
      .rejects.toThrow(/^Subagent stopped: no output for .*; last activity: started, .* ago\. Its process tree was killed\.$/);
  });

  it('gives a running tool extra quiet time but enforces the wall-clock limit', async () => {
    await expect(runSubagent(['slow tool'], tmp(), undefined, { id: 'a-slow' }, undefined, () => undefined, undefined, { idleMs: 300, maxMs: 1_200 }))
      .rejects.toThrow(/ran past its .* limit \(OCTOCODE_SUBAGENT_MAX_MINUTES\); last activity: → bash sleep 60/);
  });

  it('reads its limits from the environment, 0 meaning none', () => {
    expect(watchdogLimits({})).toEqual({ idleMs: 10 * 60_000, maxMs: Number.POSITIVE_INFINITY });
    expect(watchdogLimits({ OCTOCODE_SUBAGENT_IDLE_MINUTES: '0', OCTOCODE_SUBAGENT_MAX_MINUTES: '45' })).toEqual({ idleMs: Number.POSITIVE_INFINITY, maxMs: 45 * 60_000 });
  });

  it('names the signal that killed a child instead of "code null"', () => {
    expect(exitReason(null, 'SIGTERM')).toBe('was killed by SIGTERM');
    expect(exitReason(3, null)).toBe('exited with code 3');
  });

  it('lets /agents kill stop a foreground run without aborting the turn', async () => {
    const s = setup();
    const running = s.run({ task: 'slow foreground' });
    let id = '';
    await vi.waitFor(async () => {
      await s.commands.get('agents').handler('kill', s.ctx);
      id = /Running: (\S+)/.exec(s.ctx.ui.notes.at(-1)!.message)?.[1] ?? '';
      expect(id).not.toBe('');
    }, { timeout: 5_000 });
    await s.commands.get('agents').handler(`kill ${id}`, s.ctx);
    await expect(running).rejects.toThrow(`Subagent ${id} was stopped by the user.`);
  }, 15_000);
});

describe('background report delivery', () => {
  it('batches reports so only the last of a batch may start a turn', () => {
    const f = fakePi();
    const queue = new ReportQueue(f.pi, 60_000);
    const report = (id: string, wake: boolean) => ({ content: id, details: { id, status: 'done' as const, summary: 'Done' }, wake });
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
    read.observe('mcp__octocode__localGetFileContent', { queries: [{ path: file }] }, cwd);
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
    const { id } = result.details as { id: string };
    const scratch = path.join(s.cwd, '.octocode', 'tmp', 'agents', id);
    expect(fs.existsSync(path.join(scratch, 'report.md'))).toBe(true);
    const late = s.team.send(id, 'more');
    expect('error' in late && late.error).toContain(`(files it handed back, and its full report when cut, are in`);
  });
});

