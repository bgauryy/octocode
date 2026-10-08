import fs from 'node:fs';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { AgentsView } from '../src/team/panel.js';
import { MESSAGE_TYPE, Team } from '../src/team/session.js';
import { registerCollab } from '../src/team/tools.js';
import { registerActivity } from '../src/ui/activity.js';
import { registerAgentTool, type AgentControl } from '../src/subagents/tool.js';
import { Subcommands } from '../src/shared/commands.js';
import { fakeCtx, fakePi, rendered, theme } from './fake-pi.js';
import { TeamStore } from '../src/team/store.js';
import { tmp } from './helpers.js';

/** `/agents` as index.ts wires it: an `/octocode` subcommand with a top-level shortcut. */
function agentsCommand(pi: Parameters<Subcommands['alias']>[0], control: AgentControl): void {
  const commands = new Subcommands();
  commands.add('agents', control.command);
  commands.alias(pi, 'agents');
}

function member(dbFile: string, cwd: string) {
  const fake = fakePi();
  const team = new Team(fake.pi, { OCTOCODE_AGENT_DB: dbFile });
  const { reservationGate } = registerCollab(fake.pi, team);
  // As index.ts wires it: the activity line reaches the team panel.
  registerActivity(fake.pi, { workingLine: false, onChange: (line) => team.setActivity(line) });
  agentsCommand(fake.pi, registerAgentTool(fake.pi, () => new Map(), team));
  const ctx = fakeCtx({ cwd });
  // The reservation gate as index.ts runs it in the tool-call pipeline.
  const gate = async (event: unknown, context: unknown) => [await reservationGate(event as never, context as never)];
  return { ...fake, team, ctx, gate };
}

const run = (tool: { execute: Function }, params: unknown, ctx: unknown) => tool.execute('call-1', params, undefined, undefined, ctx);
const text = (result: { content: Array<{ text: string }> }) => result.content.map((part) => part.text).join('\n');

describe('team tools on a fake Pi', () => {
  const started: Array<ReturnType<typeof member>> = [];
  afterEach(async () => {
    for (const agent of started.splice(0)) await agent.emit('session_shutdown', {}, agent.ctx);
  });

  async function pair() {
    const cwd = tmp();
    const db = path.join(tmp(), 'team.sqlite');
    const a = member(db, cwd);
    const b = member(db, cwd);
    started.push(a, b);
    await a.emit('session_start', {}, a.ctx);
    await b.emit('session_start', {}, b.ctx);
    return { a, b, cwd };
  }

  it('joins, lists, locks, blocks a peer edit, unlocks and leaves', async () => {
    const { a, b } = await pair();
    const coordinate = a.tools.get('coordinate');
    expect(text(await run(coordinate, { action: 'list' }, a.ctx))).toMatch(/no other agents|No agents/i);
    const joined = text(await run(coordinate, { action: 'join', note: 'refactor auth\nsecond line' }, a.ctx));
    expect(joined).toContain(a.team.id!);
    await expect(run(coordinate, { action: 'lock' }, a.ctx)).rejects.toThrow('lock needs paths.');
    expect(text(await run(coordinate, { action: 'lock', paths: ['src/a.ts', 'lib/'], reason: 'auth' }, a.ctx))).toBe('Reserved: src/a.ts, lib/. Unlock when your edits are done.');
    expect(text(await run(coordinate, { action: 'lock', paths: ['src/b.ts'] }, a.ctx))).toBe('Reserved: src/b.ts. You now hold 3 reservations. Unlock when your edits are done.');
    await run(coordinate, { action: 'unlock', paths: ['src/b.ts'] }, a.ctx);

    // The peer sees the reservation: its own lock fails, and its edit/write tool calls are blocked.
    await run(b.tools.get('coordinate'), { action: 'join' }, b.ctx);
    await expect(run(b.tools.get('coordinate'), { action: 'lock', paths: ['lib/x.ts'] }, b.ctx)).rejects.toThrow(/Not reserved; nothing was locked/);
    const [blocked] = await b.gate({ toolName: 'write', input: { path: 'src/a.ts' } }, b.ctx);
    expect(blocked).toMatchObject({ block: true, reason: expect.stringMatching(new RegExp(`reserved by ${a.team.id} \\(auth; locked \\d+s ago\\)`)) });
    expect(await b.gate({ toolName: 'edit', input: { path: '@lib/deep/y.ts' } }, b.ctx)).toEqual([expect.objectContaining({ block: true })]);
    // `file` is guarded inside the file tool itself; unrelated tools pass.
    expect(await b.gate({ toolName: 'file', input: { queries: [{ path: 'src/a.ts' }] } }, b.ctx)).toEqual([undefined]);
    expect(await b.gate({ toolName: 'write', input: { path: 'free.ts' } }, b.ctx)).toEqual([undefined]);

    const listed = text(await run(b.tools.get('coordinate'), { action: 'list' }, b.ctx));
    expect(listed).toContain(a.team.id!);
    expect(listed).toContain('src/a.ts');

    expect(text(await run(coordinate, { action: 'unlock', paths: ['src/a.ts'] }, a.ctx))).toBe('Released 1 reservation(s).');
    expect(await b.gate({ toolName: 'write', input: { path: 'src/a.ts' } }, b.ctx)).toEqual([undefined]);
    expect(text(await run(coordinate, { action: 'unlock' }, a.ctx))).toBe('Released 1 reservation(s).');
    expect(text(await run(coordinate, { action: 'leave' }, a.ctx))).toBe('Left the team.');
    expect(text(await run(b.tools.get('coordinate'), { action: 'list' }, b.ctx))).not.toContain(a.team.id!);
  });

  it('resolves lock paths against the caller\'s cwd, as the edit tools do, when Pi runs in a subfolder', async () => {
    const root = tmp();
    fs.mkdirSync(path.join(root, '.git'));
    fs.mkdirSync(path.join(root, 'pkg'));
    const db = path.join(tmp(), 'team.sqlite');
    const inPkg = member(db, path.join(root, 'pkg'));
    const atRoot = member(db, root);
    const peerInPkg = member(db, path.join(root, 'pkg'));
    started.push(inPkg, atRoot, peerInPkg);
    for (const agent of [inPkg, atRoot, peerInPkg]) await agent.emit('session_start', {}, agent.ctx);
    const coordinate = inPkg.tools.get('coordinate');
    expect(text(await run(coordinate, { action: 'lock', paths: ['src/a.ts', 'lib/'], reason: 'work' }, inPkg.ctx))).toContain('pkg/src/a.ts');
    for (const agent of [atRoot, peerInPkg]) await run(agent.tools.get('coordinate'), { action: 'join' }, agent.ctx);
    expect(await atRoot.gate({ toolName: 'write', input: { path: 'pkg/src/a.ts' } }, atRoot.ctx)).toEqual([expect.objectContaining({ block: true })]);
    expect(await peerInPkg.gate({ toolName: 'edit', input: { path: 'src/a.ts' } }, peerInPkg.ctx)).toEqual([expect.objectContaining({ block: true })]);
    expect(await peerInPkg.gate({ toolName: 'edit', input: { path: path.join(root, 'pkg', 'lib', 'deep.ts') } }, peerInPkg.ctx)).toEqual([expect.objectContaining({ block: true })]);
    // The same relative path at the repository root is another file.
    expect(await atRoot.gate({ toolName: 'write', input: { path: 'src/a.ts' } }, atRoot.ctx)).toEqual([undefined]);
    // A peer's conflicting lock names the path as the peer typed it.
    await expect(run(peerInPkg.tools.get('coordinate'), { action: 'lock', paths: ['src/a.ts'] }, peerInPkg.ctx)).rejects.toThrow(/src\/a\.ts/);
    expect(text(await run(coordinate, { action: 'unlock', paths: ['src/a.ts', 'lib/'] }, inPkg.ctx))).toBe('Released 2 reservation(s).');
    expect(await atRoot.gate({ toolName: 'write', input: { path: 'pkg/src/a.ts' } }, atRoot.ctx)).toEqual([undefined]);
  });

  it('delivers messages between agents and rejects unknown recipients', async () => {
    const { a, b } = await pair();
    await run(b.tools.get('coordinate'), { action: 'join' }, b.ctx);
    const send = a.tools.get('sendMessage');
    const sent = await run(send, { to: b.team.id, message: 'please review src/a.ts', replyRequired: false }, a.ctx);
    expect(text(sent)).toMatch(new RegExp(`Queued message #\\d+ for ${b.team.id}\\.`));
    expect(sent.details).toMatchObject({ to: [b.team.id] });
    await vi.waitFor(() => expect(b.sent.some((entry) => entry.message.customType === MESSAGE_TYPE)).toBe(true), { timeout: 5_000 });
    const delivered = b.sent.find((entry) => entry.message.customType === MESSAGE_TYPE)!;
    expect(delivered.message.details).toMatchObject({ from: a.team.id, text: 'please review src/a.ts' });

    await expect(run(send, { to: 'nobody-0000', message: 'hi' }, a.ctx)).rejects.toThrow();
    const reply = await run(b.tools.get('sendMessage'), { to: 'parent', message: 'x' }, b.ctx).catch((error: Error) => error);
    expect(reply).toBeInstanceOf(Error);
  });

  it('tracks activity and renders messages and calls', async () => {
    const { a, b } = await pair();
    await run(a.tools.get('coordinate'), { action: 'join' }, a.ctx);
    await a.emit('agent_start', {}, a.ctx);
    await a.emit('tool_execution_start', { toolCallId: 't1', toolName: 'bash', args: { command: 'yarn test' } }, a.ctx);
    await a.emit('message_end', { message: { role: 'assistant', usage: { input: 1200, output: 300, cacheRead: 0, cacheWrite: 0, totalTokens: 1500, cost: { total: 0 } } } }, a.ctx);
    await a.emit('message_end', { message: { role: 'user' } }, a.ctx);
    // Activity reaches the shared database on the member's next tick.
    await vi.waitFor(() => {
      const busy = b.team.members().find((entry) => entry.id === a.team.id)!;
      expect(busy).toMatchObject({ status: 'working', input: 1200, output: 300 });
      expect(busy.activity).toContain('bash');
    }, { timeout: 3_000 });
    await a.emit('agent_end', {}, a.ctx);
    // Queued messages, retries and compaction can continue after agent_end.
    await new Promise((resolve) => setTimeout(resolve, 1_100));
    expect(b.team.members().find((entry) => entry.id === a.team.id)!.status).toBe('working');
    await a.emit('agent_settled', {}, a.ctx);
    await vi.waitFor(() => expect(b.team.members().find((entry) => entry.id === a.team.id)!.status).toBe('idle'), { timeout: 3_000 });

    const renderer = a.renderers.get(MESSAGE_TYPE);
    const message = { details: { from: 'reviewer-1a2b', text: 'first\nsecond', replyRequired: true, at: Date.parse('2026-01-01T10:00:00Z') } };
    const collapsed = rendered(renderer(message, { expanded: false }, theme));
    expect(collapsed).toContain('from reviewer-1a2b');
    expect(collapsed).toContain('reply requested');
    expect(collapsed).toMatch(/first\n\s*second/);
    expect(rendered(renderer(message, { expanded: true }, theme))).toMatch(/first\n\s*second/);
    expect(rendered(renderer({ details: { from: 'x', text: 'bad\u001b[31mred' } }, { expanded: false }, theme))).toContain('badred');
    expect(rendered(renderer({}, { expanded: false }, theme))).toContain('from agent');

    const coordinate = a.tools.get('coordinate');
    expect(rendered(coordinate.renderCall({ action: 'lock', paths: ['a.ts', 'b/'], note: 'n' }, theme, {}))).toBe('○ Coordinate(lock 2 paths "n")');
    expect(rendered(coordinate.renderCall({ action: 'unlock', paths: ['a.ts'] }, theme, {}))).toBe('○ Coordinate(unlock a.ts)');
    expect(rendered(coordinate.renderCall({}, theme, {}))).toBe('○ Coordinate');
    const listed = await run(coordinate, { action: 'list' }, a.ctx);
    expect(listed.details).toMatchObject({ summary: expect.stringMatching(/^\d+ agents? · \d+ working$/) });
    const listRow = rendered(coordinate.renderResult(listed, { expanded: false }, theme, { args: { action: 'list' }, expanded: false }));
    expect(listRow).toMatch(/^ {2}⎿ {2}\d+ agents? · \d+ working \(ctrl\+o to expand\)$/);
    expect(rendered(coordinate.renderResult(listed, { expanded: true }, theme, { args: { action: 'list' }, expanded: true }))).toContain(a.team.id!);
    expect(rendered(coordinate.renderResult({ content: [{ type: 'text', text: 'Not reserved; nothing was locked:\n- a.ts is covered' }] }, {}, theme, { isError: true }))).toBe('  ⎿  Error: Not reserved; nothing was locked:\n     - a.ts is covered');
    const send = a.tools.get('sendMessage');
    const long = `hello\n${'x'.repeat(500)}`;
    expect(rendered(send.renderCall({ to: 'all', message: long }, theme, {}), 1_000)).toBe('○ Send(→ all)');
    expect(rendered(send.renderCall({ to: 'x', replyTo: 41, message: 'm' }, theme, {}))).toBe('○ Send(→ x · reply to #41)');
    expect(rendered(send.renderCall({}, theme, {}))).toBe('○ Send');
    const delivered = { content: [{ type: 'text', text: 'Queued message #42 for b.' }], details: { id: 42, to: ['b'] } };
    expect(rendered(send.renderResult(delivered, {}, theme, { args: { message: long }, expanded: false }))).toBe('  ⎿  Queued #42 (ctrl+o to expand)');
    expect(rendered(send.renderResult(delivered, {}, theme, { args: { message: long }, expanded: true }), 1_000)).toBe(`  ⎿  Queued #42\n     hello\n     ${'x'.repeat(500)}`);
  });

  it('/agents and /agents tell report through the UI', async () => {
    const { a, b } = await pair();
    await run(b.tools.get('coordinate'), { action: 'join' }, b.ctx);
    await a.commands.get('agents').handler('', a.ctx);
    expect(a.ctx.ui.notes.at(-1)).toMatchObject({ type: 'info', message: expect.stringContaining(b.team.id!) });

    const agents = a.commands.get('agents');
    const tell = { handler: (args: string, ctx: unknown) => agents.handler(`tell ${args}`, ctx) };
    await tell.handler('   ', a.ctx);
    expect(a.ctx.ui.notes.at(-1)).toEqual({ message: 'Usage: /agents tell <agent|all> <message>', type: 'warning' });
    await tell.handler(`${b.team.id} line one\n  line two`, a.ctx);
    expect(a.ctx.ui.notes.at(-1)).toEqual({ message: `Queued for ${b.team.id}`, type: 'info' });
    await vi.waitFor(() => expect(b.sent.length).toBeGreaterThan(0), { timeout: 5_000 });
    // The user's text reaches the agent as typed, line breaks included.
    expect(b.sent.at(-1)!.message.details).toMatchObject({ from: 'user', text: 'line one\n  line two' });
    await tell.handler('ghost-0000 hi', a.ctx);
    expect(a.ctx.ui.notes.at(-1)!.type).toBe('warning');

    expect(await agents.getArgumentCompletions(`tell ${b.team.id!.slice(0, 3)}`)).toEqual([expect.objectContaining({ value: `tell ${b.team.id} ` })]);
    expect(await agents.getArgumentCompletions('tell zzz')).toBeNull();
    expect(await agents.getArgumentCompletions('k')).toEqual([expect.objectContaining({ value: 'kill ' })]);
    expect(await agents.getArgumentCompletions('kill ')).toBeNull();
    await agents.handler('bogus', a.ctx);
    expect(a.ctx.ui.notes.at(-1)).toMatchObject({ type: 'warning', message: expect.stringContaining('Usage: /agents') });
  });

  it('/agents warns when the team database is unusable', async () => {
    const fake = fakePi();
    const team = new Team(fake.pi, { OCTOCODE_AGENT_DB: path.join(tmp(), 'team.sqlite') });
    agentsCommand(fake.pi, registerAgentTool(fake.pi, () => new Map(), team));
    vi.spyOn(team, 'snapshot').mockReturnValue({ members: [], traffic: [], error: 'schema v99 is newer' });
    const ctx = fakeCtx({ cwd: tmp() });
    await fake.commands.get('agents').handler('', ctx);
    expect(ctx.ui.notes.at(-1)).toMatchObject({ type: 'warning', message: expect.stringContaining('schema v99 is newer') });
  });
});

/** A Team stand-in that honors the watch contract: one listener call now and one per second. */
function fakeTeam(read: () => { members: never[]; traffic: never[]; error?: string }, id?: string) {
  return {
    id,
    get view() {
      return read();
    },
    watch(listener: () => void) {
      const timer = setInterval(listener, 1_000);
      listener();
      return () => clearInterval(timer);
    },
  } as never;
}

describe('AgentsView', () => {
  const now = Date.now();
  const peer = (id: string, status: 'working' | 'idle' = 'working', parentId: string | null = 'me-0001') => ({ id, status, ...(parentId ? { parentId } : {}), joinedAt: now - 5_000, updatedAt: now, toolCalls: 2, input: 1000, output: 200, task: 'review the parser', activity: 'localSearch foo' }) as never;

  it('shows a widget for this session\'s subagents only, repaints, and clears when they leave', () => {
    vi.useFakeTimers();
    try {
      const otherSession = [peer('main-9999', 'working', null), peer('researcher-8888', 'working', 'main-9999')];
      let members = [peer('me-0001', 'working', null), peer('reviewer-1a2b'), peer('tester-3c4d', 'idle'), ...otherSession];
      const traffic = [
        { id: 2, from: 'me-0001', to: ['reviewer-1a2b'], text: 'look', at: now, state: 'delivered' },
        { id: 3, from: 'reviewer-1a2b', to: ['tester-3c4d'], text: 'between subagents', at: now, state: 'delivered' },
        { id: 4, from: 'main-9999', to: ['researcher-8888'], text: 'theirs', at: now, state: 'delivered' },
      ] as never[];
      const view = new AgentsView(fakeTeam(() => ({ members, traffic }), 'me-0001'));
      const ctx = fakeCtx({ cwd: tmp() });
      view.start(ctx);
      const factory = ctx.ui.widgets.get('octocode-agents') as (tui: unknown, theme: unknown) => { render(width: number): string[]; invalidate(): void };
      expect(factory).toBeTypeOf('function');
      const requestRender = vi.fn();
      const widget = factory({ requestRender }, theme);
      widget.invalidate();
      const lines = widget.render(120).join('\n');
      expect(lines).toContain('1 working · 1 idle');
      expect(lines).toContain('reviewer-1a2b');
      // Only traffic between subagents: this session's own messages are already in its transcript.
      expect(lines).toContain('#3');
      expect(lines).not.toContain('#2');
      expect(lines).not.toContain('main-9999');
      expect(lines).not.toContain('researcher-8888');
      expect(lines).not.toContain('#4');
      // The widget counts the agents; the footer does not repeat them.
      expect(ctx.ui.statuses.get('octocode-team')).toBeUndefined();
      vi.advanceTimersByTime(1_000);
      expect(requestRender).toHaveBeenCalled();

      members = [peer('me-0001', 'working', null), ...otherSession];
      vi.advanceTimersByTime(1_000);
      expect(ctx.ui.widgets.has('octocode-agents')).toBe(false);
      expect(ctx.ui.statuses.get('octocode-team')).toBeUndefined();

      members = [peer('me-0001', 'working', null), peer('reviewer-1a2b', 'idle')];
      vi.advanceTimersByTime(1_000);
      expect(ctx.ui.widgets.has('octocode-agents')).toBe(true);
      view.stop(ctx);
      expect(ctx.ui.widgets.has('octocode-agents')).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it('never mounts a widget for subagents whose heartbeat stopped, and clears it once they go silent', () => {
    vi.useFakeTimers({ now });
    try {
      const silent = { ...(peer('reviewer-1a2b') as object), updatedAt: now - 16_000 } as never;
      let members = [peer('me-0001', 'working', null), silent];
      const view = new AgentsView(fakeTeam(() => ({ members, traffic: [] }), 'me-0001'));
      const ctx = fakeCtx({ cwd: tmp() });
      view.start(ctx);
      expect(ctx.ui.widgets.has('octocode-agents')).toBe(false);
      members = [peer('me-0001', 'working', null), { ...(peer('reviewer-1a2b') as object), updatedAt: Date.now() } as never];
      vi.advanceTimersByTime(1_000);
      expect(ctx.ui.widgets.has('octocode-agents')).toBe(true);
      // The heartbeat stops: 16 s later the agent is unresponsive and the widget unmounts.
      vi.advanceTimersByTime(16_000);
      expect(ctx.ui.widgets.has('octocode-agents')).toBe(false);
      view.stop(ctx);
    } finally {
      vi.useRealTimers();
    }
  });

  it('reports an unusable team database in the status line and survives read failures', () => {
    const ctx = fakeCtx({ cwd: tmp() });
    const view = new AgentsView(fakeTeam(() => ({ members: [], traffic: [], error: 'Team database schema v9 is newer\nupgrade' })));
    view.start(ctx);
    expect(ctx.ui.statuses.get('octocode-team')).toBe('team off: Team database schema v9 is newer');
    view.stop(ctx);
    expect(ctx.ui.statuses.get('octocode-team')).toBeUndefined();

    const failing = new AgentsView(
      fakeTeam(() => {
        throw new Error('boom');
      }),
    );
    expect(() => failing.start(ctx)).not.toThrow();
    failing.stop();

    const headless = fakeCtx({ cwd: tmp(), hasUI: false });
    const read = vi.fn(() => ({ members: [], traffic: [] }));
    new AgentsView(fakeTeam(read)).start(headless);
    expect(read).not.toHaveBeenCalled();
  });
});

describe('Team ticker', () => {
  it('runs one unref\'d ticker only while joined or watched, and watchers read the cached view', async () => {
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval', 'Date'] });
    const db = path.join(tmp(), 'team.sqlite');
    const cwd = tmp();
    const a = member(db, cwd);
    const b = member(db, cwd);
    try {
      a.team.start(a.ctx);
      b.team.start(b.ctx);
      expect(vi.getTimerCount()).toBe(0);
      const seen = vi.fn();
      const unwatch = a.team.watch(seen);
      expect(vi.getTimerCount()).toBe(1);
      expect(seen).toHaveBeenCalledTimes(1);
      // Nobody created the database yet: watching reads nothing.
      expect(a.team.view.members).toEqual([]);
      const { existsSync } = await import('node:fs');
      expect(existsSync(db)).toBe(false);

      b.team.join('peer');
      expect(vi.getTimerCount()).toBe(2);
      vi.advanceTimersByTime(1_000);
      expect(seen).toHaveBeenCalledTimes(2);
      expect(a.team.view.members.map((m) => m.id)).toContain(b.team.id);

      unwatch();
      expect(vi.getTimerCount()).toBe(1);
      b.team.leave();
      expect(vi.getTimerCount()).toBe(0);
    } finally {
      a.team.stop();
      b.team.stop();
      vi.useRealTimers();
    }
  });
});

describe('team database file', () => {
  it('is the agent database, private to the user', () => {
    const file = path.join(tmp(), 'nested', 'team', 'team.sqlite');
    const cwd = tmp();
    TeamStore.open(cwd, file, {}).close();
    if (process.platform !== 'win32') {
      expect(fs.statSync(path.dirname(file)).mode & 0o777).toBe(0o700);
      expect(fs.statSync(file).mode & 0o777).toBe(0o600);
    }
  });
});
