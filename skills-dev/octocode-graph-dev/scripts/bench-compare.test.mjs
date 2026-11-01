import { test } from 'node:test';
import assert from 'node:assert/strict';
import { compare, markdown } from './bench-compare.mjs';
import { CASES } from './graph-bench.mjs';
import { fileRecall, missedSites, nonCodeRanges, inRanges, oracleSites } from './lib/oracle.mjs';
import { crosscheck } from './lsp-crosscheck.mjs';

const row = (lang, recall, precision, extra = {}) => ({
  lang,
  ingest: { exit: 0, ms: 100, callLinkRate: 0.5, ...extra.ingest },
  recall: { recall, precision, missed: extra.missed ?? [], extra: [] },
});

test('a precision drop or a new ingest failure is a regression; a recall drop is reported only', () => {
  const rows = compare(
    [row('go', 0.9, 1), row('tsx', 0.6, 1), row('rust', 1, 1)],
    [row('go', 0.95, 0.9), row('tsx', 0.56, 1), row('rust', null, null, { ingest: { exit: 5, error: 'scopeTooBroad' } })],
  );
  const by = Object.fromEntries(rows.map((r) => [r.lang, r]));
  assert.deepEqual(by.go.notes, ['precision down']);
  assert.equal(by.go.regression, true);
  assert.deepEqual(by.tsx.notes, ['recall down']);
  assert.equal(by.tsx.regression, false);
  assert.deepEqual(by.rust.notes, ['ingest failed']);
  assert.equal(by.rust.regression, true);
});

test('a row missing from the second run is named, and newly missed files are listed', () => {
  const rows = compare([row('c', 1, 1), row('java', 0.95, 1)], [row('java', 0.9, 1, { missed: ['a/B.java'] })]);
  assert.deepEqual(rows.find((r) => r.lang === 'c').notes, ['not run']);
  assert.deepEqual(rows.find((r) => r.lang === 'java').newMissed, ['a/B.java']);
  assert.match(markdown(rows), /\| java \| 0\.95 → 0\.9 \| 1 \|.*newly missed a\/B\.java/);
});

test('older runs with sampled missed lists compare metrics but name no newly missed files', () => {
  const old = { lang: 'js', ingest: { exit: 0 }, recall: { recall: 0.25, precision: 1, missedSample: ['x.js'] } };
  const [r] = compare([old], [row('js', 0.25, 1, { missed: ['x.js', 'y.js'] })]);
  assert.deepEqual(r.newMissed, []);
  assert.deepEqual(r.notes, []);
});

test('a row only in the second run is marked new', () => {
  const [r] = compare([], [row('cuda', 0.95, 1)]);
  assert.deepEqual(r.notes, ['new row']);
  assert.equal(r.regression, false);
});

test('every battery case names a distinct repository and a probe', () => {
  assert.equal(new Set(CASES.map((c) => c.repo)).size, CASES.length);
  for (const c of CASES) assert.ok(c.probe && c.ext && c.dir, c.lang);
});

// Oracle fixtures: `files` maps a path to its text; a match is every `probe(` / `probe[` in it.
function oracleOf(files, probe, { definitions = [], exclude = [] } = {}) {
  const matches = [];
  for (const [file, src] of Object.entries(files)) {
    for (const m of src.matchAll(new RegExp(`\\b${probe}\\s*[(<\\[]`, 'g'))) {
      const offset = Buffer.byteLength(src.slice(0, m.index));
      matches.push({ file, line: src.slice(0, m.index).split('\n').length, offset });
    }
  }
  const defs = definitions.map(([file, needle]) => ({ file, startByte: Buffer.byteLength(files[file].slice(0, files[file].indexOf(needle))) }));
  return oracleSites({ matches, definitions: defs, exclude, text: (f) => Buffer.from(files[f]) });
}

test('a declaration-only file is not a miss; a file that also calls the probe is', () => {
  const files = {
    'Monad.scala': 'trait Monad[F[_]] {\n  def tailRecM[A, B](a: A): F[B]\n}\n',
    'Chain.scala': 'object Chain {\n  override def tailRecM[A, B](a: A) = ???\n  def go = tailRecM(1)\n}\n',
  };
  const { sites, dropped } = oracleOf(files, 'tailRecM', { definitions: [['Monad.scala', 'tailRecM'], ['Chain.scala', 'tailRecM']] });
  assert.deepEqual(sites, [{ file: 'Chain.scala', line: 3 }]);
  assert.equal(dropped.definition, 2);
  const r = fileRecall(new Set(sites.map((s) => s.file)), new Set());
  assert.deepEqual(r.missed, ['Chain.scala']);
});

test('excluded globs and hits inside strings or comments are not call sites', () => {
  const files = {
    'vendor/underscore.js': 'isObject(a);\n',
    'perf/perf.js': "suite.add(name, '\\\n  lodash.isObject(object);\\\n  x')\n",
    'src/a.ts': '// isObject(x) here\nconst s = `isObject(${y})`; /* isObject(z) */\nisObject(w); // é isObject(v)\n',
    'm.py': 'def f():\n    """isObject(x)"""\n    return isObject(1)  # isObject(2)\n',
    'lib.rs': "fn g<'a>(x: &'a str) { isObject(x); let c = '\"'; isObject(c) }\n",
  };
  const { sites, dropped } = oracleOf(files, 'isObject', { exclude: ['vendor/**'] });
  assert.deepEqual(sites, [
    { file: 'src/a.ts', line: 3 },
    { file: 'm.py', line: 3 },
    { file: 'lib.rs', line: 1 },
    { file: 'lib.rs', line: 1 },
  ]);
  assert.deepEqual(dropped, { definition: 0, excluded: 1, nonCode: 7 });
});

test('non-code ranges are byte offsets past multibyte text', () => {
  const src = Buffer.from('const é = "ü"; call()');
  const ranges = nonCodeRanges(src, 'js');
  assert.equal(ranges.length, 1);
  assert.equal(inRanges(ranges, src.indexOf('call')), false);
  assert.equal(inRanges(ranges, src.indexOf('ü')), true);
});

test('a missed file carries its unresolved row for the probe at that line', () => {
  const sites = [{ file: 'Angle.tsx', line: 40 }, { file: 'Angle.tsx', line: 52 }, { file: 'found.ts', line: 3 }];
  const unresolved = [
    { file: 'Angle.tsx', line: 40, targetName: 'scene.mutateElement', reason: 'member-unknown-receiver-in-repo' },
    { file: 'Angle.tsx', line: 40, targetName: 'scene.getElement', reason: 'member-unknown-receiver-in-repo' },
  ];
  assert.deepEqual(missedSites({ missed: ['Angle.tsx'], sites, unresolved, probe: 'mutateElement' }), [
    { file: 'Angle.tsx', line: 40, reason: 'member-unknown-receiver-in-repo', targetName: 'scene.mutateElement' },
    { file: 'Angle.tsx', line: 52, reason: null, targetName: null },
  ]);
});

test('lsp-crosscheck counts recovered LSP-only edges, kept shared edges and unseen probed edges', () => {
  const peer = [
    { source: 'a/x#f', target: 'a/y#g', via: 'lsp' },
    { source: 'a/x#f', target: 'b/z#h', via: 'lsp' },
    { source: 'a/x#k', target: 'a/y#g', via: 'both' },
    { source: 'a/x#k', target: 'a/y#m', via: 'both' },
    { source: 'a/q#n', target: 'a/y#g', via: 'graph' },
  ];
  const edges = [{ from: 'a/x#f', to: 'a/y#g' }, { from: 'a/x#k', to: 'a/y#g' }, { from: 'a/x#k', to: 'a/w#new' }];
  const r = crosscheck(edges, peer);
  assert.deepEqual(r.summary, {
    edges: 3,
    lspOnly: { total: 2, sameCrate: 1, recovered: 1 },
    both: { total: 2, sameCrate: 2, stillLinked: 1 },
    probedGraphEdgesNotInLsp: 1,
  });
  assert.deepEqual(r.lostBoth.map((c) => c.target), ['a/y#m']);
});
