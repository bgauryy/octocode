import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { visibleWidth } from '@earendil-works/pi-tui';
import { widgetLines } from '../src/team/panel.js';
import type { Member } from '../src/team/model.js';
import { TeamStore } from '../src/team/store.js';
import { theme } from './fake-pi.js';
import { tmp } from './helpers.js';

const NOW = 1_000_000;
const member = (id: string, extra: Partial<Member> = {}): Member => ({
  id, role: id.replace(/-[0-9a-f]{6}$/, ''), pid: process.pid, status: 'idle', joinedAt: NOW - 60_000, updatedAt: NOW, toolCalls: 0, input: 0, output: 0, cost: 0, ...extra,
});

describe('agents panel', () => {
  const team = [
    member('main-aaaaaa'),
    member('researcher-3fa9', { parentId: 'main-aaaaaa', status: 'working', joinedAt: NOW - 134_000, toolCalls: 12, input: 12_000, output: 3_000, cost: 0.04, task: 'Find where tools register', activity: 'localSearch registerTool' }),
    member('reviewer-a1b2', { parentId: 'main-aaaaaa', joinedAt: NOW - 45_000, toolCalls: 3, input: 4_000, output: 1_000, task: 'Review the diff', pending: 1, locks: ['src/a.ts'] }),
    member('general-77aa', { parentId: 'researcher-3fa9', status: 'working', joinedAt: NOW - 5_000, toolCalls: 1, activity: 'bash yarn test' }),
  ];

  it('aligns the columns: every row puts state, time and tools at the same offsets', () => {
    const lines = widgetLines(team, 'main-aaaaaa', NOW, theme, 160);
    const rows = lines.slice(1, 4);
    const at = (needle: RegExp) => rows.map((row) => row.search(needle));
    expect(new Set(at(/working|idle/)).size).toBe(1);
    // Right-aligned: every tool-call count ends at the same offset.
    expect(new Set(rows.map((row) => { const hit = /\d+ tool calls?/.exec(row)!; return hit.index + hit[0].length; })).size).toBe(1);
    expect(rows.every((row) => visibleWidth(row) <= 160)).toBe(true);
  });

  it('shows the header totals, state, age, stats and what each agent is doing, working first', () => {
    const lines = widgetLines(team, 'main-aaaaaa', NOW, theme, 160);
    expect(lines[0]).toBe(' agents  2 working · 1 idle · 3 in this session · ↑16k ↓4k · $0.04');
    expect(lines[1]).toMatch(/^ ● researcher-3fa9 +working +2m14s +12 tool calls +↑12k +↓3k +\$0\.04 +Find where tools register › localSearch registerTool$/);
    // A grandchild sits under its parent, indented.
    expect(lines[2]).toMatch(/^ ● {2}└ general-77aa +working +5s +1 tool call +↑0 +↓0 +bash yarn test$/);
    expect(lines[3]).toMatch(/^ ○ reviewer-a1b2 +idle +45s +3 tool calls +↑4k +↓1k +✉ 1 +locks 1 +Review the diff$/);
  });

  it('drops the stats columns before the activity on narrow terminals', () => {
    const narrow = widgetLines(team, 'main-aaaaaa', NOW, theme, 70);
    expect(narrow.every((line) => visibleWidth(line) <= 70)).toBe(true);
    expect(narrow[1]).not.toContain('$0.04');
    expect(narrow[1]).not.toContain('↑12k');
    expect(narrow[1]).toContain('working');
    expect(narrow[1]).toContain('Find where tools');
  });

  it('hides agents whose heartbeat stopped (killed or hung) and shows nothing when none are left', () => {
    const stale = [member('main-aaaaaa'), member('general-dead', { parentId: 'main-aaaaaa', status: 'working', updatedAt: NOW - 20_000 })];
    expect(widgetLines(stale, 'main-aaaaaa', NOW, theme, 120)).toEqual([]);
    const mixed = widgetLines([...stale, member('general-live', { parentId: 'main-aaaaaa' })], 'main-aaaaaa', NOW, theme, 120).join('\n');
    expect(mixed).toContain('general-live');
    expect(mixed).not.toContain('general-dead');
  });
});

/** A member whose heartbeat is current in wall-clock time (the store stamps real times). */
const live = (id: string, extra: Partial<Member> = {}): Member => member(id, { updatedAt: Date.now(), ...extra });
const repo = () => {
  const dir = tmp();
  fs.mkdirSync(path.join(dir, '.git'));
  return dir;
};
const open = (cwd: string, db: string) => TeamStore.open(cwd, db);
const sent = (result: { id: number | undefined }) => result.id!;

describe('panel traffic', () => {
  it('keeps this session\'s subagent messages when other sessions are busier', () => {
    const store = open(repo(), path.join(tmp(), 'team.sqlite'));
    store.save(live('main-aaaaaa'));
    store.save(live('researcher-bbbbbb', { parentId: 'main-aaaaaa' }));
    store.save(live('tester-cccccc', { parentId: 'main-aaaaaa' }));
    store.save(live('main-dddddd'));
    store.save(live('helper-eeeeee', { parentId: 'main-dddddd' }));
    const mine = sent(store.send('researcher-bbbbbb', ['tester-cccccc'], 'ours', { replyRequired: false }));
    for (let index = 0; index < 5; index += 1) store.send('main-dddddd', ['helper-eeeeee'], `theirs ${index}`, { replyRequired: false });
    const traffic = store.recent();
    expect(traffic.map((message) => message.id)).toContain(mine);
    const lines = widgetLines([live('main-aaaaaa'), live('researcher-bbbbbb', { parentId: 'main-aaaaaa' }), live('tester-cccccc', { parentId: 'main-aaaaaa' })], 'main-aaaaaa', Date.now(), theme, 200, traffic);
    expect(lines.join('\n')).toContain('#' + mine);
    expect(lines.join('\n')).not.toContain('theirs');
  });

  it('shows at most three messages and cuts long rows with a single-character ellipsis', () => {
    const now = Date.now();
    const panel = [live('main-aaaaaa'), live('researcher-bbbbbb', { parentId: 'main-aaaaaa', task: 'x'.repeat(300) })];
    const traffic = [1, 2, 3, 4, 5].map((id) => ({ id, from: 'researcher-bbbbbb', to: ['tester-c'], text: 'y'.repeat(200), at: now, state: 'delivered' as const }));
    const lines = widgetLines(panel, 'main-aaaaaa', now, theme, 60, traffic);
    expect(lines.filter((line) => line.includes('✉'))).toHaveLength(3);
    const cut = lines.filter((line) => line.includes('…'));
    expect(cut.length).toBeGreaterThan(0);
    expect(lines.join('\n')).not.toContain('...');
  });
});
