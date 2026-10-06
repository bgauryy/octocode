import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { closeSharedAgentDb, openAgentDb, type AgentDb } from '../src/agentdb/db.js';
import { memoryTemplate, parseTemplate } from '../src/memory/command.js';
import { injectedIds, lastInjection, MEMORY_HEADER, MEMORY_MESSAGE_TYPE, PINNED_BUDGET, selectInjection, TOTAL_BUDGET } from '../src/memory/inject.js';
import { ftsQuery, jaccard, normalizeTitle, parseMemoryId, terms } from '../src/memory/query.js';
import { memoryStore, registerMemory } from '../src/memory/register.js';
import { MemoryError, MemoryStore, memoryLine, type MemoryInput } from '../src/memory/store.js';
import { Subcommands } from '../src/shared/commands.js';
import { fakeCtx, fakePi, rendered, theme } from './fake-pi.js';

let root: string;
let cwd: string;
const opened: AgentDb[] = [];

beforeEach(() => {
  root = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-memory-'));
  cwd = path.join(root, 'repo');
  fs.mkdirSync(cwd);
  vi.stubEnv('OCTOCODE_HOME', path.join(root, 'home'));
  vi.stubEnv('OCTOCODE_AGENT_DB', '');
  vi.stubEnv('OCTOCODE_MEMORY_AUTO', '');
});

afterEach(() => {
  for (const db of opened.splice(0)) db.close();
  closeSharedAgentDb();
  vi.unstubAllEnvs();
  fs.rmSync(root, { recursive: true, force: true });
});

function freshDb(options: { fts?: boolean } = {}): AgentDb {
  const db = openAgentDb(path.join(root, `db-${opened.length}.db`));
  opened.push(db);
  return options.fts === false ? { ...db, fts: false, transaction: db.transaction.bind(db) } : db;
}

const add = (store: MemoryStore, input: MemoryInput, now?: number) => store.set({ author: 'agent', ...input }, now).memory;

/** A ctx with the bits the memory feature reads beyond fakeCtx: trust, the branch and the editor. */
function ctxFor(options: { branch?: unknown[]; trusted?: boolean; hasUI?: boolean; selects?: Array<string | undefined>; confirms?: boolean[]; editor?: Array<string | undefined> } = {}) {
  const ctx = fakeCtx({ cwd, hasUI: options.hasUI ?? true, ui: { selects: options.selects ?? [], confirms: options.confirms ?? [] } });
  const branch = options.branch ?? [];
  const edits = options.editor ?? [];
  const editorCalls: Array<{ title: string; prefill?: string }> = [];
  Object.assign(ctx, { isProjectTrusted: () => options.trusted ?? true });
  Object.assign(ctx.sessionManager, { getBranch: () => branch });
  Object.assign(ctx.ui, {
    editor: async (title: string, prefill?: string) => {
      editorCalls.push({ title, prefill });
      return edits.shift();
    },
  });
  return { ctx, branch, editorCalls };
}

/** Distinct words, so generated fixtures are not near-duplicates of each other. */
const WORDS = 'alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike november oscar papa quebec romeo sierra tango uniform victor whiskey xray yankee zulu amber basil cedar dahlia ember fennel garnet hazel iris jasper kelp lotus maple nutmeg olive pepper quartz rowan saffron thyme'.split(' ');

describe('memory query builder', () => {
  it('quotes every term so FTS5 syntax is plain text', () => {
    expect(ftsQuery('the "sqlite" busy_timeout* col:title NEAR(a b) ^start AND OR NOT -x +y')).toBe('"sqlite" OR "busy" OR "timeout" OR "col" OR "title" OR "near" OR "start"');
    expect(ftsQuery('  "* : ^ ( )  ')).toBeUndefined();
    expect(ftsQuery('a an the of')).toBeUndefined();
  });

  it('keeps unicode words, drops stopwords and repeats, caps the term count', () => {
    expect(terms('Café café שלום naïve I x')).toEqual(['café', 'שלום', 'naïve']);
    expect(terms('ＳＱＬｉｔｅ busy')).toEqual(['sqlite', 'busy']);
    expect(ftsQuery(Array.from({ length: 50 }, (_, i) => `w${i}`).join(' '))!.split(' OR ')).toHaveLength(32);
  });

  it('normalizes titles, measures overlap, parses ids', () => {
    expect(normalizeTitle('  Use node:SQLITE!  ')).toBe(normalizeTitle('use node sqlite'));
    expect(jaccard(['a', 'b'], ['a', 'b'])).toBe(1);
    expect(jaccard([], [])).toBe(0);
    expect(jaccard(['a', 'b', 'c'], ['a'])).toBeCloseTo(1 / 3);
    expect([parseMemoryId('M7'), parseMemoryId('m12'), parseMemoryId(' 3 '), parseMemoryId('M0'), parseMemoryId('B7'), parseMemoryId('')]).toEqual([7, 12, 3, undefined, undefined, undefined]);
  });
});

describe('MemoryStore', () => {
  it('runs FTS syntax characters through search without an error', () => {
    const store = new MemoryStore(freshDb(), 'repo-a');
    add(store, { title: 'Quote handling', body: 'Strings with "quotes" and stars' });
    for (const query of ['"', '"quotes', 'quotes"*', 'title:quote', 'NEAR(quote star)', '(((', 'star*', '^quote', 'AND OR NOT']) expect(() => store.search(query)).not.toThrow();
    expect(store.search('quotes"*')[0]?.title).toBe('Quote handling');
  });

  it('ranks by BM25 with title above keywords above body', () => {
    const store = new MemoryStore(freshDb(), 'repo-a');
    const body = add(store, { title: 'Alpha', body: 'mentions widget once among many other ordinary words here' });
    const keywords = add(store, { title: 'Beta', keywords: 'widget', body: 'unrelated text' });
    const title = add(store, { title: 'Widget factory', body: 'nothing else' });
    expect(store.search('widget').map((hit) => hit.id)).toEqual([title.id, keywords.id, body.id]);
    expect(store.search('widget').every((hit) => hit.score > 0)).toBe(true);
  });

  it('sees global memories and only this repository\'s project memories', () => {
    const db = freshDb();
    const a = new MemoryStore(db, 'repo-a');
    const b = new MemoryStore(db, 'repo-b');
    const global = add(a, { title: 'Prefers tabs', scope: 'global' });
    const own = add(a, { title: 'Repo A uses tabs' });
    const other = add(b, { title: 'Repo B uses tabs' });
    expect(a.search('tabs').map((hit) => hit.id).sort()).toEqual([global.id, own.id].sort());
    expect(a.search('tabs', 'global').map((hit) => hit.id)).toEqual([global.id]);
    expect(a.list('project').map((memory) => memory.id)).toEqual([own.id]);
    expect(a.get(other.id)).toBeUndefined();
    expect(() => a.delete(other.id)).toThrow(MemoryError);
    expect(() => a.set({ id: other.id, title: 'x', author: 'agent' })).toThrow(/No memory/);
    expect(b.get(other.id)?.repoKey).toBe('repo-b');
    expect(a.count()).toBe(2);
  });

  it('boosts a project hit over an equal global one', () => {
    const store = new MemoryStore(freshDb(), 'repo-a');
    const global = add(store, { title: 'Lint rule gamma', scope: 'global' });
    const project = add(store, { title: 'Lint rule delta' });
    const hits = store.search('lint rule');
    expect(hits.map((hit) => hit.id)).toEqual([project.id, global.id]);
  });

  it('refuses near-duplicates in the same scope and points to the existing id', () => {
    const store = new MemoryStore(freshDb(), 'repo-a');
    const first = add(store, { title: 'Use node:sqlite, not better-sqlite3', body: 'No native dependencies allowed in the extension.' });
    expect(() => add(store, { title: 'use NODE sqlite not better sqlite3!', body: 'different' })).toThrow(`Similar memory M${first.id} exists`);
    expect(() => add(store, { title: 'Native dependencies', body: 'Use node sqlite not better sqlite3; no native dependencies allowed in the extension' })).toThrow(`id:"M${first.id}"`);
    // Another scope, or clearly new text, is fine.
    expect(add(store, { title: 'Use node:sqlite, not better-sqlite3', scope: 'global' }).scope).toBe('global');
    expect(add(store, { title: 'Vitest runs the tests', body: 'yarn test' }).id).toBeGreaterThan(first.id);
  });

  it('updates by id, moves scope, pins, and sanitizes and caps text', () => {
    const store = new MemoryStore(freshDb(), 'repo-a');
    const memory = add(store, { title: 'Title \u001b[31mred\u001b[0m', body: 'x'.repeat(2000), kind: 'gotcha' }, 1000);
    expect(memory.title).toBe('Title red');
    expect(memory.body).toHaveLength(1500);
    const updated = store.set({ id: memory.id, scope: 'global', pinned: true, keywords: 'k1 k2', author: 'user' }, 2000).memory;
    expect(updated).toMatchObject({ scope: 'global', repoKey: null, pinned: true, keywords: 'k1 k2', kind: 'gotcha', title: 'Title red', updatedAt: 2000, createdAt: 1000 });
    expect(store.set({ id: memory.id, scope: 'project', author: 'user' }).memory.repoKey).toBe('repo-a');
    expect(store.search('k2')[0]?.id).toBe(memory.id);
    expect(() => store.set({ id: memory.id, title: '  ', author: 'user' })).toThrow(/needs a title/);
    expect(() => add(store, { title: '' })).toThrow(/needs a title/);
  });

  it('refuses secrets in any field', () => {
    const store = new MemoryStore(freshDb(), 'repo-a');
    expect(() => add(store, { title: 'GitHub token', body: `ghp_${'a'.repeat(36)}` })).toThrow(/looks like a secret/);
    expect(() => add(store, { title: 'AWS', keywords: 'AKIAIOSFODNN7EXAMPLE' })).toThrow(/looks like a secret/);
    // Checked on the stored (sanitized) text, so an invisible character cannot hide a token.
    expect(() => add(store, { title: 'Hidden', body: `ghp_\u200b${'a'.repeat(36)}` })).toThrow(/looks like a secret/);
    expect(() => add(store, { title: 'DB', body: 'password: hunter2secret99' })).toThrow(/looks like a secret/);
    const memory = add(store, { title: 'Ordinary', body: 'the token budget is 2000; password field validation' });
    expect(() => store.set({ id: memory.id, keywords: `AKIA\u200bIOSFODNN7EXAMPLE`, author: 'agent' })).toThrow(/looks like a secret/);
    expect(store.count()).toBe(1);
  });

  it('writes a line without the body when bodyChars is 0', () => {
    const store = new MemoryStore(freshDb(), 'repo-a');
    const memory = add(store, { title: 'Line title', body: 'some body' });
    expect(memoryLine(memory, 0)).toBe(`M${memory.id} (project, fact) Line title`);
    expect(memoryLine(memory, 4)).toBe(`M${memory.id} (project, fact) Line title — som…`);
  });

  it('deletes, and the index forgets the deleted memory', () => {
    const store = new MemoryStore(freshDb(), 'repo-a');
    const memory = add(store, { title: 'Ephemeral zebra' });
    expect(store.delete(memory.id).title).toBe('Ephemeral zebra');
    expect(store.search('zebra')).toEqual([]);
  });

  it('falls back to LIKE-style scoring without FTS5, same weights and scope', () => {
    const db = freshDb({ fts: false });
    const store = new MemoryStore(db, 'repo-a');
    const body = add(store, { title: 'Alpha', body: 'mentions widget once' });
    const keywords = add(store, { title: 'Beta', keywords: 'widget' });
    const title = add(store, { title: 'Widget factory' });
    add(new MemoryStore(db, 'repo-b'), { title: 'Widget elsewhere' });
    expect(store.search('Widget "*').map((hit) => hit.id)).toEqual([title.id, keywords.id, body.id]);
    expect(store.search('nothing-matches')).toEqual([]);
    expect(() => add(store, { title: 'widget FACTORY' })).toThrow(/Similar memory/);
  });

  it('marks usage and keeps the auto setting in meta', () => {
    const store = new MemoryStore(freshDb(), 'repo-a');
    const memory = add(store, { title: 'Used one' });
    store.markUsed([memory.id], 5000);
    store.markUsed([]);
    expect(store.get(memory.id)).toMatchObject({ useCount: 1, lastUsed: 5000 });
    expect(store.autoSetting()).toBe(true);
    store.setAutoSetting(false);
    expect(store.autoSetting()).toBe(false);
    store.setAutoSetting(true);
    expect(store.autoSetting()).toBe(true);
  });
});

describe('memory injection selection', () => {
  it('injects pinned memories plus relevant hits above the relative threshold', () => {
    const store = new MemoryStore(freshDb(), 'repo-a');
    const pinned = add(store, { title: 'Always answer tersely', pinned: true, scope: 'global', kind: 'preference' });
    const strong = add(store, { title: 'Checkpoint rewind flow', keywords: 'rewind undo checkpoint', body: 'Rewind restores files from checkpoints' });
    add(store, { title: 'Unrelated banana', body: 'fruit' });
    const injection = selectInjection(store, { query: 'how does rewind of a checkpoint work', scope: 'all', seen: new Set(), topK: 5 })!;
    expect(injection.ids).toEqual([pinned.id, strong.id]);
    expect(injection.content.split('\n')[0]).toBe(MEMORY_HEADER);
    expect(injection.content).toContain(`M${strong.id} (project, fact) Checkpoint rewind flow — Rewind restores`);
  });

  it('injects a hit only with two matched words or a title/keyword match, ignoring short words', () => {
    const store = new MemoryStore(freshDb(), 'repo-a');
    const wal = add(store, { title: 'WAL mode', body: 'write ahead logging keeps readers going while one writer commits' });
    add(store, { title: 'TS layering', body: 'src folders depend downward only' });
    const inject = (query: string) => selectInjection(store, { query, scope: 'all', seen: new Set(), topK: 5 })?.ids;
    expect(inject('write a haiku about autumn')).toBeUndefined();
    expect(inject('rename variable foo in utils.ts')).toBeUndefined();
    expect(inject('why do readers keep going during writes')).toEqual([wal.id]);
    expect(inject('enable wal')).toEqual([wal.id]);
  });

  it('skips ids already injected on the branch and returns nothing new', () => {
    const store = new MemoryStore(freshDb(), 'repo-a');
    const memory = add(store, { title: 'Rewind notes' });
    expect(selectInjection(store, { query: 'rewind', scope: 'all', seen: new Set([memory.id]), topK: 5 })).toBeUndefined();
    expect(selectInjection(store, { query: '   ', scope: 'all', seen: new Set(), topK: 5 })).toBeUndefined();
  });

  it('keeps pinned and total text within their budgets and honors topK', () => {
    const store = new MemoryStore(freshDb(), 'repo-a');
    for (let i = 0; i < 6; i++) add(store, { title: `Pinned rule ${WORDS[i]}`, body: `${WORDS[i + 10]} `.repeat(60), pinned: true, scope: 'global' }, 1000 + i);
    for (let i = 0; i < 10; i++) add(store, { title: `Parser ${WORDS[i + 20]}`, body: `parser ${`${WORDS[i + 30]} `.repeat(40)}` }, 2000 + i);
    const injection = selectInjection(store, { query: 'parser', scope: 'all', seen: new Set(), topK: 3 })!;
    const lines = injection.content.split('\n').slice(1);
    const pinnedLines = lines.filter((line) => line.includes('pinned'));
    expect(pinnedLines.join('\n').length).toBeLessThanOrEqual(PINNED_BUDGET);
    expect(lines.join('\n').length).toBeLessThanOrEqual(TOTAL_BUDGET);
    expect(lines.length - pinnedLines.length).toBeLessThanOrEqual(3);
    expect(lines.length - pinnedLines.length).toBeGreaterThan(0);
    expect(lines.every((line) => line.length <= 600)).toBe(true);
  });

  it('reads injected ids and the last injection from the branch, reset by a compaction', () => {
    const message = (ids: unknown[], content = 'x') => ({ type: 'custom_message', customType: MEMORY_MESSAGE_TYPE, content, details: { ids } });
    const entries = [message([1, 2], 'first'), { type: 'custom_message', customType: 'other', details: { ids: [9] } }, message([3, 'bad'], 'second')];
    expect([...injectedIds(entries)]).toEqual([1, 2, 3]);
    expect([...injectedIds([...entries, { type: 'compaction' }, message([4])])]).toEqual([4]);
    expect(lastInjection(entries)).toBe('second');
    expect(lastInjection([null, 5])).toBeUndefined();
  });
});

describe('memory tool', () => {
  const setup = (isSubagent = false) => {
    const fake = fakePi();
    const commands = new Subcommands();
    registerMemory(fake.pi, { commands, isSubagent });
    return { ...fake, subcommands: commands, tool: fake.tools.get('memory') };
  };
  const run = (tool: any, params: Record<string, unknown>, ctx = ctxFor().ctx) => tool.execute('call', params, undefined, undefined, ctx);
  const text = (result: any) => result.content[0].text as string;

  it('sets, searches, lists, gets and deletes', async () => {
    const { tool } = setup();
    expect(tool.executionMode).toBeUndefined();
    const saved = await run(tool, { op: 'set', title: 'Build with yarn', body: 'Run yarn build from the root', keywords: 'compile bundle', kind: 'procedure' });
    expect(text(saved)).toMatch(/^Saved M1 \(project, procedure\) Build with yarn/);
    expect(text(await run(tool, { op: 'search', query: 'compile' }))).toContain('M1 (project, procedure) Build with yarn');
    expect(text(await run(tool, { op: 'search', query: 'nothing here' }))).toBe('No memory matches.');
    expect(text(await run(tool, { op: 'get', id: 'M1' }))).toContain('keywords: compile bundle');
    expect(text(await run(tool, { op: 'list' }))).toContain('M1');
    expect(text(await run(tool, { op: 'set', id: 'M1', pinned: true }, ctxFor({ confirms: [true] }).ctx))).toMatch(/^Updated M1 \(project, procedure, pinned\)/);
    await run(tool, { op: 'set', title: 'Second memory', scope: 'global' }, ctxFor({ hasUI: false }).ctx);
    expect(text(await run(tool, { op: 'list', limit: 1 }))).toContain('1 more; search, or raise limit');
    expect(text(await run(tool, { op: 'delete', id: 'm1' }))).toMatch(/^Deleted M1/);
    await expect(run(tool, { op: 'get', id: 'M1' })).rejects.toThrow(/No memory M1/);
    await expect(run(tool, { op: 'get', id: 'B1' })).rejects.toThrow(/not a memory id/);
    await expect(run(tool, { op: 'get' })).rejects.toThrow(/needs id/);
    await expect(run(tool, { op: 'search' })).rejects.toThrow(/needs query/);
    await expect(run(tool, { op: 'set', title: 'x', scope: 'all' })).rejects.toThrow(/project or global/);
    await expect(run(tool, { op: 'set', title: 'Key', body: `sk-${'a'.repeat(40)}` })).rejects.toThrow(/secret/);
    await expect(run(tool, { op: 'set', title: 'second MEMORY', scope: 'global' }, ctxFor({ hasUI: false }).ctx)).rejects.toThrow(/Similar memory M2/);
  });

  it('asks before saving a global or pinned memory, and never in an untrusted project', async () => {
    const { tool } = setup();
    // The user sees the body that will be injected, and a save the store would refuse is never offered.
    const asked: string[] = [];
    const seen = ctxFor({ confirms: [false] }).ctx;
    Object.assign(seen.ui, { confirm: async (title: string, message: string) => (asked.push(`${title}\n${message}`), false) });
    await expect(run(tool, { op: 'set', title: 'Shown', body: 'Line one\nLine two', pinned: true }, seen)).rejects.toThrow(/declined/);
    expect(asked[0]).toContain('Save pinned memory?\nShown\n\nLine one\nLine two');
    await expect(run(tool, { op: 'set', title: 'Key', body: `sk-${'a'.repeat(40)}`, pinned: true }, seen)).rejects.toThrow(/secret/);
    await expect(run(tool, { op: 'set', body: 'no title', pinned: true }, seen)).rejects.toThrow(/needs a title/);
    expect(asked).toHaveLength(1);
    await expect(run(tool, { op: 'set', title: 'Global one', scope: 'global' }, ctxFor({ confirms: [false] }).ctx)).rejects.toThrow(/declined/);
    expect(text(await run(tool, { op: 'set', title: 'Global one', scope: 'global' }, ctxFor({ confirms: [true] }).ctx))).toMatch(/^Saved M1 \(global/);
    expect(text(await run(tool, { op: 'set', title: 'Project one' }))).toMatch(/^Saved M2 \(project/);
    fs.mkdirSync(path.join(cwd, '.claude'), { recursive: true });
    fs.writeFileSync(path.join(cwd, '.claude', 'settings.json'), '{"hooks":{}}');
    const untrusted = () => ctxFor({ trusted: false, confirms: [true] }).ctx;
    await expect(run(tool, { op: 'set', title: 'Sneaky', scope: 'global' }, untrusted())).rejects.toThrow(/untrusted project/);
    await expect(run(tool, { op: 'set', title: 'Sneaky', pinned: true }, untrusted())).rejects.toThrow(/untrusted project/);
    await expect(run(tool, { op: 'set', id: 'M1', body: 'changed' }, untrusted())).rejects.toThrow(/untrusted project/);
    await expect(run(tool, { op: 'delete', id: 'M1' }, untrusted())).rejects.toThrow(/untrusted project/);
    expect(text(await run(tool, { op: 'set', title: 'Plain project note' }, untrusted()))).toMatch(/^Saved M3 \(project[\s\S]*hides project memories from reads until the user \/trusts it/);
    // Hidden project memories: changing them by id, or colliding with one, reveals nothing about them.
    for (const params of [{ op: 'set', id: 'M2', body: 'x' }, { op: 'delete', id: 'M2' }]) await expect(run(tool, params, untrusted())).rejects.toThrow(/^No memory M2 here/);
    await expect(run(tool, { op: 'set', title: 'Project one' }, untrusted())).rejects.toThrow(/^A similar project memory \(M2\) exists, hidden[^"]*$/);
    // Reads match auto-injection: only global memories, with a note why.
    const listed = text(await run(tool, { op: 'list' }, untrusted()));
    expect(listed).toContain('M1');
    expect(listed).not.toContain('M2');
    expect(listed).toMatch(/untrusted project: project memories are hidden/);
    expect(text(await run(tool, { op: 'search', query: 'project one' }, untrusted()))).not.toContain('M2');
    await expect(run(tool, { op: 'get', id: 'M2' }, untrusted())).rejects.toThrow(/No memory M2/);
  });

  it('is read-only in a subagent', async () => {
    const { tool, subcommands } = setup(true);
    expect(tool.parameters.properties.op.enum).toEqual(['search', 'get', 'list']);
    expect(tool.description).toContain('report anything worth remembering to your parent');
    expect(tool.promptSnippet).toBe('Search and read durable notes from earlier sessions');
    expect(tool.promptGuidelines.join(' ')).toMatch(/^Memory is read-only here/);
    expect(tool.promptGuidelines.join(' ')).not.toMatch(/Prefer updating/);
    await expect(run(tool, { op: 'set', title: 'x' })).rejects.toThrow(/read-only in a subagent/);
    await expect(run(tool, { op: 'delete', id: 'M1' })).rejects.toThrow(/read-only in a subagent/);
    expect(subcommands.get('memory')).toBeUndefined();
  });

  it('keeps its prompt lean: at most 2 short guidelines (1 in a subagent), every field described', () => {
    for (const [subagent, max] of [[false, 2], [true, 1]] as const) {
      const { tool } = setup(subagent);
      expect(tool.promptGuidelines.length).toBeLessThanOrEqual(max);
      for (const line of tool.promptGuidelines as string[]) expect(line.length, line).toBeLessThanOrEqual(230);
      expect([tool.description, ...tool.promptGuidelines].join(' ')).not.toMatch(/ {2}/);
      // Staleness is said once, in the injected header, not in every turn's guidelines.
      expect(tool.promptGuidelines.join(' ')).not.toMatch(/may be stale/);
      expect(MEMORY_HEADER).toMatch(/may be stale/);
      for (const [name, schema] of Object.entries(tool.parameters.properties as Record<string, { description?: string }>)) expect(schema.description, name).toBeTruthy();
    }
  });

  it('renders the call and a collapsed result', () => {
    const { tool } = setup();
    expect(rendered(tool.renderCall({ op: 'search', query: 'rewind flow' }, theme, {}))).toBe('○ Memory(search "rewind flow")');
    expect(rendered(tool.renderCall({ op: 'set', title: 'red \u001b[31mtext\u001b[0m\nnext' }, theme, {}))).toBe('○ Memory(set "red text")');
    expect(rendered(tool.renderCall({ op: 'get', id: 'M7' }, theme, {}))).toBe('○ Memory(get M7)');
    expect(setup().tool.promptGuidelines.join(' ')).toMatch(/scope global or pinned only when the user asks/);
    const hits = { content: [{ type: 'text', text: 'M1 a\nM2 b' }], details: { ids: [1, 2] } };
    expect(rendered(tool.renderResult(hits, { expanded: false }, theme, { args: { op: 'search' }, expanded: false }))).toBe('  ⎿  2 hits\n     M1 a\n     M2 b');
    expect(rendered(tool.renderResult({ content: [{ type: 'text', text: 'No memory matches.' }], details: { ids: [] } }, { expanded: false }, theme, { args: { op: 'search' } }))).toBe('  ⎿  No matches');
    const listed = { content: [{ type: 'text', text: 'M1 a' }], details: { ids: [1], total: 4 } };
    expect(rendered(tool.renderResult(listed, { expanded: false }, theme, { args: { op: 'list' } }))).toBe('  ⎿  1 of 4 memories\n     M1 a');
    const saved = { content: [{ type: 'text', text: 'Saved M7 [project fact] title' }], details: { ids: [7] } };
    expect(rendered(tool.renderResult(saved, { expanded: false }, theme, { args: { op: 'set' } }))).toBe('  ⎿  Saved M7 [project fact] title');
    expect(rendered(tool.renderResult({ content: [{ type: 'text', text: 'set needs a title' }] }, { expanded: false }, theme, { args: { op: 'set' }, isError: true }))).toBe('  ⎿  Error: set needs a title');
  });
});

describe('memory auto-injection', () => {
  const setup = (isSubagent = false) => {
    const fake = fakePi();
    registerMemory(fake.pi, { commands: new Subcommands(), isSubagent });
    return fake;
  };
  const seed = () => {
    const store = memoryStore(cwd);
    const project = add(store, { title: 'Rewind uses checkpoints', body: 'checkpoints live per session' });
    const global = add(store, { title: 'Checkpoints are cheap', scope: 'global' });
    return { store, project, global };
  };

  it('injects a hidden octocode-memory message once per branch and bumps usage', async () => {
    const fake = setup();
    const { store, project, global } = seed();
    const { ctx, branch } = ctxFor();
    const result = (await fake.fire('before_agent_start', { prompt: 'fix the rewind checkpoints bug' }, ctx)) as any;
    expect(result.message).toMatchObject({ customType: MEMORY_MESSAGE_TYPE, display: false });
    expect(result.message.details.ids.sort()).toEqual([project.id, global.id].sort());
    expect(result.message.content.startsWith(MEMORY_HEADER)).toBe(true);
    expect(store.get(project.id)?.useCount).toBe(1);
    branch.push({ type: 'custom_message', ...result.message });
    expect(await fake.fire('before_agent_start', { prompt: 'checkpoints again' }, ctx)).toBeUndefined();
  });

  it('injects only global memories in an untrusted project', async () => {
    const fake = setup();
    const { global } = seed();
    fs.mkdirSync(path.join(cwd, '.claude'), { recursive: true });
    fs.writeFileSync(path.join(cwd, '.claude', 'settings.json'), '{"hooks":{}}');
    const { ctx } = ctxFor({ trusted: false });
    const result = (await fake.fire('before_agent_start', { prompt: 'rewind checkpoints' }, ctx)) as any;
    expect(result.message.details.ids).toEqual([global.id]);
  });

  it('is off with OCTOCODE_MEMORY_AUTO=0 or the stored setting, and survives a broken DB', async () => {
    const fake = setup();
    const { store } = seed();
    vi.stubEnv('OCTOCODE_MEMORY_AUTO', '0');
    expect(await fake.fire('before_agent_start', { prompt: 'rewind' }, ctxFor().ctx)).toBeUndefined();
    vi.stubEnv('OCTOCODE_MEMORY_AUTO', '');
    store.setAutoSetting(false);
    expect(await fake.fire('before_agent_start', { prompt: 'rewind' }, ctxFor().ctx)).toBeUndefined();
    closeSharedAgentDb();
    const foreign = path.join(root, 'foreign.db');
    fs.writeFileSync(foreign, 'not a database');
    vi.stubEnv('OCTOCODE_AGENT_DB', foreign);
    expect(await fake.fire('before_agent_start', { prompt: 'rewind' }, ctxFor().ctx)).toBeUndefined();
  });

  it('gives a subagent at most 3 hits and no pinned memories (its parent holds those)', async () => {
    const fake = setup(true);
    const store = memoryStore(cwd);
    const pinned = add(store, { title: 'Always on', body: 'unrelated', pinned: true });
    for (let i = 0; i < 6; i++) add(store, { title: `Parser ${WORDS[i]}`, body: `parser ${WORDS[i + 10]} ${WORDS[i + 20]}` });
    const result = (await fake.fire('before_agent_start', { prompt: 'parser' }, ctxFor().ctx)) as any;
    expect(result.message.details.ids).toHaveLength(3);
    expect(result.message.details.ids).not.toContain(pinned.id);
  });

  it('injects no renderer: the message is hidden', () => {
    expect(setup().renderers.has(MEMORY_MESSAGE_TYPE)).toBe(false);
  });
});

describe('/octocode memory', () => {
  const setup = () => {
    const fake = fakePi();
    const commands = new Subcommands();
    registerMemory(fake.pi, { commands, isSubagent: false });
    const command = commands.get('memory')!;
    return { command, commands };
  };

  it('round-trips the editor template', () => {
    const input = parseTemplate(memoryTemplate({ title: 'T # not a comment', kind: 'gotcha', keywords: 'a b', scope: 'global', body: 'line1\nline2', pinned: true }));
    expect(input).toEqual({ title: 'T # not a comment', kind: 'gotcha', keywords: 'a b', scope: 'global', body: 'line1\nline2', pinned: true });
    expect(() => parseTemplate('kind: nope\n---\n')).toThrow(/kind must be/);
    expect(() => parseTemplate('scope: all\n---\n')).toThrow(/scope must be/);
    expect(parseTemplate('title: only')).toEqual({ title: 'only', body: '' });
  });

  it('prints grouped text headless, adds, searches, and toggles auto', async () => {
    const { command, commands } = setup();
    const { ctx } = ctxFor({ hasUI: false });
    await command.handler('', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toBe('Memories (auto on): none.');
    await command.handler('add Deploy with care — run the smoke tests first', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/^Saved M1 \(project, fact\) Deploy with care — run the smoke/);
    memoryStore(cwd).set({ title: 'Global pin', scope: 'global', pinned: true, author: 'user' });
    await command.handler('', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toBe(['Memories (auto on):', 'Pinned:', '  M2 (global, fact, pinned) Global pin', 'Project:', '  M1 (project, fact) Deploy with care — run the smoke tests first'].join('\n'));
    await command.handler('search smoke', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toContain('M1');
    await command.handler('search', ctx);
    expect(ctx.ui.notes.at(-1)!.type).toBe('warning');
    await command.handler('add', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/^Usage/);
    await command.handler('add deploy with CARE', ctx);
    expect(ctx.ui.notes.at(-1)).toMatchObject({ type: 'warning', message: expect.stringContaining('Similar memory M1') });
    await command.handler('auto off', ctx);
    expect(memoryStore(cwd).autoSetting()).toBe(false);
    await command.handler('auto', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toContain('is off');
    vi.stubEnv('OCTOCODE_MEMORY_AUTO', 'off');
    await command.handler('auto on', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toContain('Still off here');
    await command.handler('bogus', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/^Usage/);
    expect(commands.complete('memory au')?.map((item) => item.value)).toEqual(['memory auto on', 'memory auto off']);
  });

  it('shows the last injection on this branch', async () => {
    const { command } = setup();
    const { ctx, branch } = ctxFor();
    await command.handler('last', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/No memories were injected/);
    branch.push({ type: 'custom_message', customType: MEMORY_MESSAGE_TYPE, content: `${MEMORY_HEADER}\nM4 x`, details: { ids: [4] } });
    await command.handler('last', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toBe(`${MEMORY_HEADER}\nM4 x`);
  });

  it('adds through the editor and edits, pins, moves and deletes from the picker', async () => {
    const { command } = setup();
    const store = memoryStore(cwd);
    const created = parseTemplate('title: Editor made\nkind: decision\n---\nbody text');
    const { ctx, editorCalls } = ctxFor({
      editor: [memoryTemplate({ ...created, title: 'Editor made', kind: 'decision', keywords: '', scope: 'project', body: 'body text', pinned: false }), 'title: Edited title\nkind: decision\n---\nnew body'],
      selects: ['+ Add a memory'],
    });
    await command.handler('', ctx);
    expect(store.get(1)).toMatchObject({ title: 'Editor made', kind: 'decision', author: 'user' });
    expect(editorCalls[0]!.prefill).toContain('title: ');

    const option = (title: string) => `Project · M1 (project, decision) ${title} — `;
    ctx.ui.selects.push(option('Editor made') + 'body text', 'Edit');
    ctx.ui.selects.push(undefined);
    await command.handler('', ctx);
    expect(store.get(1)).toMatchObject({ title: 'Edited title', body: 'new body' });

    ctx.ui.selects.push(option('Edited title') + 'new body', 'Pin', 'Pinned · M1 (project, decision, pinned) Edited title — new body', 'Move to global', undefined);
    await command.handler('', ctx);
    expect(store.get(1)).toMatchObject({ pinned: true, scope: 'global' });

    ctx.ui.selects.push('Pinned · M1 (global, decision, pinned) Edited title — new body', 'Show', undefined);
    await command.handler('search edited', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toContain('author: user');

    ctx.ui.selects.push('Pinned · M1 (global, decision, pinned) Edited title — new body', 'Delete');
    ctx.ui.confirms.push(false);
    ctx.ui.selects.push('Pinned · M1 (global, decision, pinned) Edited title — new body', 'Delete', undefined);
    ctx.ui.confirms.push(true);
    await command.handler('', ctx);
    expect(store.get(1)).toBeUndefined();
    expect(ctx.ui.notes.at(-1)!.message).toBe('Deleted M1.');
  });
});

/**
 * BM25 relevance eval: a fixed corpus of memories and realistic prompts, each with the memory it should surface. Reports
 * top-1 and top-3 hit rates; the floor guards against a ranking regression, the log shows the numbers.
 */
describe('memory BM25 relevance eval', () => {
  const corpus: Array<[string, MemoryInput]> = [
    ['sqlite', { title: 'Use node:sqlite, never better-sqlite3', keywords: 'database native dependency', body: 'The extension ships without native modules; open databases with DatabaseSync from node:sqlite.' }],
    ['tabs', { title: 'User prefers two-space indentation', keywords: 'formatting style tabs spaces', body: 'Format TypeScript with two spaces, never tabs.', kind: 'preference', scope: 'global' }],
    ['release', { title: 'Release procedure', keywords: 'publish npm version tag', body: 'Bump the version, run yarn verify, then follow release/RELEASE_GUIDE.md to publish.', kind: 'procedure' }],
    ['trust', { title: 'Project config loads only when trusted', keywords: 'security mcp hooks untrusted', body: 'Project MCP files, hooks and skills need /octocode trust first.', kind: 'decision' }],
    ['wal', { title: 'SQLITE_BUSY on the team database', keywords: 'locked busy_timeout wal concurrency', body: 'Enable WAL and a 5s busy timeout; parallel subagents write at once.', kind: 'gotcha' }],
    ['layers', { title: 'Domain folders may only import lower layers', keywords: 'architecture layering imports', body: 'tests/architecture.test.ts enforces which src folders depend on which.', kind: 'decision' }],
    ['coverage', { title: 'Coverage target is 90 percent', keywords: 'tests vitest coverage threshold', body: 'Run yarn test with coverage; keep new code above 90%.' }],
    ['browser', { title: 'Browser tool needs Chrome with DevTools', keywords: 'cdp chromium headless screenshot', body: 'The browser tool drives Chrome over the DevTools protocol; headless unless webLive.' }],
    ['compaction', { title: 'Compaction keeps the file list', keywords: 'summary context window compress', body: 'The compaction summary lists touched files so work can resume after compressing context.' }],
    ['sanitize', { title: 'Sanitize untrusted output before rendering', keywords: 'escape ansi terminal injection', body: 'MCP, web and subagent text goes through sanitizeTerminalText before drawing.', kind: 'decision' }],
    ['worktree', { title: 'Subagents may run in git worktrees', keywords: 'isolation branch parallel', body: 'A profile with worktree: true runs its subagent in a fresh git worktree.' }],
    ['eslint', { title: 'Lint runs eslint and tsc', keywords: 'lint typecheck static analysis', body: 'yarn lint runs eslint then tsc --noEmit.', kind: 'procedure' }],
    ['python', { title: 'Python scripts use uv', keywords: 'pip virtualenv tooling', body: 'Run helper scripts with uv run, not pip install.', kind: 'preference', scope: 'global' }],
    ['mac', { title: 'macOS sed needs an empty -i argument', keywords: 'bsd gnu in-place edit', body: "Use sed -i '' on macOS; GNU sed differs.", kind: 'gotcha', scope: 'global' }],
    ['commits', { title: 'Never commit without asking', keywords: 'git push approval', body: 'Ask the user before committing or pushing anything.', kind: 'preference', scope: 'global' }],
  ];
  const queries: Array<[string, string]> = [
    ['which database library should I use for storing data?', 'sqlite'],
    ['the tests fail with database is locked when subagents run in parallel', 'wal'],
    ['how do I publish a new version to npm', 'release'],
    ['format this file, should I use tabs', 'tabs'],
    ['can the web tool take a screenshot in headless chrome', 'browser'],
    ['is it ok to git push my changes now', 'commits'],
    ['MCP output shows weird ANSI escape codes in the terminal', 'sanitize'],
    ['what coverage threshold do the vitest tests need', 'coverage'],
    ['sed -i fails on my mac', 'mac'],
  ];
  /** Prompts no memory is about: auto-injection should stay quiet for them. */
  const negatives = [
    'write a haiku about autumn',
    'rename variable foo to bar in utils.ts',
    'add a new button to the settings page',
    'explain what a monad is',
    'translate this paragraph into French',
    'why is the sky blue',
  ];

  const evaluate = (db: AgentDb, label: string) => {
    const store = new MemoryStore(db, 'repo-eval');
    const ids = new Map(corpus.map(([key, input]) => [add(store, input).id, key]));
    let top1 = 0;
    let top3 = 0;
    let injected = 0;
    const misses: string[] = [];
    for (const [query, expected] of queries) {
      const ranked = store.search(query, 'all', 3).map((hit) => ids.get(hit.id));
      if (ranked[0] === expected) top1++;
      else misses.push(`${query} → ${ranked.join(', ')}`);
      if (ranked.includes(expected)) top3++;
      if (selectInjection(store, { query, scope: 'all', seen: new Set(), topK: 5 })?.ids.some((id) => ids.get(id) === expected)) injected++;
    }
    const noisy = negatives.filter((query) => selectInjection(store, { query, scope: 'all', seen: new Set(), topK: 5 }));
    console.log(
      `memory ${label} eval: top-1 ${top1}/${queries.length}, top-3 ${top3}/${queries.length}, expected injected ${injected}/${queries.length}, negatives injected ${noisy.length}/${negatives.length}` +
        `${misses.length ? `; top-1 misses: ${misses.join(' | ')}` : ''}${noisy.length ? `; noisy: ${noisy.join(' | ')}` : ''}`,
    );
    return { top1: top1 / queries.length, top3: top3 / queries.length, injected: injected / queries.length, noisy: noisy.length };
  };

  it('ranks the expected memory first for most prompts and in the top 3 for all (BM25)', () => {
    const result = evaluate(freshDb(), 'BM25');
    expect(result.top3).toBe(1);
    expect(result.top1).toBeGreaterThanOrEqual(0.75);
    expect(result.injected).toBeGreaterThanOrEqual(0.85);
    expect(result.noisy).toBe(0);
  });

  it('keeps useful recall with the LIKE fallback', () => {
    const result = evaluate({ ...freshDb(), fts: false }, 'LIKE');
    expect(result.top3).toBeGreaterThanOrEqual(0.75);
    expect(result.noisy).toBe(0);
  });
});
