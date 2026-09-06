import assert from 'node:assert/strict';
import { test } from 'vitest';
import type { AutocompleteItem, PiAutocompleteProvider } from '../src/types.js';
import {
  extractTokenPrefix,
  buildSuggestionItems,
  createOctocodeAutocompleteProvider,
  registerOctocodeAutocomplete,
  resetOctocodeAutocompleteInstallForTests,
  type OctocodeAutocompleteDeps,
} from '../src/tools/autocomplete-providers.js';

// ─── Fakes ───────────────────────────────────────────────────────────────────

function deps(overrides: Partial<OctocodeAutocompleteDeps> = {}): OctocodeAutocompleteDeps {
  return {
    listWorkers: () => [],
    getPlanSteps: () => [],
    listSkills: () => [],
    ...overrides,
  };
}

function fakeCurrent() {
  const calls = { getSuggestions: [] as unknown[], applyCompletion: [] as unknown[] };
  const provider = {
    triggerCharacters: ['@', '/'],
    getSuggestions: async (...args: unknown[]) => {
      calls.getSuggestions.push(args);
      return { prefix: 'cur', items: [{ value: 'from-current', label: 'from-current' }] };
    },
    applyCompletion: (lines: string[], line: number, col: number, ...rest: unknown[]) => {
      calls.applyCompletion.push([lines, line, col, ...rest]);
      return { lines: ['CURRENT'], cursorLine: 0, cursorCol: 7 };
    },
    shouldTriggerFileCompletion: () => true,
  } as unknown as PiAutocompleteProvider;
  return { provider, calls };
}

const OPTS = { signal: new AbortController().signal, force: false };

// ─── extractTokenPrefix ──────────────────────────────────────────────────────

test('extractTokenPrefix: token at start of line', () => {
  assert.deepEqual(extractTokenPrefix(['@ab'], 0, 3), { trigger: '@', prefix: 'ab', startCol: 0 });
});

test('extractTokenPrefix: token mid-line after whitespace', () => {
  assert.deepEqual(extractTokenPrefix(['run @wo now'], 0, 7), { trigger: '@', prefix: 'wo', startCol: 4 });
});

test('extractTokenPrefix: hash trigger', () => {
  assert.deepEqual(extractTokenPrefix(['see #2'], 0, 6), { trigger: '#', prefix: '2', startCol: 4 });
});

test('extractTokenPrefix: no false trigger inside a word (email)', () => {
  assert.equal(extractTokenPrefix(['a@b'], 0, 3), undefined);
  assert.equal(extractTokenPrefix(['mail me a@b.com'], 0, 15), undefined);
});

test('extractTokenPrefix: cursor mid-token only takes prefix up to cursor', () => {
  assert.deepEqual(extractTokenPrefix(['@worker'], 0, 3), { trigger: '@', prefix: 'wo', startCol: 0 });
});

test('extractTokenPrefix: bare trigger char yields empty prefix', () => {
  assert.deepEqual(extractTokenPrefix(['@'], 0, 1), { trigger: '@', prefix: '', startCol: 0 });
});

test('extractTokenPrefix: misses on plain words, empty lines, bad cursor line', () => {
  assert.equal(extractTokenPrefix(['hello'], 0, 5), undefined);
  assert.equal(extractTokenPrefix([''], 0, 0), undefined);
  assert.equal(extractTokenPrefix(['@x'], 5, 1), undefined);
  // cursor at col 0 sits before any trigger char
  assert.equal(extractTokenPrefix(['@x'], 0, 0), undefined);
});

test('extractTokenPrefix: multi-line input uses the cursor line', () => {
  assert.deepEqual(extractTokenPrefix(['first', 'x #1 y'], 1, 4), { trigger: '#', prefix: '1', startCol: 2 });
});

// ─── Filtering + cap ─────────────────────────────────────────────────────────

test('buildSuggestionItems: workers filter case-insensitively by id or name', () => {
  const d = deps({
    listWorkers: () => [
      { agentId: 'abc12345', name: 'Alpha', status: 'running' },
      { agentId: 'zzz99999', name: 'beta', status: 'idle' },
    ],
  });
  const byName = buildSuggestionItems({ trigger: '@', prefix: 'AL', startCol: 0 }, d);
  assert.equal(byName.length, 1);
  assert.equal(byName[0]!.value, '@abc12345');
  const byId = buildSuggestionItems({ trigger: '@', prefix: 'zzz', startCol: 0 }, d);
  assert.equal(byId.length, 1);
  assert.equal(byId[0]!.value, '@zzz99999');
});

test('buildSuggestionItems: @ merges workers and skills; cap is 20', () => {
  const workers = Array.from({ length: 15 }, (_, i) => ({ agentId: `w${i}id`, name: `w${i}`, status: 'running' }));
  const skills = Array.from({ length: 15 }, (_, i) => ({ name: `wskill${i}` }));
  const items = buildSuggestionItems({ trigger: '@', prefix: 'w', startCol: 0 }, deps({
    listWorkers: () => workers,
    listSkills: () => skills,
  }));
  assert.equal(items.length, 20);
  assert.equal(items[0]!.value, '@w0id');
  assert.ok(items.some((i) => i.value === '@wskill0'));
});

test('buildSuggestionItems: plan steps match by 1-based index or title', () => {
  const d = deps({
    getPlanSteps: () => [
      { text: 'Alpha step', status: 'done' },
      { text: 'Beta step', status: 'todo' },
    ],
  });
  const byIndex = buildSuggestionItems({ trigger: '#', prefix: '2', startCol: 0 }, d);
  assert.equal(byIndex.length, 1);
  assert.equal(byIndex[0]!.value, '#2');
  assert.match(byIndex[0]!.label, /Beta step/);
  const byTitle = buildSuggestionItems({ trigger: '#', prefix: 'beta', startCol: 0 }, d);
  assert.deepEqual(byTitle.map((i) => i.value), ['#2']);
  const all = buildSuggestionItems({ trigger: '#', prefix: '', startCol: 0 }, d);
  assert.deepEqual(all.map((i) => i.value), ['#1', '#2']);
});

test('buildSuggestionItems: throwing deps are treated as empty', () => {
  const d = deps({
    listWorkers: () => { throw new Error('boom'); },
    listSkills: () => [{ name: 'safe' }],
  });
  const items = buildSuggestionItems({ trigger: '@', prefix: '', startCol: 0 }, d);
  assert.deepEqual(items.map((i) => i.value), ['@safe']);
});

// ─── Provider: delegation + suggestions ──────────────────────────────────────

test('provider delegates to current when no token under cursor', async () => {
  const { provider: current, calls } = fakeCurrent();
  const p = createOctocodeAutocompleteProvider(current, deps());
  const result = await p.getSuggestions(['plain text'], 0, 5, OPTS);
  assert.equal(calls.getSuggestions.length, 1);
  assert.equal(result?.items[0]?.value, 'from-current');
});

test('provider returns own suggestions when token matches', async () => {
  const { provider: current, calls } = fakeCurrent();
  const p = createOctocodeAutocompleteProvider(current, deps({
    listWorkers: () => [{ agentId: 'abc123', name: 'searcher', status: 'running' }],
  }));
  const result = await p.getSuggestions(['ping @sea'], 0, 9, OPTS);
  assert.equal(calls.getSuggestions.length, 0);
  assert.equal(result?.prefix, 'sea');
  assert.deepEqual(result?.items.map((i) => i.value), ['@abc123']);
});

test('provider falls through to current when token matches nothing (keeps @file completion)', async () => {
  const { provider: current, calls } = fakeCurrent();
  const p = createOctocodeAutocompleteProvider(current, deps());
  const result = await p.getSuggestions(['open @src/ind'], 0, 13, OPTS);
  assert.equal(calls.getSuggestions.length, 1);
  assert.equal(result?.items[0]?.value, 'from-current');
});

test('provider merges trigger characters with current', () => {
  const { provider: current } = fakeCurrent();
  const p = createOctocodeAutocompleteProvider(current, deps());
  assert.deepEqual([...(p.triggerCharacters ?? [])].sort(), ['#', '/', '@'].sort());
});

test('provider delegates shouldTriggerFileCompletion to current', () => {
  const { provider: current } = fakeCurrent();
  const p = createOctocodeAutocompleteProvider(current, deps());
  assert.equal(p.shouldTriggerFileCompletion?.(['x'], 0, 1), true);
  const bare = createOctocodeAutocompleteProvider({} as unknown as PiAutocompleteProvider, deps());
  assert.equal(bare.shouldTriggerFileCompletion?.(['x'], 0, 1), false);
});

// ─── applyCompletion cursor math ─────────────────────────────────────────────

test('applyCompletion replaces token on a single line and moves the cursor', () => {
  const { provider: current } = fakeCurrent();
  const p = createOctocodeAutocompleteProvider(current, deps());
  const item: AutocompleteItem = { value: '@alpha-1', label: '@alpha-1' };
  const out = p.applyCompletion(['say @al'], 0, 7, item, 'al');
  assert.deepEqual(out, { lines: ['say @alpha-1'], cursorLine: 0, cursorCol: 12 });
});

test('applyCompletion works mid-line with a suffix and on multi-line input', () => {
  const { provider: current } = fakeCurrent();
  const p = createOctocodeAutocompleteProvider(current, deps());
  const item: AutocompleteItem = { value: '#12', label: '#12 step' };
  const out = p.applyCompletion(['first', 'x #1 y'], 1, 4, item, '1');
  assert.deepEqual(out, { lines: ['first', 'x #12 y'], cursorLine: 1, cursorCol: 5 });
});

test('applyCompletion delegates to current when no token or foreign item', () => {
  const { provider: current, calls } = fakeCurrent();
  const p = createOctocodeAutocompleteProvider(current, deps());
  // No token under cursor
  const noToken = p.applyCompletion(['plain'], 0, 5, { value: 'x', label: 'x' }, 'pl');
  assert.deepEqual(noToken, { lines: ['CURRENT'], cursorLine: 0, cursorCol: 7 });
  // Token present but item is not ours (no trigger sigil) — e.g. current's file item on '@'
  p.applyCompletion(['open @sr'], 0, 8, { value: 'src/index.ts', label: 'src/index.ts' }, 'sr');
  assert.equal(calls.applyCompletion.length, 2);
});

// ─── Install guard ───────────────────────────────────────────────────────────

test('registerOctocodeAutocomplete installs once per process', () => {
  resetOctocodeAutocompleteInstallForTests();
  const factories: unknown[] = [];
  const ui = {
    addAutocompleteProvider: (factory: (current: PiAutocompleteProvider) => PiAutocompleteProvider) => {
      factories.push(factory);
    },
  };
  assert.equal(registerOctocodeAutocomplete(ui, deps()), true);
  assert.equal(registerOctocodeAutocomplete(ui, deps()), false);
  assert.equal(factories.length, 1);
  // The installed factory produces a working decorator
  const { provider: current } = fakeCurrent();
  const built = (factories[0] as (c: PiAutocompleteProvider) => PiAutocompleteProvider)(current);
  assert.equal(typeof built.getSuggestions, 'function');
  resetOctocodeAutocompleteInstallForTests();
});

test('registerOctocodeAutocomplete is a no-op without the API', () => {
  resetOctocodeAutocompleteInstallForTests();
  assert.equal(registerOctocodeAutocomplete(undefined, deps()), false);
  assert.equal(registerOctocodeAutocomplete({}, deps()), false);
  // Missing API must NOT consume the once-per-process guard
  const factories: unknown[] = [];
  const ui = { addAutocompleteProvider: (f: unknown) => factories.push(f) };
  assert.equal(registerOctocodeAutocomplete(ui as never, deps()), true);
  assert.equal(factories.length, 1);
  resetOctocodeAutocompleteInstallForTests();
});
