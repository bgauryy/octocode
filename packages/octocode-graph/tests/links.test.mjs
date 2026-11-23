import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { docLinks, scanMarkdown, slug } from '../src/runtime/links-docs.ts';
import { tempDir } from './test-tmp.mjs';

const tree = (files) => {
  const root = tempDir('graph-links-');
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), text);
  }
  return { root, listing: Object.keys(files) };
};
const sym = (file, local) => ({ id: `${file}#${local}`, file, name: local.split('.').pop(), kind: 'function', line: 1, exported: true, calls: 0 });

test('the scanner skips fences, comments and escapes, keeps code spans whole and reads link destinations', () => {
  const md = [
    '# Title',
    'See [the query](src/query.ts) and `src/a.ts` and \\[not](a link).',
    'A span holding link syntax: `[x](y.ts)` stays one span.',
    '```ts',
    '[fenced](nope.ts) `fenced`',
    '```',
    '<!-- [hidden](gone.ts) -->',
    '![image](docs/pic.png) [paren](a_(b).ts) [angle](<with space.md>)',
    '[ref]: docs/guide.md',
    '## Title',
  ].join('\n');
  const { mentions, slugs } = scanMarkdown(md);
  assert.deepEqual(mentions.map((m) => [m.kind, m.text, m.line]), [
    ['link', 'src/query.ts', 2],
    ['span', 'src/a.ts', 2],
    ['span', '[x](y.ts)', 3],
    ['link', 'docs/pic.png', 8],
    ['link', 'a_(b).ts', 8],
    ['link', 'with space.md', 8],
    ['link', 'docs/guide.md', 9],
  ]);
  // GitHub slugs, with -1 for a repeated heading.
  assert.deepEqual([...slugs], ['title', 'title-1']);
  assert.equal(slug('Store, diff, check, search'), 'store-diff-check-search');
});

test('docs resolve links, paths, citations and placed symbols; everything else is unresolved with its reason', () => {
  const { root, listing } = tree({
    'pkg/package.json': '{}',
    'pkg/src/query.ts': 'export function runQuery() {}\nexport function parseArgs() {}\n',
    'pkg/src/other.ts': 'export function parseArgs() {}\n',
    'pkg/README.md': [
      '# Query',
      'See [query](src/query.ts) and [guide](../docs/guide.md#store) and [missing](src/gone.ts).',
      'The entry is `src/query.ts`; it starts at `src/query.ts:1`, not `src/query.ts:99`.',
      '`runQuery` reads.',
      '',
      '## Names',
      '`parseArgs` is declared twice; the word `parse` alone is not a symbol mention.',
      '',
      '## Parsing',
      '`parseArgs` here means the one in `src/other.ts`.',
      'A dead path: `src/team/peer-registry.ts`. Not paths: `owner/repo`, `--json`, `https://x.dev/a.ts`.',
    ].join('\n'),
    'docs/guide.md': '# Store\nOutside the package, `runQuery` is only a spelling.\n',
  });
  const D = { symbols: [sym('pkg/src/query.ts', 'runQuery'), sym('pkg/src/query.ts', 'parseArgs'), sym('pkg/src/other.ts', 'parseArgs')] };
  const { links, unresolved } = docLinks(D, root, listing);
  const row = (l) => [l.rule, l.line, l.target, ...(l.lines ? [l.lines.join('-')] : [])];
  assert.deepEqual(links.map(row), [
    ['md-link', 2, 'pkg/src/query.ts'],
    ['md-link', 2, 'docs/guide.md'],
    ['md-path', 3, 'pkg/src/query.ts'],
    ['md-cite', 3, 'pkg/src/query.ts', '1-1'],
    ['md-path', 10, 'pkg/src/other.ts'],
    ['md-symbol', 4, 'pkg/src/query.ts#runQuery'],
    // Its own section names src/other.ts: the shared name means the one declared there.
    ['md-symbol', 10, 'pkg/src/other.ts#parseArgs'],
  ]);
  assert.deepEqual(unresolved.map((u) => [u.rule, u.line, u.text, u.reason, u.candidates?.length ?? 0]), [
    ['md-link', 2, 'src/gone.ts', 'no-such-file', 0],
    ['md-cite', 3, 'src/query.ts:99', 'line-out-of-range', 0],
    ['md-path', 11, 'src/team/peer-registry.ts', 'no-such-file', 0],
    // A code-shaped name its section does not place is unconfirmed, with its candidates for an agent to settle;
    // the plain word `parse` is no mention at all.
    ['md-symbol', 7, 'parseArgs', 'unconfirmed', 2],
    ['md-symbol', 2, 'runQuery', 'unconfirmed', 1],
  ]);
  assert.ok(links.every((l) => l.check === 'ok' && l.sourceHash));
});

test('a citation goes stale when the cited lines change under an unchanged doc line, and recovers when the doc changes', () => {
  const files = { 'a.ts': 'one\ntwo\nthree\n', 'doc.md': 'See `a.ts:2`.\n' };
  const { root, listing } = tree(files);
  const first = docLinks({ symbols: [] }, root, listing).links;
  assert.deepEqual(first.map((l) => [l.rule, l.check]), [['md-cite', 'ok']]);
  writeFileSync(join(root, 'a.ts'), 'one\nTWO changed\nthree\n');
  const second = docLinks({ symbols: [] }, root, listing, first).links;
  assert.equal(second[0].check, 'stale');
  // Still stale while the doc line stays as it was, even with no further code change.
  assert.equal(docLinks({ symbols: [] }, root, listing, second).links[0].check, 'stale');
  writeFileSync(join(root, 'doc.md'), 'See `a.ts:2` (reviewed).\n');
  assert.equal(docLinks({ symbols: [] }, root, listing, second).links[0].check, 'ok');
});

test('links answers every link or unresolved reference, narrows by file, rule and state; about lists what documents it', async () => {
  const { runQuery } = await import('../src/runtime/query-api.ts');
  const file = (id) => ({ id, label: id, chunk: 'a', language: 'typescript', metrics: { pagerank: 0.1, betweenness: 0, inWeight: 0, outWeight: 0, depth: 0, core: 0, community: null, cycle: null } });
  const D = {
    section: '/repo',
    files: [file('src/a.ts'), file('src/b.ts')],
    symbols: [sym('src/a.ts', 'runQuery')],
    imports: [], calls: [],
    links: [
      { source: 'README.md', line: 3, target: 'src/a.ts', relation: 'references', rule: 'md-path', check: 'ok' },
      { source: 'README.md', line: 4, target: 'src/a.ts#runQuery', relation: 'references', rule: 'md-symbol', check: 'ok' },
      { source: 'docs/x.md', line: 9, target: 'src/b.ts', lines: [1, 2], relation: 'cites', rule: 'md-cite', check: 'stale' },
    ],
    unresolvedLinks: [{ source: 'docs/x.md', line: 2, text: 'src/gone.ts', rule: 'md-path', reason: 'no-such-file' }],
  };
  const all = runQuery(D, 'links');
  assert.equal(all.rows.length, 3);
  assert.match(all.summary, /1 stale: links --stale/);
  assert.deepEqual(runQuery(D, 'links src/a.ts').rows.map((r) => r.loc.line), [3, 4]);
  assert.deepEqual(runQuery(D, 'links --stale').rows.map((r) => [r.target, r.check]), [['src/b.ts', 'stale']]);
  assert.deepEqual(runQuery(D, 'links --rule md-cite').rows.map((r) => r.rule), ['md-cite']);
  const unresolved = runQuery(D, 'links --unresolved');
  assert.deepEqual(unresolved.rows.map((r) => [r.text, r.reason, r.loc.path, r.loc.line]), [['src/gone.ts', 'no-such-file', 'docs/x.md', 2]]);
  assert.equal(unresolved.next.read.input.queries[0].path, '/repo/docs/x.md');
  assert.equal(runQuery(D, 'links --rule md-nope').ok, false);
  // about: the doc lines that link to a file (and to its symbols), or to a symbol.
  const about = runQuery(D, 'about src/a.ts');
  assert.equal(about.stats.documentedBy, 2);
  assert.deepEqual(about.rows.filter((r) => r.role === 'documentedBy').map((r) => `${r.loc.path}:${r.loc.line}`), ['README.md:3', 'README.md:4']);
  assert.equal(runQuery(D, 'about src/a.ts#runQuery').stats.documentedBy, 1);
});

test('diff reports doc references that broke and citations that went stale, only between graphs that read docs', async () => {
  const { diffGraphs } = await import('../src/runtime/diff.ts');
  const before = { files: [], fileEdges: [], links: [{ source: 'a.md', line: 1, target: 'x.ts', lines: [2, 2], relation: 'cites', rule: 'md-cite', check: 'ok' }], unresolvedLinks: [] };
  const after = {
    files: [], fileEdges: [],
    links: [{ ...before.links[0], check: 'stale' }],
    unresolvedLinks: [{ source: 'a.md', line: 4, text: 'src/gone.ts', rule: 'md-path', reason: 'no-such-file' }, { source: 'a.md', line: 5, text: 'parseArgs', rule: 'md-symbol', reason: 'unconfirmed' }],
  };
  const d = diffGraphs(before, after);
  assert.deepEqual([d.summary.docReferencesBroken, d.summary.docCitationsStale], [1, 1]);
  assert.deepEqual(d.review.filter((f) => f.kind === 'doc-link').map((f) => f.files), [['a.md'], ['a.md']]);
  // A graph built before docs were read compares nothing.
  assert.equal(diffGraphs({ files: [], fileEdges: [] }, after).summary.docReferencesBroken, 0);
});

test('an agent link is stored only when its quotes hold, re-anchors when the code moves and goes stale when it changes', async () => {
  const { proposeLink, removeLink, reviewLink, readProposed, recheckProposed, LinkRefused } = await import('../src/runtime/links-proposed.ts');
  const { readFileSync } = await import('node:fs');
  const { root } = tree({
    'README.md': '# CLI\n\nThe `parseArgs` function splits flags from words.\n',
    'src/cli.ts': 'export function parseArgs(argv) {\n  return argv;\n}\n// argv\n',
  });
  const out = tempDir('graph-links-run-');
  writeFileSync(join(out, 'data.json'), JSON.stringify({ section: root, files: [{ id: 'src/cli.ts' }], symbols: [sym('src/cli.ts', 'parseArgs')], links: [] }));
  const link = {
    relation: 'documents', source: 'README.md', target: 'src/cli.ts#parseArgs', claim: 'the README describes parseArgs',
    anchors: [{ path: 'README.md', line: 3, quote: 'The `parseArgs` function splits flags' }, { path: 'src/cli.ts', line: 1, quote: 'export function parseArgs(argv)' }],
    proposedBy: 'agent-a',
  };
  const refused = (input, reason) => assert.throws(() => proposeLink(out, input), (e) => e instanceof LinkRefused && e.reason === reason);
  refused({ ...link, anchors: [{ ...link.anchors[0], line: 1 }, link.anchors[1]] }, 'quote-mismatch');
  refused({ ...link, anchors: [link.anchors[0], { path: 'src/cli.ts', line: 1, quote: 'argv' }] }, 'quote-not-unique');
  refused({ ...link, anchors: [link.anchors[0]] }, 'missing-anchor');
  refused({ ...link, target: 'src/gone.ts' }, 'unknown-endpoint');
  refused({ ...link, anchors: [link.anchors[0], { ...link.anchors[1], line: 40 }] }, 'citation-line');
  assert.deepEqual(readProposed(out), [], 'a refused link stores nothing');
  // The README quote names the target symbol: likely.
  const stored = proposeLink(out, link);
  assert.deepEqual([stored.confidence, stored.check], ['likely', 'ok']);
  assert.equal(proposeLink(out, link).id, stored.id, 'proposing again updates in place');
  assert.equal(readProposed(out).length, 1);
  const data = JSON.parse(readFileSync(join(out, 'data.json'), 'utf8'));
  assert.deepEqual(data.links.map((l) => [l.rule, l.relation, l.target, l.confidence]), [['agent', 'documents', 'src/cli.ts#parseArgs', 'likely']]);
  assert.throws(() => reviewLink(out, stored.id, 'agent-a'), LinkRefused, 'the proposer cannot confirm');
  assert.equal(reviewLink(out, stored.id, 'agent-b').confidence, 'confirmed');
  // Two lines added above the function: the quote is found once elsewhere, the anchor moves, the link stays.
  writeFileSync(join(root, 'src/cli.ts'), '// header\n// more\nexport function parseArgs(argv) {\n  return argv;\n}\n// argv\n');
  const moved = recheckProposed(out, root)[0];
  assert.deepEqual([moved.check, moved.confidence, moved.anchors[1].line], ['ok', 'confirmed', 3]);
  // The quoted code changed: stale, and no longer confirmed.
  writeFileSync(join(root, 'src/cli.ts'), 'export function parseArgv(argv) {\n  return argv;\n}\n');
  const stale = recheckProposed(out, root)[0];
  assert.deepEqual([stale.check, stale.confidence], ['stale', 'likely']);
  removeLink(out, stored.id);
  assert.deepEqual(readProposed(out), []);
});

test('manifests and tsconfigs link the files their fields name; a missing build output lists same-named sources', async () => {
  const { fileLinks } = await import('../src/runtime/links-files.ts');
  const { root, listing } = tree({
    'pkg/package.json': JSON.stringify({ name: 'p', bin: { p: 'bin/p.mjs', q: 'out/cli.js' }, main: 'dist/index.js', exports: { '.': { import: './dist/index.js', types: './dist/index.d.ts' }, './x/*': './dist/x/*.js' }, imports: { '#a': './src/a.ts', '#ext': 'some-package' } }, null, 2),
    'pkg/bin/p.mjs': '',
    // Built: the link stands, and its source is still an open question.
    'pkg/out/cli.js': '',
    'pkg/src/index.ts': '',
    'pkg/src/a.ts': '',
    'crate/Cargo.toml': '[package]\nname = "c"\nbuild = "build.rs"\n\n[lib]\npath = "src/lib.rs"\n\n[[bin]]\nname = "c"\npath = "src/gone.rs"\n',
    'crate/build.rs': '',
    'crate/src/lib.rs': '',
    'tsconfig.json': '{\n  // shared\n  "extends": "./tsconfig.base",\n  "references": [{ "path": "./pkg" }],\n}\n',
    'tsconfig.base.json': '{}',
    'pkg/tsconfig.json': '{}',
  });
  const { links, unresolved } = fileLinks(root, listing);
  assert.deepEqual(links.map((l) => [l.rule, l.source, l.line, l.target]).toSorted(), [
    ['manifest', 'crate/Cargo.toml', 3, 'crate/build.rs'],
    ['manifest', 'crate/Cargo.toml', 6, 'crate/src/lib.rs'],
    ['manifest', 'pkg/package.json', 16, 'pkg/src/a.ts'],
    ['manifest', 'pkg/package.json', 4, 'pkg/bin/p.mjs'],
    ['manifest', 'pkg/package.json', 5, 'pkg/out/cli.js'],
    ['tsconfig', 'tsconfig.json', 3, 'tsconfig.base.json'],
    ['tsconfig', 'tsconfig.json', 4, 'pkg/tsconfig.json'],
  ]);
  // dist/ is not on disk: same-named sources are candidates, never a guessed link; globs and packages are no paths.
  assert.deepEqual(unresolved.map((u) => [u.source, u.text, u.reason, u.candidates ?? []]).toSorted(), [
    ['crate/Cargo.toml', 'src/gone.rs', 'no-such-file', []],
    ['pkg/package.json', './dist/index.d.ts', 'build-output', ['pkg/src/index.ts']],
    ['pkg/package.json', './dist/index.js', 'build-output', ['pkg/src/index.ts']],
    ['pkg/package.json', 'dist/index.js', 'build-output', ['pkg/src/index.ts']],
    ['pkg/package.json', 'out/cli.js', 'build-output', []],
  ]);
});
