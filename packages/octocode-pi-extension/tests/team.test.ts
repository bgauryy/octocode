import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { formatDuration, memberStats } from '../src/shared/format.js';
import { trafficRow, widgetLines } from '../src/team/panel.js';
import { toolHint } from '../src/shared/util.js';
import { deadLetterText, describeChanges, describeConflicts, describeMembers, mutatedPaths, resolveTarget } from '../src/team/routing.js';
import { octocodePrompt } from '../src/prompt.js';
import { subagentProcessEnv } from '../src/subagents/process.js';
import { registerAgentTool } from '../src/subagents/tool.js';
import type { Member } from '../src/team/model.js';
import { theme, fakePermission } from './fake-pi.js';
import { tmp } from './helpers.js';
import { member, repo } from './team-harness.js';

describe('team database', () => {
  it('resolves ids, unique roles, parent and all', () => {
    const all = [member('main-aaaaaa'), member('researcher-bbbbbb', { parentId: 'main-aaaaaa' }), member('reviewer-cccccc', { parentId: 'main-aaaaaa' }), member('reviewer-dddddd', { parentId: 'researcher-bbbbbb' })];
    const ids = (r: Member[] | string) => (typeof r === 'string' ? r : r.map((m) => m.id));
    expect(ids(resolveTarget(all, 'main-aaaaaa', 'researcher-bbbbbb'))).toEqual(['researcher-bbbbbb']);
    expect(ids(resolveTarget(all, 'main-aaaaaa', 'researcher'))).toEqual(['researcher-bbbbbb']);
    expect(ids(resolveTarget(all, 'researcher-bbbbbb', 'parent', 'main-aaaaaa'))).toEqual(['main-aaaaaa']);
    expect(ids(resolveTarget(all, 'main-aaaaaa', 'all'))).toEqual(['researcher-bbbbbb', 'reviewer-cccccc', 'reviewer-dddddd']);
    expect(ids(resolveTarget(all, 'main-aaaaaa', 'reviewer'))).toMatch(/several agents/);
    expect(ids(resolveTarget(all, 'main-aaaaaa', 'ghost'))).toMatch(/No live agent "ghost"/);
    expect(ids(resolveTarget(all, 'main-aaaaaa', 'main-aaaaaa'))).toMatch(/No live agent/);
  });

  it('passes identity to a child', () => {
    const env = subagentProcessEnv(undefined, {}, { id: 'general-1234', parentId: 'main-aaaaaa', goal: 'Find X' });
    expect(env).toMatchObject({ OCTOCODE_AGENT_ID: 'general-1234', OCTOCODE_PARENT_ID: 'main-aaaaaa', OCTOCODE_AGENT_GOAL: 'Find X' });
  });
});

describe('agents view', () => {
  it('formats compact numbers and durations', () => {
    expect(formatDuration(45_000)).toBe('45s');
    expect(formatDuration(134_000)).toBe('2m14s');
    expect(memberStats(member('a-0000', { toolCalls: 7, input: 12_000, cacheRead: 3_000, output: 3_000, cost: 0.024 }))).toBe('7 tool calls · ↑12k (25% cached) ↓3k · ≈$0.02');
    expect(memberStats(member('a-0000'))).toBe('0 tool calls');
  });

  it('shows only this session\'s subagents as a tree, never yourself, and nothing when alone', () => {
    const now = 100_000;
    const foreign = [member('main-cccccc', { status: 'working' }), member('c-0003', { parentId: 'main-cccccc', status: 'working' })];
    const list = [member('main-aaaaaa'), member('a-0001', { parentId: 'main-aaaaaa', joinedAt: 90_000 }), member('b-0002', { parentId: 'a-0001', status: 'working', activity: '→ localSearch foo', joinedAt: 40_000 }), ...foreign];
    const lines = widgetLines(list, 'main-aaaaaa', now, theme, 120);
    expect(lines[0]).toContain('1 working · 1 idle');
    expect(lines.join('\n')).not.toMatch(/main-cccccc|c-0003/);
    expect(lines[1]).toContain('a-0001');
    expect(lines[2]).toContain('└ b-0002');
    expect(lines[2]).toMatch(/working\s+·\s+1m\s/);
    expect(lines[2]).toContain('→ localSearch foo');
    expect(widgetLines([member('a-0001', { parentId: 'main-aaaaaa', status: 'working', goal: 'Find where tools register', activity: 'localSearch x' })], 'main-aaaaaa', now, theme, 200)[1]).toMatch(/localSearch x$/);
    expect(widgetLines(list, undefined, now, theme, 200)).toEqual([]);
    expect(lines.join('\n')).not.toContain('main-aaaaaa');
    expect(widgetLines([member('main-aaaaaa')], 'main-aaaaaa', now, theme, 80)).toEqual([]);
  });

  it('shows controls in text other agents wrote (conflicts, changes, dead letters) as visible escapes', () => {
    const held = { path: 'src', kind: 'tree', owner: 'ow\u001bner', reason: 'r\u202e', acquiredAt: 0, expiresAt: 1 } as never;
    expect(describeConflicts([{ path: 'a\u200b.ts', heldBy: held }], 0)).toContain('- a\\u{200b}.ts is covered by ow\\u{1b}ner (src/: r\\u{202e};');
    const changes = [{ path: 'a\u001b[2J.ts', agent: 'x\u0007', tool: 'edit', at: 0, count: 1 }];
    expect(describeChanges(changes, { minutes: 5, limit: 5, paths: ['d\u202e'] }, 0)).toContain('under d\\u{202e}');
    expect(describeChanges(changes, { minutes: 5, limit: 5 }, 0)).toContain('- a\\u{1b}[2J.ts · x\\u{7} via edit');
    expect(deadLetterText({ id: 3, recipients: ['bob'], text: 'hi\u001b[2J there' })).toContain('Its text began: "hi\\u{1b}[2J there"');
    for (const text of [describeConflicts([{ path: 'a\u200b.ts', heldBy: held }], 0), describeChanges(changes, { minutes: 5, limit: 5 }, 0)]) expect(text).not.toMatch(/[\u001b\u0007\u200b\u202e]/u);
  });

  it('describes members with join and last-seen times', () => {
    const text = describeMembers([member('main-aaaaaa', { joinedAt: 0, updatedAt: 58_000, goal: 'ship it' })], 'main-aaaaaa', 60_000);
    expect(text).toContain('main-aaaaaa (you) · main · idle');
    expect(text).toMatch(/goal: ship it[\s\S]*seen 2s ago/);
    expect(describeMembers([], undefined)).toMatch(/No agents/);
    expect(describeMembers([member('main-bbbbbb', { joinedAt: 0, updatedAt: 0, goal: 'fix\u001b[2Jit\u{e0041}' })], undefined, 0)).toContain('  goal: fix\\u{1b}[2Jit\\u{e0041}');
  });
});

describe('panel sanitizing', () => {
  it('strips terminal escapes from other processes\' task, activity and message text', () => {
    const evil = '\u001b]52;c;cHduZWQ=\u0007\u001b[2Jhi';
    const now = Date.now();
    const lines = widgetLines([member('main-aaaaaa', { status: 'working' }), member('general-bbbbbb', { parentId: 'main-aaaaaa', status: 'working', goal: evil, activity: evil })], 'main-aaaaaa', now, theme, 120).join('\n');
    expect(lines).toContain('general-bbbbbb');
    expect(lines).not.toMatch(/\u001b\]52|\u001b\[2J|\u0007/);
    const traffic = trafficRow({ id: 1, from: 'a', to: ['b'], text: evil, at: now, state: 'queued' }, now, theme, 120);
    expect(traffic).not.toMatch(/\u001b\]52|\u001b\[2J|\u0007/);
    expect(traffic).toContain('hi');
  });
});

describe('collaboration prompt', () => {
  it('assigns parent handoff ownership and routes child blockers to the parent', () => {
    expect(octocodePrompt({ octocode: false, profiles: [], canDelegate: true })).toContain('You own user communication, missing approvals, review and integration');
    const tools: Array<{ parameters: { properties: Record<string, { description?: string }> } }> = [];
    registerAgentTool({ registerTool: (tool: never) => tools.push(tool), registerCommand: () => undefined, registerMessageRenderer: () => undefined, on: () => undefined } as never, () => new Map(), { view: { members: [], traffic: [] }, members: () => [], setResumer: () => undefined } as never, fakePermission());
    expect(tools[0]!.parameters.properties['collaborate']?.description).toMatch(/subagents that must coordinate directly/);
    // A child names its parent generically: its own id and parent arrive in its task.
    expect(octocodePrompt({ octocode: false, profiles: [], canDelegate: false, collaborate: true })).toMatch(/your parent[^]*sendMessage/);
  });
});

describe('Team', () => {
  it('injects a parent message into a running child, tracks the reply and enforces reservations', async () => {
    const { vi } = await import('vitest');
    const { Team } = await import('../src/team/session.js');
    const { INTERNAL_MESSAGE_TYPE } = await import('../src/shared/messages.js');
    vi.useFakeTimers();
    const cwd = repo();
    const env = { OCTOCODE_AGENT_DB: path.join(tmp(), 'team.sqlite') };
    const ctx = { cwd, isIdle: () => false } as never;
    const inbox = () => [] as Array<{ message: { content: string; details?: unknown }; options: unknown }>;
    const [mainLog, childLog] = [inbox(), inbox()];
    const fakePi = (log: ReturnType<typeof inbox>) => ({ sendMessage: (message: { content: string }, options: unknown) => log.push({ message, options }) }) as never;
    try {
      const main = new Team(fakePi(mainLog), env);
      main.start(ctx);
      expect(main.members()).toEqual([]);
      const parent = main.join('coordinating');
      const child = new Team(fakePi(childLog), { ...env, OCTOCODE_AGENT_ID: 'researcher-1234', OCTOCODE_PARENT_ID: parent.id, OCTOCODE_AGENT_GOAL: 'Find X' });
      child.start(ctx);
      child.onToolStart();
      child.setActivity('Running localSearch foo');
      child.onUsage({ input: 60, cacheRead: 30, cacheWrite: 10, output: 20, cost: { total: 0.01 } }, 'anthropic/claude-haiku-4-5', false);
      child.onUsage({ input: 5, output: 1, cost: { total: 0 } }, 'local/llama', true);
      // Activity, tool counts and usage are written with the 5-second heartbeat, not on every tick.
      vi.advanceTimersByTime(1_000);
      expect(main.members().find((m) => m.id === 'researcher-1234')).toMatchObject({ toolCalls: 0 });
      vi.advanceTimersByTime(4_000);
      expect(main.members().find((m) => m.id === 'researcher-1234')).toMatchObject({ status: 'working', goal: 'Find X', activity: 'Running localSearch foo', toolCalls: 1, input: 105, cacheRead: 30, output: 21, cost: 0.01, unpriced: true, model: 'local/llama', parentId: parent.id });

      const asked = (await main.send('researcher-1234', 'also check tests', { replyRequired: true }));
      expect(asked).toMatchObject({ sent: ['researcher-1234'] });
      vi.advanceTimersByTime(1_000);
      expect(childLog).toHaveLength(1);
      const content = childLog[0]!.message.content;
      expect(content).toContain(`from your parent agent ${parent.id}`);
      expect(content).toContain('also check tests');
      expect(content).toContain(`replyTo ${(asked as { id: number }).id}`);
      expect(childLog[0]!.options).toEqual({ triggerTurn: true, deliverAs: 'steer' });
      // Awaiting a reply once Pi puts it in front of the child's model.
      child.onContext([{ role: 'custom', customType: INTERNAL_MESSAGE_TYPE, details: childLog[0]!.message.details }]);
      expect(main.members().find((m) => m.id === 'researcher-1234')?.pending).toBe(1);

      // A reply defaults to FYI, so it never asks the asker to answer again (no ping-pong of woken agents).
      (await child.send('parent', 'found it', { replyTo: (asked as { id: number }).id }));
      vi.advanceTimersByTime(1_000);
      expect(mainLog[0]!.message.content).toContain('agent researcher-1234');
      expect(mainLog[0]!.message.content).toContain('FYI: no reply needed');
      expect(main.members().find((m) => m.id === 'researcher-1234')?.pending).toBe(0);

      // Reservations: the child's edit is refused while the parent holds the file.
      expect(main.lock(['src/a.ts'], 'refactor')).toMatchObject({ ok: true });
      expect(child.reservation(path.join(cwd, 'src', 'a.ts'), 'src/a.ts')).toContain(`reserved by ${parent.id}`);
      expect(main.reservation(path.join(cwd, 'src', 'a.ts'), 'src/a.ts')).toBeUndefined();
      expect(child.lock(['src/a.ts'], 'also')).toMatchObject({ ok: false });
      expect(mutatedPaths('file', { queries: [{ path: 'src/a.ts', type: 'edit' }] })).toEqual(['src/a.ts']);
      expect(mutatedPaths('write', { path: 'x' })).toEqual(['x']);
      expect(mutatedPaths('read', { path: 'x' })).toEqual([]);
      // The file tool applies this per query, so a held path fails only its own change.
      const { FileGuard } = await import('../src/files/guard.js');
      const guard = new FileGuard();
      guard.reservedBy = (file) => child.reservation(file, file);
      expect(guard.check(path.join(cwd, 'src', 'a.ts'), 'edit', 'src/a.ts')).toContain(`reserved by ${parent.id}`);
      expect(guard.check(path.join(cwd, 'src', 'free.ts'), 'write', 'src/free.ts')).toBeUndefined();
      main.unlock();
      expect(child.reservation(path.join(cwd, 'src', 'a.ts'), 'src/a.ts')).toBeUndefined();

      expect((await main.send('ghost', 'hi'))).toHaveProperty('error');
      expect((await main.send('researcher-1234', 'from the user', { from: 'user', replyRequired: false }))).toMatchObject({ sent: ['researcher-1234'] });
      child.stop();
      expect(main.members().map((m) => m.id)).toEqual([parent.id]);
      main.stop();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('activity hints', () => {
  it('shows what a browser step is doing', () => {
    expect(toolHint({ url: 'https://example.com', tab_id: 't' })).toBe('https://example.com');
    expect(toolHint({ ref: 'p24:0', input_route: 'dom_event' })).toBe('p24:0');
    // Typed text (a password field, say) never becomes the hint: it is drawn and stored for other sessions.
    expect(toolHint({ ref: 'p1:0', text: 'hunter2' })).toBe('p1:0');
    expect(toolHint({ command: 'curl -H "Authorization: Bearer ghp_abcdefghijklmnopqrstuvwxyz0123456789" x' })).toBe('curl -H "Authorization: Bearer [redacted]" x');
    expect(toolHint({ url: 'https://me:s3cret@example.com/path' })).toBe('https://example.com/path');
    expect(toolHint({ path: 'a.ts', text: 'x' })).toBe('a.ts');
    expect(toolHint(undefined)).toBe('');
  });
});
