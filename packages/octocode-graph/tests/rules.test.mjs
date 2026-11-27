// Dependency rules over a saved graph in dependency-cruiser's vocabulary (rules.ts): forbidden, allowed and required
// sets; path captures; reachable true and false; circular with via and viaOnly; dependency types; a load per rule;
// a misspelled path or package is an error; a baseline marks known violations. Every graph here is built by hand.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { applyBaseline, baselineOf, checkRules, parseRules, RulesError } from '../src/runtime/rules.ts';

const file = (id) => ({ id, label: id.split('/').pop(), chunk: 'app', language: 'typescript' });
const imp = (source, target, line, kinds) => ({ source, target, line, lines: [line], confidence: 'high', resolution: kinds, test: false });
const STATIC = ['static-import'];
const TYPE = ['type-import'];

// index → browser/view → shared/io → pkg:node:fs; browser/legacy → pkg:node:fs; view ⇢ shared/types (type) → fs
// (type); index → shared/util ⇢ shared/types (type); bc/alpha/a → bc/alpha/a2, bc/beta/b; cycle x → y → z → x;
// lib p → q ⇢ p (type); controllers: admin → base, user → util; dead.ts imported by nothing.
function graph() {
  const ids = [
    'src/index.ts', 'src/browser/view.ts', 'src/browser/legacy.ts', 'src/shared/io.ts', 'src/shared/util.ts',
    'src/shared/types.ts', 'src/bc/alpha/a.ts', 'src/bc/alpha/a2.ts', 'src/bc/beta/b.ts', 'src/cycle/x.ts',
    'src/cycle/y.ts', 'src/cycle/z.ts', 'src/lib/p.ts', 'src/lib/q.ts', 'src/controllers/admin-controller.ts',
    'src/controllers/base-controller.ts', 'src/controllers/user-controller.ts', 'src/dead.ts',
  ];
  return {
    section: '/repo',
    files: ids.map(file),
    imports: [
      imp('src/index.ts', 'src/browser/view.ts', 1, STATIC),
      imp('src/index.ts', 'src/browser/legacy.ts', 2, STATIC),
      imp('src/index.ts', 'src/shared/util.ts', 3, STATIC),
      imp('src/index.ts', 'src/cycle/x.ts', 4, STATIC),
      imp('src/index.ts', 'src/lib/p.ts', 5, STATIC),
      imp('src/index.ts', 'src/bc/alpha/a.ts', 6, STATIC),
      imp('src/index.ts', 'src/controllers/admin-controller.ts', 7, STATIC),
      imp('src/index.ts', 'src/controllers/user-controller.ts', 8, STATIC),
      imp('src/browser/view.ts', 'src/shared/io.ts', 2, STATIC),
      imp('src/browser/view.ts', 'src/shared/types.ts', 3, TYPE),
      imp('src/shared/util.ts', 'src/shared/types.ts', 1, TYPE),
      imp('src/bc/alpha/a.ts', 'src/bc/alpha/a2.ts', 1, STATIC),
      imp('src/bc/alpha/a.ts', 'src/bc/beta/b.ts', 2, STATIC),
      imp('src/cycle/x.ts', 'src/cycle/y.ts', 1, STATIC),
      imp('src/cycle/y.ts', 'src/cycle/z.ts', 1, STATIC),
      imp('src/cycle/z.ts', 'src/cycle/x.ts', 1, STATIC),
      imp('src/lib/p.ts', 'src/lib/q.ts', 1, STATIC),
      imp('src/lib/q.ts', 'src/lib/p.ts', 1, TYPE),
      imp('src/controllers/admin-controller.ts', 'src/controllers/base-controller.ts', 1, STATIC),
      imp('src/controllers/user-controller.ts', 'src/shared/util.ts', 1, STATIC),
    ],
    symbols: [],
    calls: [],
    externals: [
      {
        id: 'pkg:node:fs',
        count: 3,
        files: ['src/shared/io.ts', 'src/browser/legacy.ts', 'src/shared/types.ts'],
        kinds: {
          'src/shared/io.ts': ['external-builtin', 'static-import'],
          'src/browser/legacy.ts': ['external-builtin', 'static-import'],
          'src/shared/types.ts': ['external-builtin', 'type-import'],
        },
      },
    ],
  };
}

const D = graph();
const run = (rules) => checkRules(D, parseRules(rules));
const pairs = (vs) => vs.map((v) => `${v.from} → ${v.to}`).toSorted();
const fails = (rules, pattern) =>
  assert.throws(() => run(rules), (e) => e instanceof RulesError && pattern.test(e.problems.join('\n')), String(pattern));

test('a reachable rule reports each from-module that reaches a target, with the chain and each hop line and kinds', () => {
  const vs = run({ forbidden: [{ name: 'no-fs-in-browser', severity: 'error', from: { path: '^src/browser/' }, to: { path: '^pkg:node:fs$', reachable: true } }] });
  assert.deepEqual(pairs(vs), ['src/browser/legacy.ts → pkg:node:fs', 'src/browser/view.ts → pkg:node:fs']);
  const view = vs.find((v) => v.from === 'src/browser/view.ts');
  assert.equal(view.severity, 'error');
  assert.equal(view.kind, 'reachable');
  assert.deepEqual(view.chain.map((h) => h.id), ['src/browser/view.ts', 'src/shared/io.ts', 'pkg:node:fs']);
  assert.equal(view.chain[0].line, 2);
  assert.deepEqual(view.chain[0].kinds, ['static-import']);
  assert.deepEqual(view.chain[1].kinds, ['external-builtin', 'static-import']);
});

test('the load decides the walk: a type-only import never reaches at run time, and does over every edge', () => {
  const rule = (load) => ({ forbidden: [{ name: 'util-no-fs', from: { path: 'shared/util\\.ts$' }, to: { path: '^pkg:node:fs$', reachable: true }, ...(load && { load }) }] });
  assert.deepEqual(run(rule()), [], 'runtime by default: util only imports types');
  assert.deepEqual(pairs(run(rule('all'))), ['src/shared/util.ts → pkg:node:fs']);
  const eager = run({ forbidden: [{ name: 'x', from: { path: '^src/index\\.ts$' }, to: { path: '^src/cycle/z\\.ts$', reachable: true }, load: 'eager' }] });
  assert.deepEqual(pairs(eager), ['src/index.ts → src/cycle/z.ts']);
});

test('a direct rule takes $1 from from.path into to.path and to.pathNot, and the severity defaults to warn', () => {
  const vs = run({
    forbidden: [{ name: 'no-inter-bc', from: { path: '^src/bc/([^/]+)/.+' }, to: { path: '^src/bc/([^/]+)/.+', pathNot: '^src/bc/$1/.+' } }],
  });
  assert.deepEqual(pairs(vs), ['src/bc/alpha/a.ts → src/bc/beta/b.ts']);
  assert.equal(vs[0].severity, 'warn');
  assert.equal(vs[0].kind, 'dependency');
  assert.deepEqual(vs[0].chain.map((h) => [h.id, h.line ?? null]), [['src/bc/alpha/a.ts', 2], ['src/bc/beta/b.ts', null]]);
  // A path array matches when any item does.
  assert.equal(run({ forbidden: [{ name: 'arr', from: { path: ['^src/nothing-here', '^src/bc/alpha/a\\.ts$'] }, to: { path: 'bc/beta' } }] }).length, 1);
});

test('reachable false lists every to-module no from-module reaches at run time', () => {
  const vs = run({ forbidden: [{ name: 'no-dead', severity: 'info', from: { path: '^src/index\\.ts$' }, to: { path: '^src/', pathNot: '^src/index\\.ts$', reachable: false } }] });
  // types.ts is reached only through import type; base-controller through admin; dead.ts by nothing.
  assert.deepEqual(vs.map((v) => v.to).toSorted(), ['src/dead.ts', 'src/shared/types.ts']);
  assert.ok(vs.every((v) => v.kind === 'unreachable' && v.from === v.to));
});

test('circular rules report each dependency inside a cycle with the ring; via and viaOnly test the ring members', () => {
  const cycles = run({ forbidden: [{ name: 'no-cycle', severity: 'error', from: {}, to: { circular: true } }] });
  assert.deepEqual(pairs(cycles), ['src/cycle/x.ts → src/cycle/y.ts', 'src/cycle/y.ts → src/cycle/z.ts', 'src/cycle/z.ts → src/cycle/x.ts']);
  const x = cycles.find((v) => v.from === 'src/cycle/x.ts');
  assert.deepEqual(x.chain.map((h) => h.id), ['src/cycle/x.ts', 'src/cycle/y.ts', 'src/cycle/z.ts', 'src/cycle/x.ts']);
  assert.deepEqual(x.cycle, ['src/cycle/y.ts', 'src/cycle/z.ts', 'src/cycle/x.ts']);
  const all = run({ forbidden: [{ name: 'no-cycle', from: {}, to: { circular: true }, load: 'all' }] });
  assert.ok(pairs(all).includes('src/lib/q.ts → src/lib/p.ts'), 'over every edge a type import closes the lib ring');
  const via = (v) => run({ forbidden: [{ name: 'c', from: {}, to: { circular: true, ...v } }] }).length;
  assert.equal(via({ via: { path: 'z\\.ts$' } }), 3);
  assert.equal(via({ via: { path: '^src/cycle/', pathNot: '.' } }), 0);
  assert.equal(via({ viaOnly: { path: '^src/cycle/' } }), 3);
  assert.equal(via({ viaOnly: { path: '^src/cycle/', pathNot: 'z\\.ts$' } }), 0);
});

test('dependencyTypes and dependencyTypesNot select pairs by their import kinds', () => {
  const typed = run({ forbidden: [{ name: 't', from: {}, to: { path: '^src/shared/', dependencyTypes: ['type-import'] }, load: 'all' }] });
  assert.deepEqual(pairs(typed), ['src/browser/view.ts → src/shared/types.ts', 'src/shared/util.ts → src/shared/types.ts']);
  const not = run({ forbidden: [{ name: 'n', from: { path: '^src/browser/' }, to: { path: '^src/shared/', dependencyTypesNot: ['type-import'] }, load: 'all' }] });
  assert.deepEqual(pairs(not), ['src/browser/view.ts → src/shared/io.ts']);
});

test('allowed: a dependency no allowed rule matches is not-in-allowed, at allowedSeverity (default warn)', () => {
  const vs = run({ allowed: [{ from: { path: '^src/' }, to: { path: '^src/' } }] });
  assert.deepEqual(pairs(vs), ['src/browser/legacy.ts → pkg:node:fs', 'src/shared/io.ts → pkg:node:fs']);
  assert.ok(vs.every((v) => v.rule === 'not-in-allowed' && v.severity === 'warn' && v.type === 'allowed'));
  const strict = run({ allowed: [{ from: { path: '^src/' }, to: { path: '^src/' } }], allowedSeverity: 'error' });
  assert.ok(strict.every((v) => v.severity === 'error'));
});

test('required: each module matching `module` must depend on a module matching to.path', () => {
  const vs = run({
    required: [{ name: 'controllers-use-base', severity: 'error', module: { path: '-controller\\.ts$', pathNot: 'base-controller\\.ts$' }, to: { path: '^src/controllers/base-controller\\.ts$' } }],
  });
  assert.deepEqual(vs.map((v) => [v.from, v.kind]), [['src/controllers/user-controller.ts', 'required']]);
  const reach = run({ required: [{ name: 'index-reaches-base', module: { path: '^src/index\\.ts$' }, to: { path: 'base-controller', reachable: true } }] });
  assert.deepEqual(reach, [], 'index reaches base-controller through admin-controller');
});

test('a rules file that is malformed, ambiguous or names nothing in the graph is an error, never a pass', () => {
  fails({ forbidden: [{ name: 'x', from: {}, to: {}, colour: 1 }] }, /forbidden\[0\]: unknown property colour/);
  fails({ forbidden: [{ name: 'x', severity: 'fatal', from: {}, to: {} }] }, /forbidden\[0\]\.severity must be one of error, warn, info/);
  fails({ forbidden: [{ from: {}, to: {} }] }, /forbidden\[0\]: missing name/);
  fails({ forbidden: [{ name: 'x', from: { path: '(' }, to: {} }] }, /forbidden\[0\]\.from\.path: invalid regular expression/);
  fails({ forbidden: [{ name: 'x', from: { path: '^src/(a)' }, to: { path: '$2' } }] }, /\$2.*1 group/);
  fails({ forbidden: [{ name: 'x', from: {}, to: { reachable: true, circular: true } }] }, /reachable and circular/);
  fails({ forbidden: [{ name: 'x', from: {}, to: { via: { path: 'a' } } }] }, /via and viaOnly need circular/);
  fails({ forbidden: [{ name: 'x', from: {}, to: {} }, { name: 'x', from: {}, to: {} }] }, /rule name x is used twice/);
  fails({ forbidden: [{ name: 'typo', from: { path: '^src/browsr/' }, to: { path: '^pkg:node:fs$', reachable: true } }] }, /forbidden\[0\]\.from\.path \^src\/browsr\/ matches no file or package/);
  fails({ forbidden: [{ name: 'typo', from: {}, to: { path: '^pkg:node:fss$' } }] }, /matches no file or package/);
  assert.throws(() => run({ forbidden: [{ name: 'typo', from: {}, to: { path: '^pkg:node:fss$' } }] }), (e) => e.candidates.includes('pkg:node:fs'));
});

test('a baseline marks its violations known and lists the ones fixed since', () => {
  const vs = run({ forbidden: [{ name: 'no-fs-in-browser', severity: 'error', from: { path: '^src/browser/' }, to: { path: '^pkg:node:fs$', reachable: true } }] });
  const baseline = baselineOf(vs.filter((v) => v.from === 'src/browser/legacy.ts'));
  assert.deepEqual(baseline.violations, [{ rule: 'no-fs-in-browser', from: 'src/browser/legacy.ts', to: 'pkg:node:fs' }]);
  const gone = { rule: 'no-fs-in-browser', from: 'src/browser/old.ts', to: 'pkg:node:fs' };
  const { violations, fixed } = applyBaseline(vs, { violations: [...baseline.violations, gone] });
  assert.deepEqual(violations.filter((v) => v.known).map((v) => v.from), ['src/browser/legacy.ts']);
  assert.deepEqual(fixed, [gone]);
});

test('the rules schema describes exactly the fields the loader accepts', () => {
  const schema = JSON.parse(readFileSync(new URL('../src/scheme/rules.json', import.meta.url), 'utf8'));
  assert.deepEqual(Object.keys(schema.properties).toSorted(), ['$schema', 'allowed', 'allowedSeverity', 'forbidden', 'required']);
  assert.deepEqual(schema.$defs.severity.enum, ['error', 'warn', 'info']);
  assert.deepEqual(schema.$defs.load.enum, ['runtime', 'eager', 'all']);
  // The loader validates against this schema: a field the schema lacks is refused.
  fails({ forbidden: [{ name: 'x', from: { orphan: true }, to: {} }] }, /forbidden\[0\]\.from: unknown property orphan/);
});
