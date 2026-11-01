import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { buildWiki, lintWiki, pageOf, promote, readPage, writeNotes } from './wiki.mjs';
import { today } from './wiki-store.mjs';
import { tempDir } from './test-tmp.mjs';

const SOURCES = {
  'src/a/x.ts': 'export function main() {\n  return run();\n}\n',
  'src/a/y.ts': 'export const y = 1;\n',
  'src/a/z.ts': 'export const z = 2;\n',
  'src/a/w.ts': 'export const w = 3;\n',
  'src/b/p.ts': 'export function run() {\n  return 1;\n}\n',
  'src/b/q.ts': 'export const q = 4;\n',
  'src/c/lonely.md': '# lonely\n',
};

function fixture() {
  const root = tempDir('gv-wiki-');
  for (const [p, text] of Object.entries(SOURCES)) {
    mkdirSync(dirname(join(root, p)), { recursive: true });
    writeFileSync(join(root, p), text);
  }
  return { root, out: join(root, 'out') };
}

const PR = { 'src/a/x.ts': 0.3, 'src/a/y.ts': 0.1, 'src/a/z.ts': 0.08, 'src/a/w.ts': 0.02, 'src/b/p.ts': 0.25, 'src/b/q.ts': 0.05, 'src/c/lonely.md': 0.01 };

// A hand-built data.json after computeMetrics: groups are [label, members] in community-id order.
function makeD(root, groups, check = {}) {
  const community = new Map(groups.flatMap(([, members], id) => members.map((m) => [m, id])));
  const files = [...community.keys()].map((id) => ({
    id, language: id.endsWith('.md') ? 'markdown' : 'typescript', chunk: '.', loc: { path: id, abs: join(root, id), line: 1 },
    check: { status: check[id] ?? 'ok', lines: SOURCES[id].split('\n').length - 1, problems: check[id] ? ['changed since the snapshot'] : [] },
    metrics: { pagerank: PR[id], betweenness: 0, inWeight: 1, outWeight: 1, depth: 0, core: 1, community: community.get(id), cycle: null },
  }));
  const communities = groups.map(([label, members], id) => ({ id, label, size: members.length, members: members.toSorted((a, b) => PR[b] - PR[a]), chunk: '.', purity: 1, internal: 2, external: 1 }));
  return {
    files, communities,
    imports: [{ source: 'src/a/z.ts', target: 'src/b/p.ts', lines: [1] }],
    calls: [{ source: 'src/a/x.ts#main', target: 'src/b/p.ts#run', lines: [2], count: 1 }],
    symbols: [{ id: 'src/b/p.ts#run', file: 'src/b/p.ts', name: 'run', kind: 'function', line: 1, endLine: 3, loc: { path: 'src/b/p.ts', line: 1 } }],
    fileEdges: [
      { source: 'src/a/x.ts', target: 'src/a/y.ts', weight: 1 }, { source: 'src/a/y.ts', target: 'src/a/z.ts', weight: 1 },
      { source: 'src/a/z.ts', target: 'src/b/p.ts', weight: 2 }, { source: 'src/a/x.ts', target: 'src/b/p.ts', weight: 1 },
      { source: 'src/b/q.ts', target: 'src/a/x.ts', weight: 0.5 },
    ],
    metrics: { communities, cycles: [] },
  };
}

const A = ['src/a', ['src/a/x.ts', 'src/a/y.ts', 'src/a/z.ts']];
const B = ['src/b', ['src/b/p.ts', 'src/b/q.ts']];
const C = ['src/c', ['src/c/lonely.md']];
const page = (out, slug) => readFileSync(join(out, 'wiki', 'pages', `${slug}.md`), 'utf8');

test('a build writes index, one page per community, the isolated listing and the log', () => {
  const { root, out } = fixture();
  const r = buildWiki({ D: makeD(root, [A, B, C]), out });
  assert.deepEqual(r.created.toSorted((a, b) => a.localeCompare(b)), ['isolated', 'src-a', 'src-b']);
  assert.deepEqual(r.pages.map((p) => p.slug), ['src-a', 'src-b', 'isolated']);
  assert.equal(r.pages[0].notesStale, false);
  const a = page(out, 'src-a');
  assert.match(a, /^# src\/a\n/);
  assert.match(a, /\| 1 \| `src\/a\/x\.ts:1` \| typescript \| 0\.3 \|/);
  assert.match(a, /\| \[\[src-b\]\] \| src\/b \| 3 \| 2 \| `src\/a\/z\.ts:1` → `src\/b\/p\.ts:1` \(2\) \|/);
  assert.match(a, /\| \[\[src-b\]\] \| src\/b \| 0\.5 \| 1 \|/);
  assert.match(page(out, 'src-b'), /\| run \| function \| `src\/b\/p\.ts:1` \| 1 \| 1 \| 1 \|/);
  assert.match(page(out, 'isolated'), /`src\/c\/lonely\.md:1`/);
  const index = readPage({ out, slug: 'index' }).markdown;
  assert.ok(index.indexOf('[[src-a]]') < index.indexOf('[[src-b]]'), 'ranked by summed PageRank');
  assert.match(index, /\[\[isolated\]\] \(1 files/);
  assert.match(readPage({ out, slug: 'log' }).markdown, /build \(full run\)[\s\S]*created \[\[src-a\]\]/);
  assert.equal(pageOf({ out, file: 'src/b/p.ts#run' }), 'src-b');
  assert.equal(pageOf({ out, file: 'src/c/lonely.md' }), 'isolated');
  assert.equal(readPage({ out, slug: '../map' }), null);
  rmSync(root, { recursive: true, force: true });
});

test('notes survive a rebuild verbatim and index.md summarizes them', () => {
  const { root, out } = fixture();
  buildWiki({ D: makeD(root, [A, B, C]), out });
  const notes = 'Entry point of the app. `main` calls `run` at src/a/x.ts:2.\n\n- invariant: y and z are constants';
  assert.deepEqual(writeNotes({ out, slug: 'src-a', text: notes }), { slug: 'src-a', updated: true });
  const r = buildWiki({ D: makeD(root, [A, B, C]), out });
  assert.deepEqual(r.updated, []);
  assert.ok(page(out, 'src-a').includes(`<!-- notes:start -->\n${notes}\n<!-- notes:end -->`));
  assert.match(page(out, 'src-a'), /> \*\*Notes:\*\* fresh/);
  assert.match(readPage({ out, slug: 'index' }).markdown, /\| \[\[src-a\]\] \| src\/a \| 3 \| [\d.]+ \| fresh \| Entry point of the app\. \|/);
  rmSync(root, { recursive: true, force: true });
});

test('slugs and notes stay when Louvain renumbers communities and membership shifts slightly', () => {
  const { root, out } = fixture();
  buildWiki({ D: makeD(root, [A, B, C]), out });
  writeNotes({ out, slug: 'src-a', text: 'About a.' });
  writeNotes({ out, slug: 'src-b', text: 'About b.' });
  const shifted = makeD(root, [['src/b', ['src/b/p.ts', 'src/b/q.ts']], ['src/a', ['src/a/x.ts', 'src/a/y.ts', 'src/a/z.ts', 'src/a/w.ts']], C]);
  const r = buildWiki({ D: shifted, out });
  assert.deepEqual(r.created, []);
  const bySlug = Object.fromEntries(r.pages.map((p) => [p.slug, p]));
  assert.equal(bySlug['src-a'].community, 1);
  assert.equal(bySlug['src-b'].community, 0);
  assert.match(page(out, 'src-a'), /About a\./);
  assert.match(page(out, 'src-a'), /`src\/a\/w\.ts:1`/);
  assert.match(readPage({ out, slug: 'log' }).markdown, /updated \[\[src-a\]\]: community 0 is now 1, members \+1 −0/);
  rmSync(root, { recursive: true, force: true });
});

test('a refresh with one changed member marks only that page stale and rewrites only its facts', () => {
  const { root, out } = fixture();
  buildWiki({ D: makeD(root, [A, B, C]), out });
  writeNotes({ out, slug: 'src-a', text: 'About a.' });
  writeNotes({ out, slug: 'src-b', text: 'About b, see src/b/p.ts:1.' });
  const before = page(out, 'src-a');
  writeFileSync(join(root, 'src/b/p.ts'), 'export function run() {\n  return 2;\n}\n');
  const r = buildWiki({ D: makeD(root, [A, B, C], { 'src/b/p.ts': 'stale' }), out, changed: ['src/b/p.ts'] });
  assert.deepEqual(r.updated, ['src-b']);
  assert.deepEqual(r.pages.filter((p) => p.notesStale).map((p) => p.slug), ['src-b']);
  assert.equal(page(out, 'src-a'), before);
  assert.match(page(out, 'src-b'), /> \*\*Notes:\*\* stale: 1 member changed since the notes were written [\d-]+: `src\/b\/p\.ts`/);
  assert.match(page(out, 'src-b'), /- `src\/b\/p\.ts:1` stale: changed since the snapshot/);
  const log = readPage({ out, slug: 'log' }).markdown;
  assert.match(log, /refresh: 1 files changed, added or removed: `src\/b\/p\.ts`\n\n- stale \[\[src-b\]\]: the notes predate changes to `src\/b\/p\.ts`\n- updated \[\[src-b\]\]: notes stale\n/);
  assert.match(readPage({ out, slug: 'index' }).markdown, /\*\*Stale notes\*\* \(a member changed since they were written: revisit\): \[\[src-b\]\]/);
  writeNotes({ out, slug: 'src-b', text: 'About b, revisited.' });
  assert.equal(buildWiki({ D: makeD(root, [A, B, C]), out }).pages.find((p) => p.slug === 'src-b').notesStale, false);
  rmSync(root, { recursive: true, force: true });
});

test('lint finds a bad citation, a missing file, a broken link, a missing page and empty top notes', () => {
  const { root, out } = fixture();
  const D = makeD(root, [A, B, C]);
  buildWiki({ D, out });
  writeNotes({ out, slug: 'src-b', text: 'See src/b/p.ts:2 and src/b/p.ts:1-3, then src/b/p.ts:40, src/gone.ts:3 and [[nope]]. Not a file: http://localhost:8080.' });
  const extra = makeD(root, [A, B, ['src/c', ['src/c/lonely.md', 'src/a/w.ts']]]);
  writeFileSync(join(out, 'data.json'), JSON.stringify(extra));
  const { issues } = lintWiki({ out, root });
  const kinds = issues.map((i) => `${i.kind} ${i.slug}`).toSorted();
  assert.deepEqual(kinds, ['broken-link src-b', 'citation-file src-b', 'citation-line src-b', 'empty-notes src-a', 'missing-page -']);
  assert.match(issues.find((i) => i.kind === 'citation-line').detail, /src\/b\/p\.ts:40: src\/b\/p\.ts has 3 lines/);
  rmSync(root, { recursive: true, force: true });
});

test('promote appends with source and date and keeps the earlier notes', () => {
  const { root, out } = fixture();
  buildWiki({ D: makeD(root, [A, B, C]), out });
  writeNotes({ out, slug: 'src-b', text: 'Runs things.' });
  assert.deepEqual(promote({ out, slug: 'src-b', text: '`run` returns 1 (src/b/p.ts:2).', source: 'chat 7' }), { slug: 'src-b', appended: true });
  assert.ok(page(out, 'src-b').includes(`<!-- notes:start -->\nRuns things.\n\n**Promoted ${today()} from chat 7:** \`run\` returns 1 (src/b/p.ts:2).\n<!-- notes:end -->`));
  assert.throws(() => promote({ out, slug: 'src-b', text: 'x' }), /source/);
  assert.match(readPage({ out, slug: 'log' }).markdown, /promoted into \[\[src-b\]\] from chat 7/);
  rmSync(root, { recursive: true, force: true });
});

test('a vanished community keeps its page as orphaned when it has notes and drops it otherwise', () => {
  const { root, out } = fixture();
  buildWiki({ D: makeD(root, [A, B, C]), out });
  writeNotes({ out, slug: 'src-b', text: 'Notes worth keeping.' });
  const merged = makeD(root, [['src', ['src/a/x.ts', 'src/a/y.ts', 'src/a/z.ts']], ['src/b/q.ts', ['src/b/q.ts']], ['src/b/p.ts', ['src/b/p.ts']], C]);
  buildWiki({ D: merged, out });
  const lone = makeD(root, [['src/x', ['src/a/w.ts', 'src/c/lonely.md']], ['x', ['src/a/x.ts']], ['y', ['src/a/y.ts']], ['z', ['src/a/z.ts']], ['p', ['src/b/p.ts']], ['q', ['src/b/q.ts']]]);
  const r = buildWiki({ D: lone, out });
  assert.deepEqual(r.removed, ['src-a']);
  assert.deepEqual(r.created, ['src-x']);
  assert.match(page(out, 'src-b'), /\*\*orphaned\*\* since/);
  assert.match(page(out, 'src-b'), /Notes worth keeping\./);
  assert.match(readPage({ out, slug: 'index' }).markdown, /## Orphaned pages[\s\S]*\[\[src-b\]\]/);
  assert.ok(lintWiki({ out, root }).issues.some((i) => i.kind === 'orphaned' && i.slug === 'src-b'));
  rmSync(root, { recursive: true, force: true });
});
