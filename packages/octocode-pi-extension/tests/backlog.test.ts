import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { closeSharedAgentDb, openAgentDb, sharedAgentDb, type AgentDb } from '../src/agentdb/db.js';
import { boardRows, showBoard } from '../src/backlog/board.js';
import { doItPrompt } from '../src/backlog/command.js';
import { registerBacklog } from '../src/backlog/index.js';
import { countsText } from '../src/backlog/format.js';
import { BacklogError, BacklogStore, parseRef } from '../src/backlog/store.js';
import { Subcommands } from '../src/shared/commands.js';
import { fakeCtx, fakePi, rendered, theme } from './fake-pi.js';

const KEY = { enter: '\r', escape: '\x1b', up: '\x1b[A', down: '\x1b[B', right: '\x1b[C', left: '\x1b[D' };
const SECRET = `ghp_${'a1B2c3D4e5'.repeat(4)}`;

let tmp: string;
let savedDb: string | undefined;

beforeEach(() => {
  tmp = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'octo-backlog-')));
  savedDb = process.env.OCTOCODE_AGENT_DB;
  process.env.OCTOCODE_AGENT_DB = path.join(tmp, 'home', 'octocode.db');
});

afterEach(() => {
  closeSharedAgentDb();
  if (savedDb === undefined) delete process.env.OCTOCODE_AGENT_DB;
  else process.env.OCTOCODE_AGENT_DB = savedDb;
  fs.rmSync(tmp, { recursive: true, force: true });
});

function storeFor(repo: string, db: AgentDb = sharedAgentDb()): BacklogStore {
  return new BacklogStore(() => db, repo);
}

/** Record a session row, as the sessions feature does at session_start. */
function session(id: string, pid: number | null, db: AgentDb = sharedAgentDb()): void {
  db.db.prepare(`INSERT INTO sessions (id, pid) VALUES (?, ?)`).run(id, pid);
}

function repoDir(name = 'repo'): string {
  const dir = path.join(tmp, name);
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

function setup(options: { isSubagent?: boolean; agentId?: string; parentPid?: number } = {}) {
  const fake = fakePi();
  const commands = new Subcommands();
  registerBacklog(fake.pi, { commands, isSubagent: options.isSubagent ?? false, ...(options.agentId ? { agentId: () => options.agentId } : {}), ...(options.parentPid ? { parentPid: options.parentPid } : {}) });
  const cwd = repoDir();
  const tool = fake.tools.get('backlog');
  const call = (params: Record<string, unknown>, ctx = fakeCtx({ cwd })) => tool.execute('id', params, undefined, undefined, ctx) as Promise<{ content: Array<{ text: string }> }>;
  const text = async (params: Record<string, unknown>) => (await call(params)).content[0]!.text;
  return { ...fake, commands, cwd, tool, call, text, command: commands.get('backlog')! };
}

describe('backlog store', () => {
  it('numbers items per repository and never reuses a number', () => {
    const a = storeFor('repo-a');
    const b = storeFor('repo-b');
    expect(a.add({ title: 'one', createdBy: 'user' }).ref).toBe('B1');
    expect(a.add({ title: 'two', createdBy: 'user' }).ref).toBe('B2');
    expect(b.add({ title: 'other', createdBy: 'user' }).ref).toBe('B1');
    a.remove('B2');
    expect(a.add({ title: 'three', createdBy: 'user' }).ref).toBe('B3');
    expect(a.list().items.map((item) => item.title)).toEqual(['three', 'one']);
    expect(parseRef('b7')).toBe(7);
    expect(parseRef('7')).toBe(7);
    expect(parseRef('x')).toBeUndefined();
  });

  it('defaults to the backlog state and sorts ongoing, then priority, then recent', () => {
    const store = storeFor('r');
    const low = store.add({ title: 'low', state: 'todo', priority: 3, createdBy: 'user' });
    store.add({ title: 'high', state: 'todo', priority: 0, createdBy: 'user' });
    expect(store.add({ title: 'idea', createdBy: 'agent' }).state).toBe('backlog');
    store.update(low.ref, { state: 'ongoing' }, { id: 's1' });
    expect(store.list({ states: ['todo', 'ongoing'] }).items.map((item) => item.title)).toEqual(['low', 'high']);
    expect(store.counts()).toEqual({ backlog: 1, todo: 1, ongoing: 1, done: 0 });
    expect(store.list({ query: 'hig' }).items.map((item) => item.title)).toEqual(['high']);
    expect(store.list({ query: '%' }).items).toEqual([]);
  });

  it('claims ongoing items, refuses a live holder, and takes over a finished one', () => {
    const store = storeFor('r');
    const item = store.add({ title: 'task', state: 'todo', createdBy: 'user' });
    session('live', process.ppid);
    session('gone', null);
    expect(store.update(item.ref, { state: 'ongoing' }, { id: 'live' }).assignee).toBe('live');
    expect(() => store.update(item.ref, { state: 'ongoing' }, { id: 'me' })).toThrow(/ongoing for live, which is still running/);
    store.update(item.ref, { state: 'todo' }, { id: 'live' });
    expect(store.get(item.ref)!.assignee).toBeUndefined();
    store.update(item.ref, { state: 'ongoing' }, { id: 'gone' });
    expect(store.update(item.ref, { state: 'ongoing' }, { id: 'me' }).assignee).toBe('me');
  });

  it('refuses changing an item a live session holds, except to add a note, unless the user forces it', () => {
    const store = storeFor('r');
    const item = store.add({ title: 'task', state: 'todo', createdBy: 'user' });
    session('live', process.ppid);
    store.update(item.ref, { state: 'ongoing' }, { id: 'live' });
    for (const input of [{ state: 'done' as const, note: 'x' }, { state: 'todo' as const }, { title: 'renamed' }, { priority: 0 }])
      expect(() => store.update(item.ref, input, { id: 'me' }), JSON.stringify(input)).toThrow(/ongoing for live, which is still running/);
    expect(store.update(item.ref, { note: 'fyi' }, { id: 'me' }).assignee).toBe('live');
    expect(store.update(item.ref, { state: 'todo', force: true }, { id: 'me' }).assignee).toBeUndefined();
  });

  it('stamps done, logs notes, and refuses a stale version', () => {
    const store = storeFor('r');
    const item = store.add({ title: 'task', createdBy: 'user' });
    const done = store.update(item.ref, { state: 'done', note: 'shipped; tests pass' }, { id: 'me' });
    expect(done.doneAt).toBeTypeOf('number');
    expect(done.version).toBe(item.version + 1);
    expect(store.notes(done)).toMatchObject([{ author: 'me', text: 'shipped; tests pass' }]);
    expect(() => store.update(item.ref, { title: 'x', version: item.version }, { id: 'me' })).toThrow(/changed elsewhere/);
    expect(store.update(item.ref, { state: 'todo' }, { id: 'me' }).doneAt).toBeUndefined();
    store.remove(item.ref);
    expect(store.get(item.ref)).toBeUndefined();
  });

  it('sanitizes stored text and normalizes tags', () => {
    const store = storeFor('r');
    const item = store.add({ title: 'bad\u001b]0;pwned\u0007 title', tags: ['#UI', 'ui', 'a b', ''], createdBy: 'user' });
    expect(item.title).toBe('bad title');
    expect(item.tags).toEqual(['ui', 'a-b']);
  });

  it('refuses secrets on the stored (sanitized) text of title, body, note and tags', () => {
    const store = storeFor('r');
    const hidden = `ghp_\u200b${'a1B2c3D4e5'.repeat(4)}`;
    expect(() => store.add({ title: hidden, createdBy: 'user' })).toThrow(BacklogError);
    expect(() => store.add({ title: 'ok', body: `use ${hidden}`, createdBy: 'user' })).toThrow(/body .*secret/);
    expect(() => store.add({ title: 'ok', tags: ['AKIAIOSFODNN7EXAMPLE'], createdBy: 'user' })).toThrow(/tags .*secret/);
    const item = store.add({ title: 'ok', createdBy: 'user' });
    expect(() => store.update(item.ref, { note: hidden }, { id: 'me' })).toThrow(/note .*secret/);
    expect(() => store.update(item.ref, { title: hidden }, { id: 'me' })).toThrow(/secret/);
    expect(store.list().items).toHaveLength(1);
    expect(store.noteCount(item)).toBe(0);
  });

  it('lets a subagent touch only items its owners hold', () => {
    const store = storeFor('r');
    const mine = store.add({ title: 'given', state: 'todo', createdBy: 'user' });
    const other = store.add({ title: 'other', state: 'todo', createdBy: 'user' });
    store.update(mine.ref, { state: 'ongoing' }, { id: 'parent' });
    const sub = { id: 'sub-1', owners: new Set(['parent']) };
    expect(() => store.update(other.ref, { state: 'ongoing' }, sub)).toThrow(BacklogError);
    expect(store.update(mine.ref, { state: 'done', note: 'did it' }, sub).state).toBe('done');
  });

  it('keeps separate databases apart', () => {
    const db = openAgentDb(path.join(tmp, 'other.db'));
    try {
      storeFor('r', db).add({ title: 'elsewhere', createdBy: 'user' });
      expect(storeFor('r').list().items).toEqual([]);
    } finally {
      db.close();
    }
  });
});

describe('backlog tool', () => {
  it('adds proposals to backlog, lists open work with a triage line, and gets details', async () => {
    const t = setup();
    expect(await t.text({ op: 'add', title: 'Add memory tool', body: 'with BM25', priority: 'p1', tags: ['mem'] })).toBe('Added B1 [backlog p1] Add memory tool #mem');
    await t.text({ op: 'add', title: 'Accepted', state: 'todo' });
    expect(await t.text({ op: 'list' })).toBe('B2 [todo p2] Accepted\n1 item(s) in backlog await triage (list with states: ["backlog"]).');
    expect(await t.text({ op: 'update', id: 'B1', state: 'ongoing', note: 'starting' })).toBe('Updated B1 [ongoing p1] Add memory tool #mem (you)');
    const detail = await t.text({ op: 'get', id: 'b1' });
    expect(detail).toContain('B1 [ongoing p1] Add memory tool #mem (you)');
    expect(detail).toContain('with BM25');
    expect(detail).toMatch(/Notes:\n- .* ago you: starting/);
    expect(detail).not.toMatch(/version/);
    expect(await t.text({ op: 'list', states: ['todo'], limit: 1, query: 'Acc' })).toContain('B2 [todo p2] Accepted');
  });

  it('requires a note to finish and refuses secrets', async () => {
    const t = setup();
    await t.text({ op: 'add', title: 'task', state: 'todo' });
    await expect(t.call({ op: 'update', id: 'B1', state: 'done' })).rejects.toThrow(/what changed and how it was verified/);
    await expect(t.call({ op: 'add', title: 'token', body: `use ${SECRET}` })).rejects.toThrow(/secret/);
    await expect(t.call({ op: 'update', id: 'B1', note: SECRET })).rejects.toThrow(/secret/);
    await expect(t.call({ op: 'update', id: 'B1', tags: [SECRET] })).rejects.toThrow(/secret/);
    // A secret note on add is refused before anything is stored.
    await expect(t.call({ op: 'add', title: 'fast', state: 'done', note: SECRET })).rejects.toThrow(/secret/);
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
    await expect(t.call({ op: 'add', title: 'shipped', state: 'done' })).rejects.toThrow(/note/);
    await expect(t.call({ op: 'add', title: 'idea', note: 'context' })).rejects.toThrow(/note only with state ongoing or done/);
    expect(storeFor(fs.realpathSync(t.cwd)).counts()).toMatchObject({ backlog: 0, todo: 0, done: 0 });
    expect(await t.text({ op: 'add', title: 'shipped', state: 'done', note: 'merged; tests pass' })).toMatch(/^Added B1 \[done/);
  });

  it('removes an item and its notes, but not one another live session holds', async () => {
    const t = setup();
    await t.text({ op: 'add', title: 'dup', state: 'todo' });
    await t.text({ op: 'update', id: 'B1', note: 'a note' });
    expect(await t.text({ op: 'remove', id: 'b1' })).toBe('Removed B1 "dup" and its notes.');
    await expect(t.call({ op: 'get', id: 'B1' })).rejects.toThrow(/No backlog item B1/);
    await expect(t.call({ op: 'remove', id: 'B1' })).rejects.toThrow(/No backlog item/);
    await expect(t.call({ op: 'remove' })).rejects.toThrow(/needs an id/);
    await t.text({ op: 'add', title: 'held', state: 'todo' });
    session('other-session', process.ppid);
    storeFor(fs.realpathSync(t.cwd)).update('B2', { state: 'ongoing' }, { id: 'other-session' });
    await expect(t.call({ op: 'remove', id: 'B2' })).rejects.toThrow(/ongoing for other-session/);
  });

  it('refuses to claim an item another live session holds, naming it', async () => {
    const t = setup();
    await t.text({ op: 'add', title: 'task', state: 'todo' });
    session('other-session', process.ppid);
    storeFor(fs.realpathSync(t.cwd)).update('B1', { state: 'ongoing' }, { id: 'other-session' });
    await expect(t.call({ op: 'update', id: 'B1', state: 'ongoing' })).rejects.toThrow(/other-session/);
  });

  it('in a subagent: adds only to backlog and updates only its or its parent\'s items', async () => {
    const parent = setup();
    await parent.text({ op: 'add', title: 'delegated', state: 'todo' });
    await parent.text({ op: 'add', title: 'unrelated', state: 'todo' });
    session('parent-session', 4242);
    storeFor(fs.realpathSync(parent.cwd)).update('B1', { state: 'ongoing' }, { id: 'parent-session' });
    const sub = setup({ isSubagent: true, agentId: 'general-1', parentPid: 4242 });
    expect(sub.tool.parameters.properties.op.enum).toEqual(['list', 'get', 'add', 'update']);
    expect(sub.tool.parameters.properties.state.enum).toEqual(['ongoing', 'done']);
    expect(sub.tool.description).toContain('As a subagent');
    expect(sub.tool.description).toContain('your parent has claimed');
    expect(sub.tool.promptGuidelines.join(' ')).not.toContain('set it ongoing');
    expect(await sub.text({ op: 'list' })).toBe('B1 [ongoing p2] delegated (parent)\nB2 [todo p2] unrelated');
    expect(await sub.text({ op: 'add', title: 'follow-up', state: 'todo' })).toBe('Added B3 [backlog p2] follow-up');
    expect(storeFor(fs.realpathSync(sub.cwd)).get('B3')!.createdBy).toBe('agent:general-1');
    // A proposal takes no note, and the parent has not claimed it, so the hint points at body, not a later update.
    await expect(sub.call({ op: 'add', title: 'with note', note: 'details' })).rejects.toThrow('A subagent adds without a note: put the details in body.');
    await expect(sub.call({ op: 'update', id: 'B2', state: 'ongoing' })).rejects.toThrow(/not claimed by your parent/);
    await expect(sub.call({ op: 'update', id: 'B1', title: 'renamed' })).rejects.toThrow(/only the state and notes/);
    await expect(sub.call({ op: 'update', id: 'B1', state: 'todo' })).rejects.toThrow(/sets only ongoing or done/);
    expect(storeFor(fs.realpathSync(sub.cwd)).get('B1')!.state).toBe('ongoing');
    expect(await sub.text({ op: 'update', id: 'B1', state: 'done', note: 'done; verified' })).toBe('Updated B1 [done p2] delegated');
    await expect(sub.call({ op: 'remove', id: 'B3' })).rejects.toThrow(/subagent cannot remove/);
  });

  it('keeps its prompt lean: at most 2 short guidelines (1 in a subagent), no double spaces', () => {
    for (const [t, max] of [[setup(), 2], [setup({ isSubagent: true }), 1]] as const) {
      const guidelines: string[] = t.tool.promptGuidelines;
      expect(guidelines.length).toBeLessThanOrEqual(max);
      for (const line of guidelines) expect(line.length, line).toBeLessThanOrEqual(230);
      expect(guidelines.join(' ')).not.toMatch(/not instructions/);
      expect(t.tool.description).not.toMatch(/ {2}/);
      for (const [name, schema] of Object.entries(t.tool.parameters.properties as Record<string, { description?: string }>)) expect(schema.description, name).toBeTruthy();
    }
  });

  it('renders a call line and refreshes the footer counts after a call', async () => {
    const t = setup();
    const ctx = fakeCtx({ cwd: t.cwd });
    await t.call({ op: 'add', title: 'one', state: 'todo' }, ctx);
    expect(ctx.ui.statuses.get('octocode-backlog')).toBe('backlog 1 todo · /backlog');
    await t.call({ op: 'update', id: 'B1', state: 'ongoing' }, ctx);
    expect(ctx.ui.statuses.get('octocode-backlog')).toBe('backlog 1 ongoing · /backlog');
    // Items live on the board (`/backlog`), not in a widget under the editor.
    expect(ctx.ui.widgets.has('octocode-backlog')).toBe(false);
    await t.call({ op: 'update', id: 'B1', state: 'done', note: 'ok' }, ctx);
    expect(ctx.ui.statuses.get('octocode-backlog')).toBeUndefined();
    expect(rendered(t.tool.renderCall({ op: 'update', id: 'B1', state: 'done' }, theme, {}))).toBe('○ Backlog(update B1 → done)');
    const row = async (params: Record<string, unknown>) => rendered(t.tool.renderResult(await t.call(params, ctx), { expanded: false }, theme, { args: params, isPartial: false, expanded: false }));
    expect(await row({ op: 'add', title: 'two' })).toBe('  ⎿  Added B2 · backlog');
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
    await t.call({ op: 'add', title: 'one', state: 'todo' }, rpc);
    await t.call({ op: 'update', id: 'B1', state: 'ongoing' }, rpc);
    expect(rpc.ui.statuses.get('octocode-backlog')).toBe('backlog 1 ongoing · /backlog');
    expect(rpc.ui.widgets.has('octocode-backlog')).toBe(false);
    const db = path.join(t.cwd, 'foreign.sqlite');
    fs.writeFileSync(db, 'not sqlite at all, not even close to a header of one'.repeat(20));
    // afterEach restores OCTOCODE_AGENT_DB.
    closeSharedAgentDb();
    process.env.OCTOCODE_AGENT_DB = db;
    await t.handlers.get('agent_end')![0]!({}, rpc);
    expect(rpc.ui.statuses.get('octocode-backlog')).toBeUndefined();
    expect(rpc.ui.widgets.has('octocode-backlog')).toBe(false);
  });
});

describe('/octocode backlog', () => {
  it('works headless: add, list, move, detail, export', async () => {
    const t = setup();
    const ctx = fakeCtx({ cwd: t.cwd, hasUI: false });
    const stderr = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    await t.command.handler('', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/empty/);
    // Print mode has no notify surface: the text goes to stderr too.
    expect(String(stderr.mock.calls.at(-1)?.[0])).toMatch(/empty/);
    stderr.mockRestore();
    await t.command.handler('add Write the docs', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toBe('Added B1 [todo p2] Write the docs');
    await t.command.handler('B1 ongoing', ctx);
    await t.command.handler('', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toBe('Ongoing (1)\n  B1 [ongoing p2] Write the docs (you)');
    await t.command.handler('B1', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/assignee you · version \d/);
    await t.command.handler('B1 done', ctx);
    await t.command.handler('B9', ctx);
    expect(ctx.ui.notes.at(-1)).toMatchObject({ type: 'warning' });
    await t.command.handler('export', ctx);
    const file = path.join(t.cwd, '.octocode', 'backlog.md');
    expect(ctx.ui.notes.at(-1)!.message).toContain(file);
    const markdown = fs.readFileSync(file, 'utf8');
    expect(markdown).toMatch(/^<!-- Generated by Octocode .* not read back\. -->/);
    expect(markdown).toContain('## Done (1)\n\n- **B1** [p2] Write the docs');
    expect(t.commands.complete('backlog e')).toEqual([expect.objectContaining({ value: 'backlog export' })]);
    expect(t.commands.complete('backlog B1 d')!.map((item) => item.value)).toEqual(['backlog B1 done', 'backlog B1 delete']);
  });

  it('"do" marks the item ongoing and sends it to the agent, as a follow-up while busy', async () => {
    const t = setup();
    storeFor(fs.realpathSync(t.cwd)).add({ title: 'Fix login', body: 'The form hangs.', state: 'todo', createdBy: 'user' });
    await t.command.handler('do B1', fakeCtx({ cwd: t.cwd, hasUI: false }));
    expect(t.sent.at(-1)).toEqual({ message: doItPrompt(storeFor(fs.realpathSync(t.cwd)).get('B1')!), options: undefined });
    expect(t.sent.at(-1)!.message).toBe('Work on backlog item B1: Fix login\n\nThe form hangs.\n\nWhen finished, mark it done with the backlog tool (note what changed and how it was verified).');
    expect(storeFor(fs.realpathSync(t.cwd)).get('B1')).toMatchObject({ state: 'ongoing', assignee: 'session-test' });
    await t.command.handler('do B1', fakeCtx({ cwd: t.cwd, hasUI: false, idle: false }));
    expect(t.sent.at(-1)!.options).toEqual({ deliverAs: 'followUp' });
  });

  it('falls back to select lists without a custom component: actions, move, priority, delegate, delete', async () => {
    const t = setup();
    const store = storeFor(fs.realpathSync(t.cwd));
    store.add({ title: 'first', state: 'todo', createdBy: 'user' });
    const rpc = (selects: Array<string | undefined>, confirms: boolean[] = []) => fakeCtx({ cwd: t.cwd, mode: 'rpc', ui: { selects, confirms } });
    await t.command.handler('', rpc(['B1 [todo p2] first', 'Move to…', 'backlog', undefined]));
    expect(store.get('B1')!.state).toBe('backlog');
    await t.command.handler('', rpc(['B1 [backlog p2] first', 'Priority…', 'p0', undefined]));
    expect(store.get('B1')!.priority).toBe(0);
    await t.command.handler('', rpc(['+ Add an item', undefined]));
    await t.command.handler('B1', rpc(['Delegate to a subagent']));
    expect(t.sent.at(-1)!.message).toMatch(/^Delegate backlog item B1 to a subagent with the agent tool: first/);
    expect(store.get('B1')!.state).toBe('ongoing');
    const edits = fakeCtx({ cwd: t.cwd, mode: 'rpc', ui: { selects: ['Edit'] } });
    Object.assign(edits.ui, { editor: async () => 'renamed\n\nnew body' });
    await t.command.handler('B1', edits);
    expect(store.get('B1')).toMatchObject({ title: 'renamed', body: 'new body' });
    await t.command.handler('B1', rpc(['Delete'], [true]));
    expect(store.get('B1')).toBeUndefined();
  });

  it('shows the board, moves items with arrows, and "d" hands the selected item over', async () => {
    const t = setup();
    const store = storeFor(fs.realpathSync(t.cwd));
    store.add({ title: 'alpha', state: 'todo', createdBy: 'user' });
    store.add({ title: 'beta', state: 'backlog', createdBy: 'user' });
    const frames: string[] = [];
    const tui = { requestRender: () => undefined, terminal: { rows: 40, columns: 100 } };
    const ctx = fakeCtx({
      cwd: t.cwd,
      ui: {
        custom: (factory) =>
          new Promise((resolve) => {
            const board = factory(tui, theme, {}, resolve);
            frames.push(rendered(board, 100));
            for (const key of [KEY.down, KEY.right]) {
              board.handleInput(key);
              frames.push(rendered(board, 100));
            }
            board.handleInput('d');
          }),
      },
    });
    await t.command.handler('', ctx);
    // Sections are drawn in flow order, so → (toward done) moves an item down the board.
    const at = (heading: string) => frames[0]!.indexOf(heading);
    expect(at('Backlog (triage) (1)')).toBeLessThan(at('Todo (1)'));
    expect(at('Todo (1)')).toBeLessThan(at('Ongoing (0)'));
    expect(at('Ongoing (0)')).toBeLessThan(at('Done (0)'));
    expect(frames[0]).toMatch(/› B2 p2 beta/);
    expect(frames[1]).toMatch(/› B1 p2 alpha/);
    // → moves alpha from todo to ongoing; ← would move it back.
    expect(frames[2]).toContain('Ongoing (1)');
    expect(frames[2]).toMatch(/› B1 p2 alpha/);
    expect(frames[2]).toContain('←→ move up/down a state');
    expect(store.get('B1')).toMatchObject({ state: 'ongoing', assignee: 'session-test' });
    expect(t.sent.at(-1)!.message).toMatch(/^Work on backlog item B1: alpha/);
  });

  it('orders board rows by state and keeps only the latest done items', async () => {
    const store = storeFor('r');
    for (let index = 0; index < 7; index++) store.add({ title: `d${index}`, state: 'done', createdBy: 'user' });
    store.add({ title: 'now', state: 'ongoing', createdBy: 'user' });
    const rows = boardRows(store.list().items);
    expect(rows[0]!.title).toBe('now');
    expect(rows).toHaveLength(6);
    const ctx = fakeCtx({ cwd: tmp, ui: { custom: (factory) => new Promise((resolve) => factory({ requestRender: () => undefined, terminal: { rows: 40 } }, theme, {}, resolve).handleInput(KEY.escape)) } });
    expect(await showBoard(ctx, { title: 'x', self: 's', load: () => store.list().items, move: () => undefined })).toBeUndefined();
  });
});
