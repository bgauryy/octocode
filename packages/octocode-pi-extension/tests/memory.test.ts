import { describe, expect, it } from 'vitest';
import type { AgentDb } from '../src/agentdb/db.js';
import { injectedIds, lastInjection, MEMORY_HEADER, MEMORY_MESSAGE_TYPE, PINNED_BUDGET, selectInjection, TOTAL_BUDGET } from '../src/memory/inject.js';
import { ftsQuery, jaccard, normalizeTitle, parseMemoryId, terms } from '../src/memory/query.js';
import { MemoryError, MemoryStore, memoryLine, type MemoryInput } from '../src/memory/store.js';
import { add, freshDb, useMemoryFixture, WORDS } from './memory-fixture.js';

useMemoryFixture();

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

  it('updates by id, moves scope, pins, and sanitizes text without cutting it', () => {
    const store = new MemoryStore(freshDb(), 'repo-a');
    const memory = add(store, { title: 'Title \u001b[31mred\u001b[0m', body: 'x'.repeat(2000), kind: 'gotcha' }, 1000);
    expect(memory.title).toBe('Title red');
    expect(memory.body).toHaveLength(2000);
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
    expect(injection.content).toContain(`M${strong.id} (project, fact) (agent-written) Checkpoint rewind flow — "Rewind restores`);
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
    expect([...injectedIds(entries).keys()]).toEqual([1, 2, 3]);
    expect([...injectedIds([...entries, { type: 'compaction' }, message([4])]).keys()]).toEqual([4]);
    expect(lastInjection(entries)).toBe('second');
    expect(lastInjection([null, 5])).toBeUndefined();
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
