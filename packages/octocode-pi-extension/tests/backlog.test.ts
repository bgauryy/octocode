import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { USER_AUTHOR } from '../src/agentdb/author.js';
import { openAgentDb } from '../src/agentdb/db.js';
import { BacklogError, changeText, parseRef, type Item } from '../src/backlog/item.js';
import { session, storeFor, tmpRoot, useBacklogHome } from './backlog-harness.js';

useBacklogHome();

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

  it('needs a goal for anything an agent adds, and lets a person add a bare title', () => {
    const store = storeFor('goal');
    expect(() => store.add({ title: 'no goal', createdBy: 'agent' })).toThrow(/add needs a goal/);
    expect(() => store.add({ title: 'blank goal', goal: ' \n ', createdBy: 'agent:sub-1' })).toThrow(/add needs a goal/);
    expect(store.list().items).toEqual([]);
    expect(store.add({ title: 'with goal', goal: 'tests pass', createdBy: 'agent' }).goal).toBe('tests pass');
    expect(store.add({ title: 'typed by the user', createdBy: USER_AUTHOR }).goal).toBe('');
  });

  it('defaults to the backlog state and sorts ongoing, then priority, then recent', () => {
    const store = storeFor('r');
    const low = store.add({ title: 'low', state: 'todo', priority: 3, createdBy: 'user' });
    store.add({ title: 'high', state: 'todo', priority: 0, createdBy: 'user' });
    expect(store.add({ title: 'idea', createdBy: 'agent', goal: 'g' }).state).toBe('backlog');
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
    expect(store.notes(done)).toMatchObject([
      { author: 'me', kind: 'change', text: 'state backlog → done' },
      { author: 'me', kind: 'note', text: 'shipped; tests pass' },
    ]);
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

  it('links an item to the one it came from, lists those derived from it, and refuses unknown, self and loop links', () => {
    const store = storeFor('r');
    const root = store.add({ title: 'root', state: 'todo', createdBy: 'user' });
    const split = store.add({ title: 'split', from: 'b1', createdBy: 'agent', goal: 'g' });
    const followUp = store.add({ title: 'follow-up', from: 'B1', state: 'todo', createdBy: 'agent', goal: 'g' });
    const grandchild = store.add({ title: 'grandchild', from: split.ref, createdBy: 'agent', goal: 'g' });
    expect(split.from).toBe(1);
    expect(store.notes(split)).toMatchObject([{ author: 'agent', kind: 'change', text: 'created from B1' }]);
    expect(store.derived(root).map((item) => item.ref)).toEqual([followUp.ref, split.ref]);
    expect(store.list({ from: 'B1' }).items.map((item) => item.ref)).toEqual([followUp.ref, split.ref]);
    expect(store.list({ from: 'B1', states: ['todo'] })).toMatchObject({ total: 1, items: [{ ref: followUp.ref }] });
    expect(() => store.add({ title: 'orphan', from: 'B99', createdBy: 'agent', goal: 'g' })).toThrow(/No backlog item B99 to link from/);
    expect(() => storeFor('other-repo').add({ title: 'x', from: 'B1', createdBy: 'agent', goal: 'g' })).toThrow(/No backlog item B1/);
    expect(() => store.update(root.ref, { from: root.ref }, { id: 'me' })).toThrow(/cannot come from itself/);
    expect(() => store.update(root.ref, { from: grandchild.ref }, { id: 'me' })).toThrow(/B4 already comes from B1: the link would make a loop/);
    expect(store.update(grandchild.ref, { from: root.ref }, { id: 'me' }).from).toBe(1);
    expect(store.update(grandchild.ref, { from: null }, { id: 'me' }).from).toBeUndefined();
    expect(store.notes(grandchild).map((note) => note.text)).toEqual(['created from B2', 'from B2 → B1', 'from B1 → none']);
  });

  it('clears the link on derived items when their origin is removed, and logs it on each', () => {
    const store = storeFor('r');
    const root = store.add({ title: 'root', createdBy: 'user' });
    const child = store.add({ title: 'child', from: root.ref, createdBy: 'agent', goal: 'g' });
    store.remove(root.ref);
    const after = store.get(child.ref)!;
    expect(after.from).toBeUndefined();
    expect(after.version).toBe(child.version + 1);
    expect(store.notes(after).at(-1)).toMatchObject({ author: 'user', kind: 'change', text: 'from B1 → none (B1 "root" was removed)' });
  });

  it('logs every field an update changes, keeps a replaced body in full, and logs nothing for a note or a no-op', () => {
    const store = storeFor('r');
    const body = `why: ${'evidence '.repeat(300)}`.trim();
    const item = store.add({ title: 'old title', body, tags: ['a'], state: 'todo', createdBy: 'user' });
    store.update(item.ref, { title: 'new title', body: 'rewritten', priority: 0, tags: ['b', 'c'], state: 'ongoing' }, { id: 's1' });
    store.update(item.ref, { note: 'why I changed it' }, { id: 's1' });
    store.update(item.ref, { priority: 0, tags: ['b', 'c'], title: 'new title' }, { id: 's1' });
    const notes = store.notes(item, 50);
    expect(notes.map((note) => note.kind)).toEqual(['change', 'note']);
    expect(notes[0]!.text).toBe(`state todo → ongoing · assignee none → s1 · priority p2 → p0 · tags #a → #b #c · title was "old title" · body replaced; it was:\n${body}`);
    expect(body.length).toBeGreaterThan(1000);
    const blank = store.add({ title: 'blank', createdBy: 'user' });
    expect(store.notes(store.update(blank.ref, { body: 'first text' }, { id: 's1' }))[0]!.text).toBe('body added');
  });

  it('matches a query against title and body substrings but only whole tags', () => {
    const store = storeFor('r');
    store.add({ title: 'build cache', tags: ['infra'], createdBy: 'user' });
    store.add({ title: 'menu', tags: ['ui'], createdBy: 'user' });
    expect(store.list({ query: 'ui' }).items.map((item) => item.title)).toEqual(['menu', 'build cache']);
    expect(store.list({ query: '#ui' }).items.map((item) => item.title)).toEqual(['menu']);
    expect(store.list({ query: 'infr' }).items).toEqual([]);
    expect(store.list({ query: 'infra' }).items.map((item) => item.title)).toEqual(['build cache']);
  });

  it('keeps separate databases apart', () => {
    const db = openAgentDb(path.join(tmpRoot(), 'other.db'));
    try {
      storeFor('r', db).add({ title: 'elsewhere', createdBy: 'user' });
      expect(storeFor('r').list().items).toEqual([]);
    } finally {
      db.close();
    }
  });
});

describe('changeText', () => {
  const base: Item = { rowId: 1, seq: 1, ref: 'B1', title: 't', goal: '', body: '', paths: [], why: '', evidence: [], plan: '', state: 'todo', priority: 2, tags: [], createdBy: 'user', createdAt: 0, updatedAt: 0, version: 1 };

  it('names every changed field in a fixed order, short fields first, then the texts in full', () => {
    const before: Item = { ...base, goal: 'g', why: 'w', evidence: ['e1', 'e2'], plan: 'p', body: 'b', from: 3 };
    const after: Item = { ...base, state: 'ongoing', assignee: 's1', priority: 0, tags: ['x'], from: 4, title: 'u', goal: 'h', paths: ['a.ts'], why: 'w2', evidence: ['e3'], plan: 'p2', body: 'b2' };
    expect(changeText(before, after)).toBe(
      'state todo → ongoing · assignee none → s1 · priority p2 → p0 · tags none → #x · from B3 → B4 · title was "t" · goal was "g" · paths none → a.ts · why replaced; it was:\nw · evidence replaced; it was:\ne1\ne2 · plan replaced; it was:\np · body replaced; it was:\nb',
    );
  });

  it('says a field was added when it was empty, and nothing when nothing changed', () => {
    expect(changeText(base, { ...base, goal: 'g', why: 'w', evidence: ['e'], plan: 'p', body: 'b' })).toBe('goal added · why added · evidence added · plan added · body added');
    expect(changeText(base, { ...base })).toBe('');
  });
});
