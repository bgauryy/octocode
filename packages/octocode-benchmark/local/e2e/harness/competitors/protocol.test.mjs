import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { contractValidators } from '../contract.mjs';
import { bytesUnderKey, keyBytes, gateTask } from '../sensors.mjs';
import { callIssue, offeredContinuations, walkContinuations } from './navigation.mjs';
import { replayCalls, runRecipe } from './octocode.mjs';
import { prepareShell, removeShellScratch, shellSide } from './shell.mjs';
import { taskSensors } from './response-sensors.mjs';
import { emptyEvidence } from './truth.mjs';
import { octocodeEvidence } from './octocode-evidence.mjs';
import { qualityPair, retainColdTrial } from './trials.mjs';

const next = cursor => ({ kind: 'continue', tool: 'localSearch', input: { cursor } });
const related = { kind: 'related', tool: 'localFetch', input: { queries: [{ path: 'unused.ts' }] } };
const relatedTree = ref => ({ kind: 'related', tool: 'ghStructure', input: { queries: [{ operation: 'tree', owner: 'o', repo: 'r', ref }] } });
const entry = (raw, input = { queries: [{ path: '.', matchString: ['x'] }] }, extras = {}) => {
  const output = JSON.stringify(raw);
  return { tool: 'localSearch', args: input, raw, sc: raw, output, text: output, bytes: Buffer.byteLength(output), ms: 1, isError: false, ...extras };
};

test('every recipe resolves and validates against the current public contract without repairs', async () => {
  const tasks = JSON.parse(fs.readFileSync(new URL('../competitor-tasks.json', import.meta.url), 'utf8')).tasks;
  const { validateInput } = contractValidators();
  assert.ok(tasks.length > 0);
  // Typed related calls and clone locations exercise templates without provider calls.
  const response = { results: [{ data: { location: { localPath: 'fixture/clone' }, related: [
    { kind: 'related', tool: 'ghStructure', input: { queries: [{ operation: 'tree', owner: 'expressjs', repo: 'express', ref: 'v4.21.2' }] } },
    { kind: 'related', tool: 'ghGetHistoryItem', input: { queries: [{ operation: 'pullRequest', owner: 'cli', repo: 'cli', number: 14405 }] } },
  ] } }] };
  for (const task of tasks) {
    for (const step of task.octocode) {
      if (step.walk) assert.equal(step.walk, true, task.id);
      for (const obsolete of ['args', 'queries', 'envelope']) assert.equal(step[obsolete], undefined, `${task.id}: ${obsolete}`);
    }
    const run = await runRecipe(task, '', async (tool, input) => {
      assert.equal(validateInput(tool, input), null, `${task.id}: ${tool}`);
      return entry(response, input, { tool });
    });
    assert.equal(run.error, null, task.id);
    assert.equal(run.entries.length, task.octocode.filter(step => step.tool || step.cli).length, task.id);
  }
});

test('related templates select typed arrays and reject ambiguous candidates', async () => {
  const task = { octocode: [
    { tool: 'artifactSearch', input: { queries: [{ operation: 'resolve', ecosystem: 'npm', packageName: 'example' }] } },
    { tool: 'ghGetFileContent', input: { queries: [{ owner: 'o', repo: 'r', path: 'a.ts', ref: '{{related:ghStructure.tree.input.queries.0.ref}}' }] } },
  ] };
  const run = await runRecipe(task, '', async (tool, input) => entry({ results: [{ data: { related: [relatedTree('v1'), relatedTree('v2')] } }] }, input, { tool }));
  assert.match(run.error, /ambiguous related call ghStructure.tree/);
  assert.equal(run.entries.length, 1);
});

test('cursor walk drains response pages before row pages and ignores optional related calls', async () => {
  const first = entry({ results: [{ data: { next: { matches: next('row-a') }, related: [related] } }], responsePagination: { hasMore: true, next: next('outer-a') } });
  const responses = {
    'outer-a': { results: [{ data: { next: { matches: next('row-a'), files: next('row-b') } } }], responsePagination: { hasMore: true, next: next('outer-b') } },
    'outer-b': { results: [{ data: { value: 'last response' } }] },
    'row-a': { results: [{ data: { value: 'row A', next: { matches: next('row-c') } } }] },
    'row-b': { results: [{ data: { value: 'row B' } }] },
    'row-c': { results: [{ data: { value: 'row C' } }] },
  };
  const calls = [];
  const entries = [first];
  const result = await walkContinuations(entries, 0, async (tool, input) => {
    assert.equal(tool, 'localSearch');
    assert.deepEqual(Object.keys(input), ['cursor']);
    calls.push(input.cursor);
    return entry(responses[input.cursor], input);
  });
  assert.equal(result.exhausted, true);
  assert.deepEqual(calls, ['outer-a', 'outer-b', 'row-a', 'row-b', 'row-c']);
  assert.equal(offeredContinuations(entries).length, 5);
  assert.ok(offeredContinuations(entries).every(call => call.byRecipe));
});

test('continuation cycles, exhausted budgets and runtime errors cannot become complete walks', async () => {
  const first = entry({ next: { page: next('a') } });
  await assert.rejects(walkContinuations([first], 0, async (_tool, input) => entry({ next: { page: next('a') } }, input)), /continuation cycle/);
  const capped = await walkContinuations([first], 0, async (_tool, input) => entry({ next: { page: next('b') } }, input), 1);
  assert.equal(capped.exhausted, false);
  assert.equal(capped.remaining, 1);
  const entries = [first];
  const failed = await walkContinuations(entries, 0, async (_tool, input) => entry({ results: [{ status: 'error', data: { errorCode: 'cursorExpired' } }] }, input));
  assert.equal(failed.exhausted, false);
  assert.equal(entries.length, 2);
  assert.match(failed.error, /error rows/);
});

test('cross-branch cursor cycles fail while shared DAG continuations execute once', async () => {
  const first = entry({ next: { a: next('a'), b: next('b') } });
  const cyclic = { a: { next: { page: next('b') } }, b: { next: { page: next('a') } } };
  await assert.rejects(walkContinuations([first], 0, async (_tool, input) => entry(cyclic[input.cursor], input)), /continuation cycle/);
  const dag = { a: { next: { page: next('c') } }, b: { next: { page: next('c') } }, c: { results: [] } };
  const calls = [];
  const result = await walkContinuations([first], 0, async (_tool, input) => {
    calls.push(input.cursor);
    return entry(dag[input.cursor], input);
  });
  assert.equal(result.exhausted, true);
  assert.deepEqual(calls, ['a', 'b', 'c']);
});

test('schema-valid replay still fails when the tool returns a runtime error', async () => {
  const first = entry({ next: { page: next('a') } });
  const replay = await replayCalls([first], async (_tool, input) => entry({ results: [{ status: 'error', data: { errorCode: 'cursorExpired' } }] }, input), 1);
  assert.equal(replay.failed, 1);
  assert.equal(replay.replays[0].schemaErrors, 0);
  assert.deepEqual(replay.replays[0].input, { cursor: 'a' });
  assert.ok(replay.replays[0].output.includes('cursorExpired'));
});

test('failed recipe calls retain their input, output and cost', async () => {
  const task = { octocode: [{ tool: 'localSearch', input: { queries: [{ path: '.', matchString: ['x'] }] } }] };
  const run = await runRecipe(task, '', async (_tool, input) => entry({ results: [{ status: 'error', data: { errorCode: 'pathNotFound' } }] }, input));
  assert.match(run.error, /error rows/);
  assert.equal(run.entries.length, 1);
  assert.ok(run.entries[0].bytes > 0);
  assert.deepEqual(run.entries[0].args, task.octocode[0].input);
  assert.ok(callIssue(run.entries[0]));
});

test('a successful warm trial cannot erase a failed cold trial or its measured output', () => {
  const warm = { ok: true, fails: [], bytes: 10, steps: [{ output: 'warm' }] };
  const cold = { ok: false, fails: ['runtime failure'], bytes: 25, steps: [{ output: 'failed output' }] };
  retainColdTrial(warm, cold);
  assert.equal(warm.ok, false);
  assert.deepEqual(warm.fails, ['cold trial: runtime failure']);
  assert.equal(warm.bytes, 10);
  assert.equal(warm.cold.bytes, 25);
  assert.equal(warm.cold.steps[0].output, 'failed output');
  const missedPage = { ok: true, fails: [], sensors: { violations: [{}], schemaErrors: 0 } };
  const second = { ok: true, fails: [] };
  retainColdTrial(second, missedPage);
  assert.equal(second.ok, false);
});

test('efficiency comparisons require passing pairs without explicit coverage gaps', () => {
  const good = { ok: true, bytes: 100 };
  assert.equal(qualityPair({ octocode: good, competitor: good }), true);
  assert.equal(qualityPair({ octocode: good, competitor: { ok: false, bytes: 1 } }), false);
  assert.equal(qualityPair({ octocode: { ...good, sensors: { coverageGaps: [{}] } }, competitor: good }), false);
  assert.equal(qualityPair({ octocode: { ...good, replay: { failed: 1 } }, competitor: good }), false);
  assert.equal(qualityPair({ octocode: { ...good, cold: { ok: false } }, competitor: good }), false);
});

test('UTF-8 byte sensors and shell receipts preserve Unicode and failing output', () => {
  const value = { next: { message: '日本🙂' } };
  assert.equal(bytesUnderKey(value, 'next'), Buffer.byteLength(JSON.stringify(value)) - 2);
  assert.equal(keyBytes([{ raw: { results: [{ data: { content: '日本🙂' } }] } }])['data.content'], Buffer.byteLength('"content":"日本🙂"'));
  prepareShell();
  try {
    const result = shellSide([{ sh: "printf '日本🙂'; exit 2" }], { text: ['日本'] }, '');
    assert.equal(result.ok, false);
    assert.equal(result.bytes, Buffer.byteLength('日本🙂'));
    assert.equal(result.steps[0].stdout, '日本🙂');
    assert.equal(result.steps[0].exit, 2);
  } finally { removeShellScratch(); }
});

test('shell exit one and failed pipeline stages cannot pass using partial evidence', () => {
  prepareShell();
  try {
    for (const sh of ["printf 'needle'; printf 'failure' >&2; exit 1", "(printf 'needle'; exit 1) | cat", "false; printf 'needle'"]) {
      const result = shellSide([{ sh }], { text: ['needle'] }, '');
      assert.equal(result.ok, false, sh);
      assert.equal(result.steps[0].exit, 1, sh);
      assert.ok(result.fails.some(failure => failure.includes('shell exit 1')), sh);
    }
    const noMatch = shellSide([{ sh: 'rg -n needle /dev/null', expectedExitCodes: [0, 1] }], {}, '');
    assert.equal(noMatch.ok, true);
    assert.equal(noMatch.steps[0].exit, 1);
  } finally { removeShellScratch(); }
});

test('optional related calls cannot hide missing required pagination or provide source evidence', () => {
  const raw = { results: [{ data: { pagination: { hasMore: true }, related: [related], hints: ['read unused.ts'] } }] };
  const sensors = taskSensors([entry(raw)], [{ walk: true, exhausted: true }]);
  assert.ok(sensors.violations.some(row => row.signal === 'hasMore'));
  const ev = emptyEvidence();
  octocodeEvidence(entry(raw), ev);
  assert.equal(ev.files.size, 0);
  assert.ok(!ev.text.includes('unused.ts'));
});

test('a measurement change cannot silently compare new UTF-8 bytes to old code-unit baselines', () => {
  const result = gateTask({ id: 'x', measurement: 'utf8-v1', bytes: 4 }, { anchor: { tasks: { x: { bytes: 2 } } } });
  assert.deepEqual(result.flags, []);
  assert.deepEqual(result.compared, []);
  assert.match(result.notes[0], /measurement changed; not comparable/);
});

test('local evidence excludes skipped paths and diagnostics and counts each actual location once', () => {
  const raw = { results: [{ data: { files: [{ path: 'real.rs', matches: [{ line: 7, value: 'needle', enclosing: { line: 2 }, parseAssessment: { range: { start: { line: 6 } } } }] }], diagnostics: [{ path: 'diagnostic.rs', line: 55 }], skippedPaths: { entries: [{ path: 'hidden.rs' }] } } }] };
  const ev = emptyEvidence();
  octocodeEvidence(entry(raw), ev);
  assert.deepEqual(ev.pairs, [{ file: 'real.rs', line: 7 }]);
  assert.deepEqual([...ev.files], ['real.rs']);
  const refs = entry({ results: [{ data: { matches: [{ path: 'ref.rs', displayRange: { startLine: 10 }, content: '10\tcall();\n' }] } }] }, undefined, { tool: 'lspSearch' });
  const refEvidence = emptyEvidence();
  octocodeEvidence(refs, refEvidence);
  assert.deepEqual(refEvidence.pairs, [{ file: 'ref.rs', line: 10 }]);
});

test('document-analysis errors remain a semantic coverage gap without inventing missing pages', () => {
  const raw = { results: [{ data: { isPartial: true, partialReasons: ['documentAnalysisErrors'], lsp: { analysisContext: { diagnostics: 'errors', errorCount: 1 } }, pagination: { hasMore: false } } }] };
  const sensors = taskSensors([entry(raw, undefined, { tool: 'lspSearch' })], [{ walk: true, exhausted: true }]);
  assert.equal(sensors.violations.length, 0);
  assert.equal(sensors.coverageGaps.length, 1);
  const missingPage = structuredClone(raw);
  missingPage.results[0].data.pagination.hasMore = true;
  assert.ok(taskSensors([entry(missingPage, undefined, { tool: 'lspSearch' })], []).violations.some(issue => issue.signal === 'hasMore'));
});
