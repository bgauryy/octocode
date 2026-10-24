import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { visibleWidth } from '@earendil-works/pi-tui';
import { widgetLines } from '../src/team/panel.js';
import { describeMembers, messageText, resolveTarget } from '../src/team/routing.js';
import { AGENT_NAMES } from '../src/team/identity.js';
import { Team } from '../src/team/session.js';
import { registerCollab } from '../src/team/tools.js';
import { AGENT_GOAL_ENV, AGENT_ID_ENV, AGENT_ROLE_ENV, TeamStore } from '../src/team/store.js';
import type { Member } from '../src/team/model.js';
import { fakeCtx, fakePi, theme } from './fake-pi.js';
import { repo, tmp } from './helpers.js';

const NOW = 1_000_000;
const member = (id: string, role: string, extra: Partial<Member> = {}): Member => ({
  id, role, pid: process.pid, status: 'idle', joinedAt: NOW - 60_000, updatedAt: NOW, toolCalls: 0, input: 0, cacheRead: 0, output: 0, cost: 0, unpriced: false, ...extra,
});
const session = (env: NodeJS.ProcessEnv, cwd: string, db: string) => {
  const fake = fakePi();
  const team = new Team(fake.pi, { OCTOCODE_AGENT_DB: db, ...env });
  team.start(fakeCtx({ cwd }));
  return { team, ...fake };
};

describe('agent identity on the team', () => {
  it('names a main session from the closed list, with the role main and the join note as its goal', () => {
    const { team } = session({}, repo(), path.join(tmp(), 'team.sqlite'));
    const me = team.join('audit auth token refresh');
    expect(AGENT_NAMES).toContain(me.id);
    expect(me).toMatchObject({ role: 'main', goal: 'audit auth token refresh' });
    // Joining again keeps the name and replaces the goal.
    expect(team.join('write the docs').id).toBe(me.id);
    expect(team.members()[0]).toMatchObject({ id: me.id, role: 'main', goal: 'write the docs' });
  });

  it('gives each process its own name: never one a live member of the shared database holds', () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const names = new Set<string>();
    for (let i = 0; i < 12; i++) names.add(session({}, cwd, db).team.join().id);
    expect(names.size).toBe(12);
    const store = TeamStore.open(cwd, db);
    expect(store.names().sort()).toEqual([...names].sort());
    store.close();
  });

  it('proposes a new name that avoids live members, finished subagents and names the caller holds', () => {
    const { team } = session({}, repo(), path.join(tmp(), 'team.sqlite'));
    const me = team.join();
    const finished = AGENT_NAMES.find((name) => name !== me.id)!;
    team.rememberFinished(finished, 'its report reached you.');
    const held = AGENT_NAMES.find((name) => name !== me.id && name !== finished)!;
    for (let i = 0; i < 200; i++) expect([me.id, finished, held]).not.toContain(team.newName([held]));
  });

  it('takes a subagent\'s name, role and goal from the parent that started it', () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const parent = session({}, cwd, db).team.join();
    const child = session({ [AGENT_ID_ENV]: 'inky-kraken', [AGENT_ROLE_ENV]: 'reviewer', [AGENT_GOAL_ENV]: 'audit auth token refresh', OCTOCODE_PARENT_ID: parent.id }, cwd, db);
    expect(child.team.join()).toMatchObject({ id: 'inky-kraken', role: 'reviewer', goal: 'audit auth token refresh', parentId: parent.id });
  });

  it('still resolves a target by its role', () => {
    const members = [member('main-aaaaaa', 'main'), member('inky-kraken', 'reviewer'), member('salty-squid', 'implementer')];
    expect((resolveTarget(members, 'main-aaaaaa', 'reviewer') as Member[]).map((m) => m.id)).toEqual(['inky-kraken']);
    expect((resolveTarget(members, 'main-aaaaaa', 'salty-squid') as Member[]).map((m) => m.id)).toEqual(['salty-squid']);
  });

  it('refuses a blank coordinate join goal and accepts a long one whole', async () => {
    const fake = fakePi();
    const team = new Team(fake.pi, { OCTOCODE_AGENT_DB: path.join(tmp(), 'team.sqlite') });
    registerCollab(fake.pi, team, { stopAgent: () => undefined });
    const ctx = fakeCtx({ cwd: repo() });
    await fake.emit('session_start', {}, ctx);
    const coordinate = fake.tools.get('coordinate');
    await expect(coordinate.execute('c', { action: 'join', goal: '   ' }, undefined, undefined, ctx)).rejects.toThrow(/goal is required/);
    expect(team.id).toBeUndefined();
    await coordinate.execute('c', { action: 'join', goal: 'refactor the whole auth module today please' }, undefined, undefined, ctx);
    expect(team.members()[0]).toMatchObject({ role: 'main', goal: 'refactor the whole auth module today please' });
    expect(coordinate.parameters.properties.goal.description).toMatch(/goal.*48 characters fit/);
    expect(coordinate.parameters.properties.goal).not.toHaveProperty('maxLength');
    await fake.emit('session_shutdown', {}, ctx);
  });
});

describe('what the team shows about an agent', () => {
  const team = [
    member('salty-squid', 'main', { goal: 'ship the release' }),
    member('inky-kraken', 'implementer', { parentId: 'salty-squid', status: 'working', joinedAt: NOW - 518_000, toolCalls: 56, cost: 2.28, goal: 'audit auth token refresh', activity: 'Thinking', model: 'claude-opus-5-5', locks: Array.from({ length: 20 }, (_, i) => `src/${i}.ts`), pending: 0 }),
    member('deep-nautilus', 'reviewer', { parentId: 'salty-squid', joinedAt: NOW - 45_000, goal: 'review parser changes' }),
  ];

  it('draws the panel row as name · goal · model · state · time, within the width', () => {
    const lines = widgetLines(team, 'salty-squid', NOW, theme, 150);
    expect(lines[1]).toBe(' ● inky-kraken   · audit auth token refresh · claude-opus-5-5 · working · 8m38s · 56 tool calls · ≈$2.28 · 20 locks  Thinking');
    expect(lines[2]).toBe(' ○ deep-nautilus · review parser changes    ·                 · idle    ·   45s ·  0 tool calls');
    for (const width of [150, 100, 70, 40]) for (const line of widgetLines(team, 'salty-squid', NOW, theme, width)) expect(visibleWidth(line)).toBeLessThanOrEqual(width);
  });

  it('keeps name, model, state and time when the terminal is narrow, dropping stats and shortening the goal first', () => {
    const narrow = widgetLines(team, 'salty-squid', NOW, theme, 70)[1]!;
    for (const kept of ['inky-kraken', 'claude-opus-5-5', 'working', '8m38s']) expect(narrow).toContain(kept);
    expect(narrow).toMatch(/audit auth[^·]*…/);
    expect(narrow).not.toContain('implementer');
    expect(narrow).not.toContain('≈$2.28');
  });

  it('lists name, role and goal in `coordinate list` and /agents', () => {
    const text = describeMembers(team, 'salty-squid', NOW + 10_000);
    expect(text).toContain('inky-kraken · implementer · working · claude-opus-5-5 · parent salty-squid');
    expect(text).toContain('salty-squid (you) · main · idle');
    expect(text).toContain('  goal: audit auth token refresh');
    expect(text).toContain('  Thinking');
    expect(text).toContain('  goal: review parser changes');
    expect(text).toContain('56 tool calls · ≈$2.28');
    const cached = describeMembers([member('salty-squid', 'main', { toolCalls: 3, input: 1_200_000, cacheRead: 1_128_000, output: 35_000, unpriced: true })], 'salty-squid', NOW);
    expect(cached).toContain('3 tool calls · ↑1.2M (94% cached) ↓35k · cost n/a');
  });

  it('names the sender\'s role and goal in the header of a message', () => {
    const base = { id: 12, to: 'salty-squid', text: 'found it', at: 0, replyRequired: true };
    const peer = messageText({ ...base, from: 'inky-kraken', sender: { role: 'reviewer', goal: 'audit auth token refresh' } });
    expect(peer).toContain('Message #12 from agent inky-kraken (reviewer: audit auth token refresh) at ');
    expect(messageText({ ...base, from: 'inky-kraken', sender: { role: 'reviewer', goal: 'audit auth token refresh' } }, { parentId: 'inky-kraken' })).toContain('from your parent agent inky-kraken (reviewer: audit auth token refresh) at ');
    expect(messageText({ ...base, from: 'inky-kraken', sender: { role: 'reviewer' } })).toContain('from agent inky-kraken (reviewer) at ');
    // The sender already left: only its name remains.
    expect(messageText({ ...base, from: 'inky-kraken' })).toContain('from agent inky-kraken at ');
    expect(messageText({ ...base, from: 'user' }, { fromUser: true })).toContain('from the user at ');
  });

  it('reads the sender\'s role and goal from the team database when a message is delivered', () => {
    const cwd = repo();
    const store = TeamStore.open(cwd, path.join(tmp(), 'team.sqlite'));
    store.save(member('inky-kraken', 'reviewer', { goal: 'audit auth token refresh', joinedAt: NOW, pid: process.pid }));
    store.save(member('salty-squid', 'main'));
    store.send('inky-kraken', ['salty-squid'], 'found it', { replyRequired: false }, NOW);
    expect(store.pending('salty-squid')[0]).toMatchObject({ from: 'inky-kraken', sender: { role: 'reviewer', goal: 'audit auth token refresh' } });
    store.close();
  });

  it('strips terminal escapes from another process\'s goal and role', () => {
    const evil = [member('x-y', 'main'), member('evil-agent', 'rev\u001b[2Jiewer', { parentId: 'x-y', goal: 'goal\u001b]0;pwned\u0007 here', status: 'working' })];
    expect(widgetLines(evil, 'x-y', NOW, theme, 150).join('\n')).not.toMatch(/\u001b|pwned/);
    // The model reads this text: the controls stay as visible escapes, never raw.
    const described = describeMembers(evil, 'x-y', NOW);
    expect(described).not.toMatch(/\u001b|\u0007/);
    expect(described).toContain('rev\\u{1b}[2Jiewer');
    expect(described).toContain('goal: goal\\u{1b}]0;pwned\\u{7} here');
  });
});
