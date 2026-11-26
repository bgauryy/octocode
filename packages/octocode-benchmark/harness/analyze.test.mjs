import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { RATIO_METRICS, analyze, failureCause, fieldChars, fitPrices, pairedRatio, pipeline, readMode, replyFailed, shellCall, tables } from './analyze.mjs';
import { HARNESS, codePoints, loadManifest, passPath, runPath, seeded } from './lib.mjs';
import { roundIn, scratch, storeRun } from './testing.mjs';

const sum = values => values.reduce((total, value) => total + value, 0);

test('a paired ratio is the geometric mean, sits inside its interval, and is fixed by the seed', () => {
  const pairs = [[2, 1], [3, 1], [1, 2], [5, 4], [9, 4], [1, 1], [0, 3]];
  const stat = pairedRatio(pairs, seeded('s'));
  const usable = pairs.filter(([a, b]) => a > 0 && b > 0);
  assert.equal(stat.n, usable.length);
  assert.equal(stat.skipped, pairs.length - usable.length);
  assert.ok(Math.abs(stat.geomean - Math.exp(sum(usable.map(([a, b]) => Math.log(a / b))) / usable.length)) < 1e-12);
  assert.ok(stat.low <= stat.geomean && stat.geomean <= stat.high);
  assert.ok(stat.min <= stat.low && stat.high <= stat.max);
  assert.equal(stat.aLower + stat.bLower, usable.filter(([a, b]) => a !== b).length);
  assert.deepEqual(pairedRatio(pairs, seeded('s')), stat);
  assert.notDeepEqual([pairedRatio(pairs, seeded('t')).low, pairedRatio(pairs, seeded('t')).high], [stat.low, stat.high]);
  const flipped = pairedRatio(pairs.map(([a, b]) => [b, a]), seeded('s'));
  assert.ok(Math.abs(flipped.geomean * stat.geomean - 1) < 1e-9);
  const same = pairedRatio([[4, 2], [2, 1], [6, 3]], seeded('s'));
  assert.deepEqual([same.low, same.geomean, same.high].map(value => Number(value.toFixed(9))), [2, 2, 2]);
});

test('fitPrices recovers the prices that produced the costs', () => {
  const price = { input: 2e-6, cacheWrite: 4e-6, cacheRead: 1e-7, output: 1e-5 };
  const random = seeded('prices');
  const rows = Array.from({ length: 25 }, () => {
    const row = { input: Math.floor(random() * 50), cacheWrite: Math.floor(random() * 80_000), cacheRead: Math.floor(random() * 900_000), output: Math.floor(random() * 12_000) };
    return { ...row, cost: sum(Object.keys(price).map(key => price[key] * row[key])) };
  });
  const fit = fitPrices(rows);
  for (const key of Object.keys(price)) assert.ok(Math.abs(fit.perToken[key] / price[key] - 1) < 1e-6, key);
  assert.ok(fit.worstRelativeError < 1e-6);
  assert.equal(fitPrices(rows.slice(0, 2)), undefined);
});

test('field characters add up to the reply, whatever its shape', () => {
  const replies = [
    JSON.stringify({ results: [{ index: 0, data: { content: '1\ta😀\n2\tb', files: [{ path: 'a.ts', matches: ['3\tx', { line: 4, value: 'y' }] }], 'lib/core/': ['M +1 -1 a.js'] } }], next: { input: { cursor: 'c' } } }),
    '{"results":[]}',
    'not json at all é',
    '"a bare string"',
  ];
  for (const reply of replies) assert.equal(sum(Object.values(fieldChars(reply))), codePoints(reply));
  const fields = fieldChars(replies[0]);
  assert.equal(fields['results/*/data/content'], codePoints(JSON.stringify('1\ta😀\n2\tb')));
  assert.ok('results/*/data/{key}/*' in fields, 'a key that is data is not a field name');
  assert.deepEqual(Object.keys(fieldChars(replies[2])), ['(text)']);
});

test('a failed reply is recognized per arm kind, and refusals are told from errors', () => {
  const tool = text => ({ text, isError: false });
  assert.equal(replyFailed('mcp', tool('{"results":[{"index":0,"data":{"content":"1\\tx"}}]}')), false);
  const refused = tool('{"results":[{"index":0,"data":{"errorCode":"pathPolicyDenied","errorClass":"refused","error":"no"}}]}');
  const wrong = tool('{"results":[{"index":0,"data":{"errorCode":"notFound","errorClass":"correctInput","error":"no"}}]}');
  assert.equal(replyFailed('mcp', refused), true);
  assert.equal(replyFailed('mcp', tool('{"errorCode":"invalidInput","error":"bad"}')), true);
  assert.match(failureCause('mcp', refused), /^refused: pathPolicyDenied$/);
  assert.match(failureCause('mcp', wrong), /^error: notFound$/);
  assert.equal(replyFailed('shell', tool('{"errorCode":"x"}')), false, 'a shell arm fails by exit, status or refusal');
  const cases = [
    [{ text: 'gh (read-only benchmark): refused: only read-only commands are allowed: pr create', isError: true }, /^refused: wrapper/],
    [{ text: "A variable in this command can't be checked before it runs", isError: true }, /^refused: host/],
    [{ text: 'gh: Not Found (HTTP 404)', isError: false }, /^error: HTTP 404$/],
    [{ text: 'Exit code 1\nnothing', isError: true }, /^error: exit 1$/],
  ];
  for (const [reply, cause] of cases) {
    assert.equal(replyFailed('shell', reply), true);
    assert.match(failureCause('shell', reply), cause);
  }
});

test('a shell call is named by its gh command and its filters; a file read by how it selects lines', () => {
  assert.deepEqual(pipeline(`gh api x --jq '.a | .b' | grep -n "a|b" | sed -n 1,5p`).map(stage => stage[0]), ['gh', 'grep', 'sed']);
  const read = shellCall(`gh api 'repos/o/r/contents/src/a.ts?ref=abc' -H 'Accept: application/vnd.github.raw' | grep -n foo | sed -n 1,9p`);
  assert.deepEqual(read, { tool: 'gh api repos/*/contents', filters: 'grep | sed', fileMode: 'filter (grep)' });
  assert.equal(shellCall('gh api repos/o/r/contents/a.ts --jq .content | base64 -d | sed -n 10,20p').fileMode, 'range (sed, head, tail)');
  assert.equal(shellCall('gh api repos/o/r/contents/a.ts').fileMode, 'whole');
  assert.deepEqual(shellCall('gh api -X GET search/code -f q=x'), { tool: 'gh api search/code', filters: '(none)', fileMode: undefined });
  assert.equal(shellCall('gh pr view 1 -R o/r --json files').tool, 'gh pr view');
  assert.equal(shellCall('for f in a b; do gh api x; done').tool, '(not a gh command)');
  assert.equal(readMode({ path: 'a' }), 'whole');
  assert.equal(readMode({ path: 'a', ranges: [{ startLine: 1, endLine: 5 }] }), 'ranges');
  assert.equal(readMode({ path: 'a', ranges: [{ startLine: 1, endLine: 5 }], block: true }), 'ranges + block');
  assert.equal(readMode({ path: 'a', matchString: 'x', block: true }), 'matchString + block');
  assert.equal(readMode({ path: 'a', view: 'symbols' }), 'view: symbols');
});

/** A round of two arms, two passes and three questions with known usage. */
function storedRound(dir) {
  const manifest = roundIn(dir, { passes: 2, compare: [['tool', 'gh']] });
  const content = JSON.stringify({ results: [{ index: 0, data: { content: '1\tfn a() {}\n2\tfn b() {}', totalLines: 2 } }] });
  const refused = JSON.stringify({ results: [{ index: 0, data: { errorCode: 'pathPolicyDenied', errorClass: 'refused', error: 'no' } }] });
  const price = { input: 2e-6, cacheWrite: 4e-6, cacheRead: 1e-7, output: 1e-5 };
  let n = 0;
  for (const pass of [1, 2]) {
    for (const id of manifest.sets.s.ids) {
      for (const arm of ['tool', 'gh']) {
        n += 1;
        const steps = arm === 'tool'
          ? [
              { input: 3, cacheWrite: 500 + n, cacheRead: 20_000, calls: [{ name: 'mcp__octocode__localFetch', input: { queries: [{ path: 'a.rs', ranges: [{ startLine: 1, endLine: 2 }] }, { path: 'b.rs' }] }, reply: content }, { name: 'mcp__octocode__localFetch', input: { queries: [{ path: '.git/config' }] }, reply: refused, isError: true }] },
              { input: 2, cacheWrite: 900 + 7 * n, cacheRead: 20_500, calls: [{ name: 'mcp__octocode__localFetch', input: { cursor: 'c1' }, reply: content }] },
              { input: 2, cacheWrite: 300, cacheRead: 21_400, output: 400 + n },
            ]
          : [
              { input: 3, cacheWrite: 400 + n, cacheRead: 4_000, calls: [{ name: 'Bash', input: { command: 'gh api repos/o/r/contents/a.rs | grep fn' }, reply: 'fn a() {}\nfn b() {}' }, { name: 'Bash', input: { command: 'gh pr create' }, reply: 'gh (read-only benchmark): refused: only read-only commands are allowed: pr create', isError: true }] },
              { input: 2, cacheWrite: 200 + 3 * n, cacheRead: 4_400, output: 300 + n },
            ];
        const usage = key => sum(steps.map(step => step[key] ?? 0));
        const cost = price.input * usage('input') + price.cacheWrite * usage('cacheWrite') + price.cacheRead * usage('cacheRead') + price.output * usage('output');
        storeRun(runPath(manifest, pass, 's', id, arm), { steps, cost });
      }
      const judge = path.join(passPath(manifest, pass), 'judge', `s-${id}`);
      fs.mkdirSync(judge, { recursive: true });
      fs.writeFileSync(path.join(judge, 'verdict.json'), JSON.stringify({ scores: { tool: 10, gh: id === 'Q3' ? 6 : 9 }, facts: { tool: ['right', 'right'], gh: ['right', id === 'Q3' ? 'wrong' : 'missing'] }, order: ['tool', 'gh'] }));
    }
  }
  return manifest;
}

test('analyze: totals are sums of runs, the ledger accounts for every character and token, ratios cover every pair', () => {
  const { dir, cleanup } = scratch('analyze');
  try {
    const manifest = storedRound(dir);
    const numbers = analyze(manifest);
    const group = numbers.groups.s;
    const runs = arm => Object.values(numbers.per).flatMap(entry => Object.values(entry.passes).map(pass => pass.arms[arm]));
    for (const arm of ['tool', 'gh']) {
      const s = group.arms[arm];
      assert.equal(s.n, manifest.passes * manifest.sets.s.ids.length);
      for (const [total, key] of [['calls', 'calls'], ['failed', 'failed'], ['charsTotal', 'chars'], ['newTotal', 'new'], ['cachedTotal', 'cached'], ['outputTotal', 'output'], ['modelCalls', 'modelCalls']]) assert.equal(s[total], sum(runs(arm).map(run => run[key])), `${arm} ${total}`);
      const ledger = s.ledger;
      assert.equal(sum(Object.values(ledger.tools).map(row => row.chars)), s.charsTotal, 'tools hold every reply character');
      assert.equal(sum(Object.values(ledger.fields).map(row => row.chars)), s.charsTotal, 'fields hold every reply character');
      assert.equal(sum(Object.values(ledger.tools).map(row => row.calls)), s.calls);
      assert.equal(ledger.failures.refusals + ledger.failures.errors, s.failed);
      assert.ok(Math.abs(ledger.newInputTokens.total - s.newTotal) <= 0.02 * s.newTotal, 'attributed new input is the provider total within 2%');
      assert.equal(ledger.newInputTokens.providerReported, s.newTotal);
      assert.equal(ledger.firstCallCachedTokens.median, runs(arm)[0].firstCallCacheRead);
      assert.ok(s.extraModelCall.withoutNewInput > 0 && s.extraModelCall.total >= s.extraModelCall.withoutNewInput);
      assert.ok(Math.abs(s.costSplit.newInput + s.costSplit.cachedReread + s.costSplit.output - s.cost) <= 0.02);
    }
    const tool = group.arms.tool.ledger;
    assert.equal(tool.cursor.calls, group.arms.tool.n);
    assert.equal(tool.fileReads.ranges.requests, group.arms.tool.n);
    assert.equal(tool.fileReads.whole.requests, 2 * group.arms.tool.n);
    assert.equal(tool.failures.refusals, group.arms.tool.n);
    assert.equal(group.arms.gh.ledger.failures.refusals, group.arms.gh.n);
    assert.equal(group.arms.gh.ledger.fileReads['filter (grep)'].requests, group.arms.gh.n);
    assert.equal(group.arms.gh.facts.wrong, manifest.passes);
    assert.equal(group.arms.tool.facts.allRight, group.arms.tool.n);
    assert.equal(group.arms.tool.vs_gh.won, manifest.passes, 'a gap of more than one point is a win');
    const ratio = group.ratios['tool/gh'];
    for (const [metric] of RATIO_METRICS) {
      assert.equal(ratio[metric].n, manifest.sets.s.ids.length, metric);
      assert.ok(ratio[metric].low <= ratio[metric].geomean && ratio[metric].geomean <= ratio[metric].high, metric);
    }
    assert.equal(ratio.cost.bLower, manifest.sets.s.ids.length);
    assert.deepEqual(Object.keys(group.perPass), ['1', '2']);
    assert.deepEqual(analyze(manifest), numbers, 'the same stored round gives the same numbers');
    assert.ok(Math.abs(numbers.prices.perMillion.cacheRead - 0.1) < 1e-3);
    const text = tables(numbers);
    assert.match(text, /### Ledger: tool/);
    assert.match(text, /Cost ratio per pass: pass 1 /);
  } finally {
    cleanup();
  }
});

// The stored 25 November round (campaigns/ is local): analyze must reproduce the numbers its scratch harness wrote.
const stored = path.join(HARNESS, 'rounds', 'v19-v20-gh-2026-11-25.json');
const reference = fs.existsSync(stored) ? path.join(loadManifest(stored).campaign, 'numbers.json') : undefined;
test('analyze reproduces the stored 25 November round', { skip: !reference || !fs.existsSync(reference) ? 'the campaign folder is not on this machine' : false }, () => {
  const numbers = analyze(loadManifest(stored));
  const was = JSON.parse(fs.readFileSync(reference, 'utf8'));
  for (const [group, arms] of Object.entries(was.summary)) {
    for (const [arm, s] of Object.entries(arms)) {
      const now = numbers.groups[group].arms[arm];
      for (const key of ['n', 'judged', 'quality', 'tens', 'calls', 'failed', 'cost', 'newTotal', 'cachedTotal', 'outputTotal', 'chars', 'firstCallCacheRead', 'coldPrompt']) assert.equal(now[key], s[key], `${group} ${arm} ${key}`);
      assert.deepEqual(now[`vs_${arm === 'v20' ? 'v19' : 'v20'}`], s[`vs_${arm === 'v20' ? 'v19' : 'v20'}`]);
    }
  }
  for (const [name, entry] of Object.entries(was.per)) {
    for (const [arm, run] of Object.entries(entry.arms)) {
      const now = numbers.per[name].passes[1].arms[arm];
      for (const key of ['calls', 'failed', 'chars', 'new', 'cached', 'output', 'cost', 'modelCalls', 'answerChars']) assert.equal(now[key], run[key], `${name} ${arm} ${key}`);
    }
  }
  const all = numbers.groups['(all questions)'].arms;
  for (const [arm, s] of Object.entries(all)) {
    assert.equal(s.failed, sum(Object.values(was.per).map(entry => entry.arms[arm]?.failed ?? 0)), `${arm} failed over every question`);
    assert.equal(sum(Object.values(s.ledger.fields).map(row => row.chars)), s.charsTotal);
    assert.ok(Math.abs(s.ledger.newInputTokens.total - s.newTotal) <= 0.02 * s.newTotal);
  }
});
