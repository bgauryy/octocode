import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DEFAULT_MIN_SIMILARITY, ORIGINS, poolOptions, similarity, words } from '../pool-options.mjs';

const script = fileURLToPath(new URL('../pool-options.mjs', import.meta.url));
const row = (id, pass, option, extra = {}) => ({ id, pass, origin: 'yours', option, at: `t:${id}`, ...extra });

function run(lines, args = []) {
  const dir = mkdtempSync(join(tmpdir(), 'pool-options-'));
  try {
    const file = join(dir, 'options.jsonl');
    writeFileSync(file, lines.map((l) => (typeof l === 'string' ? l : JSON.stringify(l))).join('\n'));
    const r = spawnSync(process.execPath, [script, '--options', file, ...args], { encoding: 'utf8' });
    return { status: r.status, json: JSON.parse(r.stdout) };
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

test('a pass counts only the distinct options it introduced, and saturation is a last pass that adds none', () => {
  const first = [row('A1', 'incentives', 'Usage-based pricing'), row('A2', 'incentives', 'Seat licences'), row('A3', 'incentives', 'Free tier')];
  const second = [row('B1', 'inversion', 'Per-seat licence', { sameAs: 'A2' }), row('B2', 'inversion', 'No pricing change', { origin: 'user' })];
  const growing = poolOptions([...first, ...second]);
  assert.equal(growing.options, first.length + second.length);
  assert.equal(growing.distinct, growing.options - 1, 'one sameAs merges two rows into one option');
  assert.deepEqual(growing.passes.map((p) => [p.pass, p.new, p.repeats]), [['incentives', first.length, 0], ['inversion', 1, 1]]);
  for (const p of growing.passes) assert.equal(p.new + p.repeats, p.options);
  assert.equal(growing.saturated, false);
  const merged = growing.clusters.find((c) => c.id === 'A2');
  assert.deepEqual(merged.members, ['A2', 'B1']);
  assert.deepEqual(merged.passes, ['incentives', 'inversion']);
  assert.equal(growing.origins.user, 1);
  assert.deepEqual(Object.keys(growing.origins), ORIGINS);

  const done = poolOptions([...first, ...second, row('C1', 'base rates', 'Free tier again', { sameAs: 'A3' })]);
  assert.equal(done.saturated, true, 'the last pass only repeated an earlier option');
  assert.equal(poolOptions(first).saturated, false, 'one pass cannot show saturation');
});

test('a sameAs chain or a forward link still roots the cluster at its earliest row', () => {
  const r = poolOptions([row('A1', 'p1', 'x one', { sameAs: 'B1' }), row('B1', 'p2', 'x two', { sameAs: 'C1' }), row('C1', 'p3', 'x three')]);
  assert.equal(r.distinct, 1);
  assert.equal(r.clusters[0].id, 'A1');
  assert.deepEqual(r.passes.map((p) => p.new), [1, 0, 0]);
});

test('word-overlap candidates are listed for review, highest first, and disappear once merged', () => {
  const rows = [
    row('A1', 'p1', 'Adopt Kafka for event streaming', { claim: 'Kafka keeps ordering per partition' }),
    row('B1', 'p2', 'Kafka event streaming', { claim: 'ordering per partition with Kafka' }),
    row('B2', 'p2', 'Nightly batch export'),
  ];
  const open = poolOptions(rows);
  assert.equal(open.candidates.length, 1);
  assert.deepEqual([open.candidates[0].a, open.candidates[0].b], ['A1', 'B1']);
  assert.ok(open.candidates[0].similarity >= DEFAULT_MIN_SIMILARITY);
  const resolved = poolOptions([rows[0], { ...rows[1], sameAs: 'A1' }, rows[2]]);
  assert.deepEqual(resolved.candidates, []);
  assert.equal(poolOptions(rows, { minSimilarity: 1 }).candidates.length, 0, 'the threshold is the caller\'s');
});

test('words drops case, accents, punctuation and stop words; similarity is symmetric and bounded', () => {
  assert.deepEqual([...words('The Café-based API, for USERS!')], ['cafe', 'based', 'api', 'users']);
  const [a, b] = [words('queue with retries and backoff'), words('backoff retries queue')];
  assert.equal(similarity(a, b), similarity(b, a));
  assert.equal(similarity(a, a), 1);
  assert.equal(similarity(new Set(), new Set()), 0);
});

test('the CLI prints the pool, refuses a malformed row with its line, and fails an empty file', () => {
  const ok = run([{ id: 'A1', pass: 'p', origin: 'user', option: 'Keep the plan' }, '', { id: 'A2', pass: 'p', origin: 'found', option: 'Switch vendor' }]);
  assert.equal(ok.status, 0);
  assert.equal(ok.json.ok, true);
  assert.equal(ok.json.distinct, 2);
  for (const [lines, pattern] of [
    [[{ id: 'A1', pass: 'p', origin: 'mine', option: 'x' }], /options\.jsonl:1: "origin" must be one of user, yours, found/],
    [[{ id: 'A1', pass: 'p', origin: 'user', option: 'x' }, { id: 'A1', pass: 'p', origin: 'user', option: 'y' }], /options\.jsonl:2: id A1 repeats/],
    [[{ id: 'A1', pass: 'p', origin: 'user', option: 'x', sameAs: 'Z9' }], /sameAs Z9 is not an id/],
    [['{not json'], /options\.jsonl:1: not a JSON object/],
    [[{ id: 'A1', pass: '', origin: 'user', option: 'x' }], /"pass" must be a non-empty string/],
  ]) {
    const bad = run(lines);
    assert.equal(bad.status, 2, JSON.stringify(bad.json));
    assert.equal(bad.json.error.code, 'invalidOptions');
    assert.match(bad.json.error.message, pattern);
  }
  const empty = run(['', '']);
  assert.equal(empty.status, 1);
  assert.equal(empty.json.error.code, 'noOptions');
  const flag = run([{ id: 'A1', pass: 'p', origin: 'user', option: 'x' }], ['--min-similarity', '2']);
  assert.equal(flag.status, 2);
});
