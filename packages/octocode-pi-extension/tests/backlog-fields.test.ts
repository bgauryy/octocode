import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { SECRET, setup, storeFor, useBacklogHome } from './backlog-harness.js';
import { fakeCtx } from './fake-pi.js';

useBacklogHome();

const FIELDS = {
  goal: 'Search finds an item by path; focused tests pass.',
  paths: ['src/backlog/store.ts', 'src/backlog/'],
  why: 'Agents lose the reason behind an item after a resume.',
  evidence: ['src/backlog/store.ts:87 — body copied into notes', 'yarn test → 2 failures'],
  plan: 'Add four columns, then render them.\nKeep search on LIKE.',
};

describe('backlog fields (where, why, evidence, plan): store', () => {
  it('stores each field with entries cleaned to single lines, and finds an item by a path substring or an evidence word', () => {
    const store = storeFor('r');
    const item = store.add({ title: 'search', ...FIELDS, paths: ['  src/backlog/store.ts ', 'src/backlog/store.ts', '', 'docs/\nA.md'], createdBy: 'user' });
    expect(item).toMatchObject({ paths: ['src/backlog/store.ts', 'docs/ A.md'], why: FIELDS.why, evidence: FIELDS.evidence, plan: FIELDS.plan });
    store.add({ title: 'unrelated', createdBy: 'user' });
    expect(store.get('B2')).toMatchObject({ paths: [], why: '', evidence: [], plan: '' });
    const found = (query: string) => store.list({ query }).items.map((each) => each.ref);
    expect(found('backlog/store')).toEqual(['B1']);
    expect(found('2 failures')).toEqual(['B1']);
    expect(found('after a resume')).toEqual(['B1']);
    expect(found('four columns')).toEqual(['B1']);
    expect(found('100%')).toEqual([]);
  });

  it('logs every changed field with the old value in full, and nothing when a field is set to what it was', () => {
    const store = storeFor('r');
    const item = store.add({ title: 'fields', ...FIELDS, state: 'todo', createdBy: 'user' });
    store.update(item.ref, { paths: ['src/a.ts'], why: 'new why', evidence: ['one'], plan: 'new plan' }, { id: 's1' });
    store.update(item.ref, { paths: ['src/a.ts'], why: 'new why', evidence: ['one'], plan: 'new plan' }, { id: 's1' });
    const blank = store.add({ title: 'blank', createdBy: 'user' });
    store.update(blank.ref, { why: 'w', evidence: ['e'], plan: 'p', paths: ['x'] }, { id: 's1' });
    const [first, ...more] = store.notes(item, 50);
    expect(more).toEqual([]);
    expect(first.text).toBe(
      `paths src/backlog/store.ts, src/backlog/ → src/a.ts · why replaced; it was:\n${FIELDS.why} · evidence replaced; it was:\n${FIELDS.evidence.join('\n')} · plan replaced; it was:\n${FIELDS.plan}`,
    );
    expect(store.notes(blank)[0].text).toBe('paths none → x · why added · evidence added · plan added');
    expect(store.update(item.ref, { paths: [] }, { id: 's1' }).paths).toEqual([]);
  });

  it('refuses a secret in any new field', () => {
    const store = storeFor('r');
    for (const field of ['why', 'plan']) expect(() => store.add({ title: 't', [field]: `use ${SECRET}`, createdBy: 'user' })).toThrow(new RegExp(`${field} .*secret`));
    expect(() => store.add({ title: 't', evidence: ['fine', SECRET], createdBy: 'user' })).toThrow(/evidence .*secret/);
    const item = store.add({ title: 't', createdBy: 'user' });
    expect(() => store.update(item.ref, { paths: [SECRET] }, { id: 's1' })).toThrow(/paths .*secret/);
  });
});

describe('backlog fields (where, why, evidence, plan): tool', () => {
  it('adds with every field through the tool; get shows Goal, Where, Why, Evidence, Plan, then times, body and notes; list stays one line', async () => {
    const t = setup();
    expect(await t.text({ op: 'add', title: 'Path search', ...FIELDS, body: 'More background.', state: 'todo' })).toBe('Added B1 [todo p2] Path search');
    await t.text({ op: 'update', id: 'B1', note: 'looked at it' });
    const detail = await t.text({ op: 'get', id: 'B1' });
    const order = ['Goal: Search finds', 'Where: src/backlog/store.ts, src/backlog/', 'Why: Agents lose', 'Evidence:\n- src/backlog/store.ts:87 — body copied into notes\n- yarn test → 2 failures', 'Plan: Add four columns, then render them.\n  Keep search on LIKE.', 'created ', 'More background.', 'Notes and changes:'];
    const at = order.map((part) => detail.indexOf(part));
    expect(at.every((index) => index >= 0)).toBe(true);
    const ascending = [...at];
    ascending.sort((a, b) => a - b);
    expect(ascending).toEqual(at);
    expect(await t.text({ op: 'list' })).toBe('B1 [todo p2] Path search');
    expect(await t.text({ op: 'list', query: 'src/backlog/' })).toBe('B1 [todo p2] Path search');
    expect(await t.text({ op: 'update', id: 'B1', paths: ['src/x.ts'], plan: 'Replace it.' })).toBe('Updated B1 [todo p2] Path search');
    expect(await t.text({ op: 'get', id: 'B1' })).toContain('Where: src/x.ts\n');
    expect(await t.text({ op: 'get', id: 'B1' })).toContain('Plan: Replace it.\n');
  });

  it('omits the sections an item leaves empty', async () => {
    const t = setup();
    await t.text({ op: 'add', title: 'Plain', goal: 'It works.', why: 'Because.' });
    const detail = await t.text({ op: 'get', id: 'B1' });
    expect(detail).toContain('Goal: It works.');
    expect(detail).toContain('Why: Because.');
    for (const heading of ['Where:', 'Evidence:', 'Plan:']) expect(detail).not.toContain(heading);
  });

  it('requires a goal when an agent adds, before anything is stored, but not when the user adds', async () => {
    const t = setup();
    await expect(t.call({ op: 'add', title: 'no goal' })).rejects.toThrow(/add needs a goal/);
    await expect(t.call({ op: 'add', title: 'blank goal', goal: '  ' })).rejects.toThrow(/add needs a goal/);
    expect(storeFor(fs.realpathSync(t.cwd)).list({ states: ['backlog'] }).total).toBe(0);
    await t.command.handler('add by the user', fakeCtx({ cwd: t.cwd, hasUI: false }));
    expect(storeFor(fs.realpathSync(t.cwd)).get('B1')).toMatchObject({ title: 'by the user', goal: '' });
  });

  it('does not let an agent clear a goal by update, and still lets it change one', async () => {
    const t = setup();
    await t.text({ op: 'add', title: 'Item', goal: 'Tests pass.' });
    for (const goal of ['', ' \n ']) await expect(t.call({ op: 'update', id: 'B1', goal })).rejects.toThrow(/update cannot clear the goal/);
    expect(storeFor(fs.realpathSync(t.cwd)).get('B1')).toMatchObject({ goal: 'Tests pass.' });
    await t.text({ op: 'update', id: 'B1', goal: 'Tests and lint pass.' });
    expect(storeFor(fs.realpathSync(t.cwd)).get('B1')).toMatchObject({ goal: 'Tests and lint pass.' });
  });

  it('takes paths and evidence as lists', () => {
    const { properties } = setup().tool.parameters;
    expect(properties.paths.type).toBe('array');
    expect(properties.evidence.type).toBe('array');
  });
});

describe('backlog fields (where, why, evidence, plan): subagents, do and export', () => {
  it('lets a subagent propose with every field, but update only state and notes', async () => {
    const parent = setup();
    await parent.text({ op: 'add', title: 'delegated', goal: 'Done.', state: 'todo' });
    storeFor(fs.realpathSync(parent.cwd)).update('B1', { state: 'ongoing' }, { id: 'parent-session' });
    const sub = setup({ isSubagent: true, agentId: 'general-1', parentSession: 'parent-session' });
    expect(Object.keys(sub.tool.parameters.properties)).toEqual(expect.arrayContaining(['paths', 'why', 'evidence', 'plan']));
    await sub.text({ op: 'add', title: 'found', ...FIELDS, from: 'B1' });
    expect(storeFor(fs.realpathSync(sub.cwd)).get('B2')).toMatchObject({ paths: FIELDS.paths, evidence: FIELDS.evidence, why: FIELDS.why, plan: FIELDS.plan });
    await expect(sub.call({ op: 'add', title: 'no goal', why: 'x' })).rejects.toThrow(/add needs a goal/);
    for (const change of [{ paths: ['x'] }, { why: 'x' }, { evidence: ['x'] }, { plan: 'x' }]) {
      await expect(sub.call({ op: 'update', id: 'B1', ...change })).rejects.toThrow(/only the state and notes/);
    }
  });

  it('hands every saved field to the agent with "do"', async () => {
    const t = setup();
    storeFor(fs.realpathSync(t.cwd)).add({ title: 'Fix search', ...FIELDS, body: 'Extra.', state: 'todo', createdBy: 'user' });
    await t.command.handler('do B1', fakeCtx({ cwd: t.cwd, hasUI: false }));
    const message = t.sent.at(-1)!.message;
    expect(message).toContain('Where: src/backlog/store.ts, src/backlog/');
    expect(message).toContain('Why: Agents lose');
    expect(message).toContain('- yarn test → 2 failures');
    expect(message).toContain('Plan: Add four columns');
    expect(message).toContain('Extra.');
  });

  it('shows the fields in /backlog <id> and in the markdown export', async () => {
    const t = setup();
    storeFor(fs.realpathSync(t.cwd)).add({ title: 'Fix search', ...FIELDS, state: 'todo', createdBy: 'user' });
    const ctx = fakeCtx({ cwd: t.cwd, hasUI: false });
    await t.command.handler('B1', ctx);
    const shown = ctx.ui.notes.at(-1)!.message;
    expect(shown).toMatch(/Goal: .*\nWhere: src\/backlog\/store\.ts, src\/backlog\/\nWhy: .*\nEvidence:\n- .*\n- .*\nPlan: /);
    await t.command.handler('export', ctx);
    const markdown = fs.readFileSync(path.join(t.cwd, '.octocode', 'backlog.md'), 'utf8');
    expect(markdown).toContain('  Where: src/backlog/store.ts, src/backlog/\n  Why: Agents lose the reason behind an item after a resume.\n  Evidence:\n  - src/backlog/store.ts:87');
    expect(markdown).toContain('  Plan: Add four columns, then render them.\n    Keep search on LIKE.');
  });
});
