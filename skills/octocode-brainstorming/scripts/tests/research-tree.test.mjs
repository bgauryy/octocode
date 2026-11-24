import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DIRECTIONS, SURFACES, coverage, planTree } from '../research-tree.mjs';

const script = fileURLToPath(new URL('../research-tree.mjs', import.meta.url));
const claim = (id, question, extra = {}) => ({
  id, question, claim: `claim ${id}`, rival: `rival ${id}`, refutedBy: `refuter ${id}`, at: `plan:${id}`,
  queries: [{ direction: 'supports', surface: 'web', text: `for ${id}` }, { direction: 'refutes', surface: 'github', text: `against ${id}` }],
  ...extra,
});
const ledgerRow = (decides, direction, status = 'verified', extra = {}) => ({ decides, direction, url: `https://s.test/${decides}/${direction}/${status}`, claimStatus: 'proven', verification: { status }, ...extra });

function run(files, args) {
  const dir = mkdtempSync(join(tmpdir(), 'research-tree-'));
  try {
    const paths = Object.fromEntries(Object.entries(files).map(([name, rows]) => {
      const path = join(dir, name);
      writeFileSync(path, rows.map((r) => (typeof r === 'string' ? r : JSON.stringify(r))).join('\n'));
      return [name, path];
    }));
    const r = spawnSync(process.execPath, [script, ...args.map((a) => paths[a] || a)], { encoding: 'utf8' });
    return { status: r.status, json: JSON.parse(r.stdout) };
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

test('the plan groups claims by sub-question and by worker, with each worker\'s surfaces', () => {
  const plan = [claim('C1', 'Q latency', { worker: 'W1' }), claim('C2', 'Q latency', { worker: 'W2' }), claim('C3', 'Q cost')];
  const tree = planTree(plan);
  assert.equal(tree.claims, plan.length);
  assert.deepEqual(tree.questions.map((q) => [q.question, q.claims]), [['Q latency', ['C1', 'C2']], ['Q cost', ['C3']]]);
  assert.deepEqual(tree.workers.map((w) => w.worker), ['W1', 'W2', 'unassigned']);
  assert.deepEqual(tree.workers[0].surfaces, ['web', 'github']);
  assert.equal(tree.workers.reduce((n, w) => n + w.claims.length, 0), plan.length, 'every claim has exactly one worker');
});

test('coverage counts only verified rows, names each claim\'s state, and lists what to deepen', () => {
  const plan = [claim('C1', 'Q'), claim('C2', 'Q'), claim('C3', 'Q'), claim('C4', 'Q')];
  const rows = [
    ledgerRow('C1', 'supports'), ledgerRow('C1', 'refutes', 'verified-manual'),
    ledgerRow('C2', 'supports'), ledgerRow('C2', 'refutes', 'quote-not-found'),
    ledgerRow('C3', 'refutes', 'verified', { claimStatus: 'contradicted' }), ledgerRow('C3', 'refutes', 'verified', { url: 'https://s.test/other', claimStatus: 'contradicted' }),
    ledgerRow('C9', 'supports'),
  ];
  const r = coverage(plan, rows);
  const byId = Object.fromEntries(r.coverage.map((c) => [c.id, c]));
  assert.equal(byId.C1.state, 'bothSides');
  assert.equal(byId.C2.state, 'supportOnly', 'an unverified refuting row does not count');
  assert.equal(byId.C2.unverified, 1);
  assert.equal(byId.C3.state, 'refuteOnly');
  assert.equal(byId.C3.sources, 2);
  assert.equal(byId.C4.state, 'open');
  assert.deepEqual(r.deepen.map((d) => d.id), ['C2', 'C3', 'C4']);
  assert.equal(Object.values(r.states).reduce((a, b) => a + b, 0), plan.length);
  assert.deepEqual(r.unplanned, [{ decides: 'C9', rows: 1 }]);
  assert.deepEqual(r.conflicting, [], 'one status per claim');
  const mixed = coverage([claim('C1', 'Q')], [ledgerRow('C1', 'supports'), ledgerRow('C1', 'refutes', 'verified', { claimStatus: 'unresolved' })]);
  assert.deepEqual(mixed.conflicting, [{ id: 'C1', claimStatus: ['proven', 'unresolved'] }]);
});

test('a claim without a rival, a refuting observation or both query directions is refused with its line', () => {
  const ok = claim('C1', 'Q');
  const cases = [
    [{ ...ok, rival: '' }, /"rival" must be a non-empty string/],
    [{ ...ok, refutedBy: undefined }, /"refutedBy" must be a non-empty string/],
    [{ ...ok, queries: ok.queries.filter((q) => q.direction === 'supports') }, /has no "refutes" query; plan both directions/],
    [{ ...ok, queries: [{ ...ok.queries[0], surface: 'blogs' }, ok.queries[1]] }, new RegExp(`surface must be one of ${SURFACES.join(', ')}`)],
    [{ ...ok, queries: [{ ...ok.queries[0], direction: 'maybe' }, ok.queries[1]] }, new RegExp(`direction must be one of ${DIRECTIONS.join(', ')}`)],
  ];
  for (const [row, pattern] of cases) {
    const { at, ...clean } = row;
    const bad = run({ 'plan.jsonl': [clean] }, ['--plan', 'plan.jsonl']);
    assert.equal(bad.status, 2, JSON.stringify(bad.json));
    assert.equal(bad.json.error.code, 'invalidPlan');
    assert.match(bad.json.error.message, /plan\.jsonl:1:/);
    assert.match(bad.json.error.message, pattern);
  }
  const { at, ...clean } = ok;
  const dup = run({ 'plan.jsonl': [clean, clean] }, ['--plan', 'plan.jsonl']);
  assert.match(dup.json.error.message, /plan\.jsonl:2: id C1 repeats/);
});

test('the CLI joins several verified ledgers to the plan; an empty plan fails', () => {
  const { at, ...c1 } = claim('C1', 'Q', { worker: 'W1' });
  const r = run({ 'plan.jsonl': [c1], 'a.jsonl': [ledgerRow('C1', 'supports')], 'b.jsonl': ['', ledgerRow('C1', 'refutes')] },
    ['--plan', 'plan.jsonl', '--verified', 'a.jsonl', '--verified', 'b.jsonl']);
  assert.equal(r.status, 0, JSON.stringify(r.json));
  assert.equal(r.json.coverage[0].state, 'bothSides');
  assert.deepEqual(r.json.deepen, []);
  const planOnly = run({ 'plan.jsonl': [c1] }, ['--plan', 'plan.jsonl']);
  assert.equal(planOnly.json.coverage, undefined, 'no ledger, no coverage');
  const empty = run({ 'plan.jsonl': [''] }, ['--plan', 'plan.jsonl']);
  assert.equal(empty.status, 1);
  assert.equal(empty.json.error.code, 'noClaims');
  const missing = run({}, ['--verified', 'x.jsonl']);
  assert.equal(missing.status, 2);
});
