import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { isAbsolute, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, beforeEach, test } from 'node:test';

const script = fileURLToPath(new URL('../corpus-find.mjs', import.meta.url));
let sessionDir;
beforeEach(() => {
  sessionDir = mkdtempSync(join(tmpdir(), 'octocode corpus '));
  mkdirSync(join(sessionDir, 'graph'));
  writeFileSync(join(sessionDir, 'graph/site-graph.json'), JSON.stringify({
    pages: Array.from({ length: 7 }, (_, index) => ({ pageId: `page-${index}`, title: `Needle entry ${index}`, url: `https://example.test/${index}` })),
    edges: [],
  }));
});
afterEach(() => rmSync(sessionDir, { recursive: true, force: true }));

function run(args) {
  return spawnSync(process.execPath, [script, '--session-dir', sessionDir, '--query', 'Needle', ...args], { encoding: 'utf8' });
}
function readSuccess(result) {
  assert.equal(result.status, 0, result.stderr);
  const json = JSON.parse(result.stdout);
  assert.equal(json.ok, true);
  return json;
}

test('limit-one executable continuations cover all seven ranked matches exactly once', () => {
  const all = readSuccess(run(['--limit', '100']));
  assert.equal(all.matches.length, 7);
  let page = readSuccess(run(['--limit', '1']));
  const matches = [], seenCalls = new Set();
  for (let index = 0; index < 7; index += 1) {
    assert.equal(page.pagination.totalMatches, 7);
    assert.equal(page.pagination.offset, index);
    assert.equal(page.pagination.returnedMatches, 1);
    assert.equal(page.pagination.remainingMatches, 6 - index);
    assert.equal(page.isPartial, index < 6);
    assert.equal(page.completeness, index < 6 ? 'partial' : 'complete');
    matches.push(...page.matches);
    if (index === 6) { assert.equal(page.next, null); break; }
    const next = page.next.page;
    assert.ok(isAbsolute(next.command));
    assert.ok(isAbsolute(next.args[0]));
    assert.equal(next.args[0], script);
    const key = JSON.stringify(next);
    assert.ok(!seenCalls.has(key), 'continuation must advance');
    seenCalls.add(key);
    // Execute the returned command/argv unchanged from an unrelated cwd.
    page = readSuccess(spawnSync(next.command, next.args, { cwd: tmpdir(), encoding: 'utf8' }));
  }
  assert.deepEqual(matches, all.matches);
  assert.equal(new Set(matches.map(match => match.pageId)).size, 7);
  assert.ok(Array.isArray(all.suggestedFiles));
});

test('an empty query result is explicitly complete without a continuation', () => {
  const result = spawnSync(process.execPath, [script, '--session-dir', sessionDir, '--query', 'absent'], { encoding: 'utf8' });
  const page = readSuccess(result);
  assert.deepEqual(page.matches, []);
  assert.equal(page.isPartial, false);
  assert.equal(page.completeness, 'complete');
  assert.equal(page.pagination.totalMatches, 0);
  assert.equal(page.pagination.remainingMatches, 0);
  assert.equal(page.next, null);
});

test('offset at or beyond the last match has an explicit empty terminal result', () => {
  for (const offset of ['7', '8']) {
    const page = readSuccess(run(['--limit', '1', '--offset', offset]));
    assert.deepEqual(page.matches, []);
    assert.equal(page.pagination.offset, Number(offset));
    assert.equal(page.pagination.totalMatches, 7);
    assert.equal(page.pagination.returnedMatches, 0);
    assert.equal(page.pagination.remainingMatches, 0);
    assert.equal(page.isPartial, false);
    assert.equal(page.completeness, 'complete');
    assert.equal(page.next, null);
  }
});

test('invalid bounds and malformed options fail explicitly before search', () => {
  for (const args of [
    ['--limit', '0'], ['--limit', '-1'], ['--limit', '1.5'], ['--limit', 'NaN'],
    ['--limit', 'Infinity'], ['--limit', '9007199254740992'], ['--limit'],
    ['--offset', '-1'], ['--offset', '1.5'], ['--offset', 'NaN'],
    ['--offset', '9007199254740992'], ['--unknown', '1'],
    ['--limit', '1', '--limit', '2'],
  ]) {
    const result = run(args);
    assert.equal(result.status, 2, `arguments ${JSON.stringify(args)}: ${result.stdout}`);
    const json = JSON.parse(result.stdout);
    assert.equal(json.ok, false);
    assert.equal(json.error.code, 'invalidArguments');
    assert.ok(json.error.message.length > 0);
  }
});

test('body-text scan finds pages whose metadata lacks the terms and damps boilerplate', () => {
  mkdirSync(join(sessionDir, 'text'));
  // page-9 exists only in sources.jsonl (plain-text payload, no graph entry);
  // its body carries the rare term. Pages 0..6 share boilerplate nav text.
  const rows = [];
  for (let index = 0; index < 7; index += 1) {
    const rel = `text/page-${index}.clean.part-001.md`;
    writeFileSync(join(sessionDir, rel), 'needle nav sidebar boilerplate needle');
    rows.push(JSON.stringify({ pageId: `page-${index}`, url: `https://example.test/${index}`, textParts: [rel] }));
  }
  const rareRel = 'text/page-9.clean.part-001.md';
  writeFileSync(join(sessionDir, rareRel), 'plain text payload with needle and the rare zebra term');
  rows.push(JSON.stringify({ pageId: 'page-9', url: 'https://example.test/llms.txt', textParts: [rareRel] }));
  writeFileSync(join(sessionDir, 'sources.jsonl'), `${rows.join('\n')}\n`);

  const zebra = readSuccess(spawnSync(process.execPath, [script, '--session-dir', sessionDir, '--query', 'zebra'], { encoding: 'utf8' }));
  assert.equal(zebra.matches.length, 1, JSON.stringify(zebra.matches));
  assert.equal(zebra.matches[0].pageId, 'page-9');
  assert.deepEqual(zebra.suggestedFiles, [rareRel]);

  // Rare term must outrank the boilerplate term shared by every page.
  const mixed = readSuccess(spawnSync(process.execPath, [script, '--session-dir', sessionDir, '--query', 'zebra needle'], { encoding: 'utf8' }));
  assert.equal(mixed.matches[0].pageId, 'page-9', JSON.stringify(mixed.matches.slice(0, 3)));

  // suggestedFiles are deduplicated.
  const needle = readSuccess(spawnSync(process.execPath, [script, '--session-dir', sessionDir, '--query', 'needle', '--limit', '20'], { encoding: 'utf8' }));
  assert.equal(new Set(needle.suggestedFiles).size, needle.suggestedFiles.length);
});
