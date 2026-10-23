import fs from 'node:fs';
import path from 'node:path';
import type { TObject } from 'typebox';
import { describe, expect, it } from 'vitest';
import { closeSharedAgentDb } from '../src/agentdb/db.js';
import { countsText } from '../src/backlog/format.js';
import { SECRET, session, setup, storeFor, useBacklogHome } from './backlog-harness.js';
import { fakeCtx, rendered, theme } from './fake-pi.js';

useBacklogHome();

/** The arguments of the `Next: backlog …` continuation a page ends with. */
const next = (text: string) => JSON.parse(text.match(/^Next: backlog (.+)$/m)![1]);

describe('backlog tool: adding, listing and paging', () => {
  it('pages every matching item and every note with executable continuations', async () => {
    const t = setup();
    for (let i = 0; i < 53; i++) await t.text({ op: 'add', title: `Task ${i}`, goal: 'it works', state: 'todo' });
    const first = await t.text({ op: 'list', limit: 50 });
    const second = await t.text(next(first));
    const ids = [...`${first}\n${second}`.matchAll(/^(B\d+) \[/gm)].map((match) => match[1]);
    expect(ids).toHaveLength(53);
    expect(new Set(ids).size).toBe(53);
    expect(second).not.toContain('Next:');

    for (let i = 0; i < 23; i++) await t.text({ op: 'update', id: 'B1', note: `note-${i}` });
    const pages: string[] = [];
    let page = await t.text({ op: 'get', id: 'B1' });
    while (true) {
      pages.push(page);
      if (!page.includes('Next: backlog')) break;
      expect(pages.length).toBeLessThan(4);
      page = await t.text(next(page));
    }
    const notes = [...pages.join('\n').matchAll(/: (note-\d+)$/gm)].map((match) => match[1]);
    expect(notes).toHaveLength(23);
    expect(new Set(notes).size).toBe(23);
  });

  it('adds proposals to backlog, lists open work with a triage line, and gets details', async () => {
    const t = setup();
    expect(await t.text({ op: 'add', title: 'Add memory tool', goal: 'it works', body: 'with BM25', priority: 'p1', tags: ['mem'] })).toBe('Added B1 [backlog p1] Add memory tool #mem');
    await t.text({ op: 'add', title: 'Accepted', goal: 'it works', state: 'todo' });
    expect(await t.text({ op: 'list' })).toBe('B2 [todo p2] Accepted\n1 proposed item(s) await triage (list with states: ["backlog"]).');
    expect(await t.text({ op: 'update', id: 'B1', state: 'ongoing', note: 'starting' })).toBe('Updated B1 [ongoing p1] Add memory tool #mem (you)');
    const detail = await t.text({ op: 'get', id: 'b1' });
    expect(detail).toContain('B1 [ongoing p1] Add memory tool #mem (you)');
    expect(detail).toContain('with BM25');
    expect(detail).toMatch(/Notes and changes:\n- .* ago you · state backlog → ongoing · assignee none → \S+\n- .* ago you: starting/);
    expect(detail).not.toMatch(/version/);
    expect(await t.text({ op: 'list', states: ['todo'], limit: 1, query: 'Acc' })).toContain('B2 [todo p2] Accepted');
  });

  it('requires a note to finish and refuses secrets', async () => {
    const t = setup();
    await t.text({ op: 'add', title: 'task', goal: 'it works', state: 'todo' });
    await expect(t.call({ op: 'update', id: 'B1', state: 'done' })).rejects.toThrow(/needs a note/);
    await expect(t.call({ op: 'add', title: 'token', goal: 'it works', body: `use ${SECRET}` })).rejects.toThrow(/secret/);
    await expect(t.call({ op: 'update', id: 'B1', note: SECRET })).rejects.toThrow(/secret/);
    await expect(t.call({ op: 'update', id: 'B1', tags: [SECRET] })).rejects.toThrow(/secret/);
    // A secret note on add is refused before anything is stored.
    await expect(t.call({ op: 'add', title: 'fast', goal: 'it works', state: 'done', note: SECRET })).rejects.toThrow(/secret/);
    expect(await t.text({ op: 'list', states: ['todo'] })).toBe('B1 [todo p2] task');
    await expect(t.call({ op: 'update', id: 'B1' })).rejects.toThrow(/needs something to change/);
    await expect(t.call({ op: 'get' })).rejects.toThrow(/needs an id/);
    await expect(t.call({ op: 'get', id: 'B9' })).rejects.toThrow(/No backlog item B9/);
    expect(await t.text({ op: 'update', id: 'B1', state: 'done', note: 'fixed; yarn test passes' })).toBe('Updated B1 [done p2] task');
    expect(t.tool.executionMode).toBeUndefined();
    expect(t.tool.parameters.properties.op.enum).toEqual(['list', 'get', 'add', 'update', 'remove']);
  });

  it('refuses an add it cannot complete before creating anything, so a retry leaves no duplicate', async () => {
    const t = setup();
    await expect(t.call({ op: 'add', title: 'shipped', goal: 'it works', state: 'done' })).rejects.toThrow(/note/);
    await expect(t.call({ op: 'add', title: 'idea', goal: 'it works', note: 'context' })).rejects.toThrow(/note only with state ongoing or done/);
    expect(storeFor(fs.realpathSync(t.cwd)).counts()).toMatchObject({ backlog: 0, todo: 0, done: 0 });
    expect(await t.text({ op: 'add', title: 'shipped', goal: 'it works', state: 'done', note: 'merged; tests pass' })).toMatch(/^Added B1 \[done/);
  });
});

describe('backlog tool: claims, subagents and links', () => {
  it('removes an item and its notes, but not one another live session holds', async () => {
    const t = setup();
    await t.text({ op: 'add', title: 'dup', goal: 'it works', state: 'todo' });
    await t.text({ op: 'update', id: 'B1', note: 'a note' });
    expect(await t.text({ op: 'remove', id: 'b1' })).toBe('Removed B1 "dup" and its notes.');
    await expect(t.call({ op: 'get', id: 'B1' })).rejects.toThrow(/No backlog item B1/);
    await expect(t.call({ op: 'remove', id: 'B1' })).rejects.toThrow(/No backlog item/);
    await expect(t.call({ op: 'remove' })).rejects.toThrow(/needs an id/);
    await t.text({ op: 'add', title: 'held', goal: 'it works', state: 'todo' });
    session('other-session', process.ppid);
    storeFor(fs.realpathSync(t.cwd)).update('B2', { state: 'ongoing' }, { id: 'other-session' });
    await expect(t.call({ op: 'remove', id: 'B2' })).rejects.toThrow(/ongoing for other-session/);
  });

  it('refuses to claim an item another live session holds, naming it', async () => {
    const t = setup();
    await t.text({ op: 'add', title: 'task', goal: 'it works', state: 'todo' });
    session('other-session', process.ppid);
    storeFor(fs.realpathSync(t.cwd)).update('B1', { state: 'ongoing' }, { id: 'other-session' });
    await expect(t.call({ op: 'update', id: 'B1', state: 'ongoing' })).rejects.toThrow(/other-session/);
  });

  it('in a subagent: adds only to backlog and updates only its or its parent\'s items', async () => {
    const parent = setup();
    await parent.text({ op: 'add', title: 'delegated', goal: 'it works', state: 'todo' });
    await parent.text({ op: 'add', title: 'unrelated', goal: 'it works', state: 'todo' });
    storeFor(fs.realpathSync(parent.cwd)).update('B1', { state: 'ongoing' }, { id: 'parent-session' });
    const sub = setup({ isSubagent: true, agentId: 'general-1', parentSession: 'parent-session' });
    expect(sub.tool.parameters.properties.op.enum).toEqual(['list', 'get', 'add', 'update']);
    expect(sub.tool.parameters.properties.state.enum).toEqual(['ongoing', 'done']);
    expect(sub.tool.description).toContain('its parent has claimed');
    expect(sub.tool.parameters.properties.op.description).toContain('propose an item');
    expect(sub.tool.parameters.properties.op.description).not.toContain('remove');
    expect(sub.tool.promptGuidelines.join(' ')).not.toContain('set it ongoing');
    expect(await sub.text({ op: 'list' })).toBe('B1 [ongoing p2] delegated (parent)\nB2 [todo p2] unrelated');
    expect(await sub.text({ op: 'add', title: 'follow-up', goal: 'it works', state: 'todo' })).toBe('Added B3 [backlog p2] follow-up');
    expect(storeFor(fs.realpathSync(sub.cwd)).get('B3')!.createdBy).toBe('agent:general-1');
    // A proposal takes no note, and the parent has not claimed it, so the hint points at body, not a later update.
    await expect(sub.call({ op: 'add', title: 'with note', goal: 'it works', note: 'details' })).rejects.toThrow('A subagent adds without a note: put the details in body.');
    await expect(sub.call({ op: 'update', id: 'B2', state: 'ongoing' })).rejects.toThrow(/not claimed by your parent/);
    await expect(sub.call({ op: 'update', id: 'B1', title: 'renamed' })).rejects.toThrow(/only the state and notes/);
    await expect(sub.call({ op: 'update', id: 'B1', state: 'todo' })).rejects.toThrow(/sets only ongoing or done/);
    expect(storeFor(fs.realpathSync(sub.cwd)).get('B1')!.state).toBe('ongoing');
    expect(await sub.text({ op: 'update', id: 'B1', state: 'done', note: 'done; verified' })).toBe('Updated B1 [done p2] delegated');
    await expect(sub.call({ op: 'remove', id: 'B3' })).rejects.toThrow(/subagent cannot remove/);
  });

  it('links follow-ups with from: shows the origin and derived items in get, filters list, and lets a subagent link only on add', async () => {
    const t = setup();
    await t.text({ op: 'add', title: 'Root task', goal: 'it works', state: 'todo' });
    expect(await t.text({ op: 'add', title: 'Follow-up', goal: 'it works', from: 'B1' })).toBe('Added B2 [backlog p2] Follow-up (from B1)');
    const root = await t.text({ op: 'get', id: 'B1' });
    expect(root).toContain('Derived items (1):\n- B2 [backlog p2] Follow-up (from B1)');
    const child = await t.text({ op: 'get', id: 'B2' });
    expect(child).toContain('From: B1 [todo p2] Root task');
    expect(child).toMatch(/- .* ago agent · created from B1/);
    expect(await t.text({ op: 'list', from: 'B1', states: ['backlog'] })).toBe('B2 [backlog p2] Follow-up (from B1)');
    expect(await t.text({ op: 'update', id: 'B2', from: '' })).toBe('Updated B2 [backlog p2] Follow-up');
    await expect(t.call({ op: 'add', title: 'x', goal: 'it works', from: 'B9' })).rejects.toThrow(/No backlog item B9 to link from/);
    expect(rendered(t.tool.renderCall({ op: 'add', from: 'B1' }, theme, {}))).toContain('from B1');

    storeFor(fs.realpathSync(t.cwd)).update('B1', { state: 'ongoing' }, { id: 'parent-session' });
    const sub = setup({ isSubagent: true, agentId: 'general-1', parentSession: 'parent-session' });
    expect(await sub.text({ op: 'add', title: 'Found while working', goal: 'it works', from: 'B1' })).toBe('Added B3 [backlog p2] Found while working (from B1)');
    await expect(sub.call({ op: 'update', id: 'B1', from: 'B3' })).rejects.toThrow(/only the state and notes/);
  });
});

describe('backlog tool: prompt and rendering', () => {
  it('keeps its prompt lean: at most 2 short guidelines (1 in a subagent), no double spaces', () => {
    for (const [t, max] of [[setup(), 2], [setup({ isSubagent: true }), 1]] as const) {
      const guidelines: string[] = t.tool.promptGuidelines;
      expect(guidelines.length).toBeLessThanOrEqual(max);
      for (const line of guidelines) expect(line.length, line).toBeLessThanOrEqual(230);
      expect(guidelines.join(' ')).not.toMatch(/not instructions/);
      expect(t.tool.description).not.toMatch(/ {2}/);
      const parameters: TObject = t.tool.parameters;
      for (const [name, schema] of Object.entries(parameters.properties)) expect('description' in schema && schema.description, name).toBeTruthy();
    }
  });

  it('renders a call line and refreshes the footer counts after a call', async () => {
    const t = setup();
    const ctx = fakeCtx({ cwd: t.cwd });
    await t.call({ op: 'add', title: 'one', goal: 'it works', state: 'todo' }, ctx);
    expect(ctx.ui.statuses.get('octocode-backlog')).toBe('backlog 1 todo · /backlog');
    await t.call({ op: 'update', id: 'B1', state: 'ongoing' }, ctx);
    expect(ctx.ui.statuses.get('octocode-backlog')).toBe('backlog 1 ongoing · /backlog');
    // Items live on the board (`/backlog`), not in a widget under the editor.
    expect(ctx.ui.widgets.has('octocode-backlog')).toBe(false);
    await t.call({ op: 'update', id: 'B1', state: 'done', note: 'ok' }, ctx);
    expect(ctx.ui.statuses.get('octocode-backlog')).toBeUndefined();
    expect(rendered(t.tool.renderCall({ op: 'update', id: 'B1', state: 'done' }, theme, {}))).toBe('○ Backlog(update B1 → done)');
    const row = async (params: Record<string, unknown>) => rendered(t.tool.renderResult(await t.call(params, ctx), { expanded: false }, theme, { args: params, isPartial: false, expanded: false }));
    expect(await row({ op: 'add', title: 'two', goal: 'it works' })).toBe('  ⎿  Added B2 · backlog');
    expect(await row({ op: 'update', id: 'B2', state: 'todo' })).toBe('  ⎿  B2 → todo');
    expect(await row({ op: 'list', states: ['todo', 'done'], limit: 1 })).toMatch(/^ {2}⎿ {2}1 of 2 items\n {5}B\d \[/);
    expect(await row({ op: 'list', states: ['ongoing'] })).toMatch(/^ {2}⎿ {2}No items/);
    expect(await row({ op: 'get', id: 'B2' })).toMatch(/^ {2}⎿ {2}B2 · todo · two\n/);
    expect(await row({ op: 'remove', id: 'B2' })).toBe('  ⎿  Removed B2');
    expect(rendered(t.tool.renderResult({ content: [{ type: 'text', text: 'No backlog item B9' }] }, {}, theme, { args: { op: 'get' }, isError: true }))).toBe('  ⎿  Error: No backlog item B9');
  });
});

describe('backlog status', () => {
  it('names every open state in words and ends with the command that opens the board', () => {
    expect(countsText({ ongoing: 2, todo: 4, backlog: 1, done: 9 })).toBe('backlog 2 ongoing · 4 todo · 1 to triage · /backlog');
    expect(countsText({ ongoing: 0, todo: 0, backlog: 0, done: 3 })).toBe('');
  });

  it('gives RPC clients the same status text and clears it when the database becomes unreadable', async () => {
    const t = setup();
    const rpc = fakeCtx({ cwd: t.cwd, mode: 'rpc' });
    await t.call({ op: 'add', title: 'one', goal: 'it works', state: 'todo' }, rpc);
    await t.call({ op: 'update', id: 'B1', state: 'ongoing' }, rpc);
    expect(rpc.ui.statuses.get('octocode-backlog')).toBe('backlog 1 ongoing · /backlog');
    expect(rpc.ui.widgets.has('octocode-backlog')).toBe(false);
    const db = path.join(t.cwd, 'foreign.sqlite');
    fs.writeFileSync(db, 'not sqlite at all, not even close to a header of one'.repeat(20));
    // afterEach restores OCTOCODE_AGENT_DB.
    closeSharedAgentDb();
    process.env.OCTOCODE_AGENT_DB = db;
    await t.handlers.get('agent_end')![0]({}, rpc);
    expect(rpc.ui.statuses.get('octocode-backlog')).toBeUndefined();
    expect(rpc.ui.widgets.has('octocode-backlog')).toBe(false);
  });
});
