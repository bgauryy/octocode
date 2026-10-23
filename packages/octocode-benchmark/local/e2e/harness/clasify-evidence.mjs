// Hermetic conformance under deliberately wrong/uncertain provider judgments.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { ROOT, contract, setup, startServer, walk, recoverUnfiltered, dataOf } from './read-tool-selection/support.mjs';
import { gradeRelations } from './classification-evidence/grade.mjs';

const output = path.join(ROOT, 'packages/octocode-benchmark/local/e2e/results/clasify-evidence');
fs.mkdirSync(output, { recursive: true });
const checks = [], raw = [];
const fact = 'Restoration requires two approvals.';
const boundaryFacts = ['Project Orion follows policy K17.', 'Policy K17 requires two approvals.'];
const boundaryText = [boundaryFacts[0], ...Array.from({ length: 160 }, (_, n) =>
  `Unrelated operational entry ${n}: ordinary housekeeping and maintenance records.`), boundaryFacts[1]].join('\n') + '\n';
const frozen = {
  version: 1, kind: 'deterministic conformance, not provider accuracy',
  fact, boundaryFacts, boundaryText,
  relationOracle: { 'A->B': true, 'B->C': true, 'A->C': false, 'B->A': false },
  paidRequestBudget: 0,
  contractFingerprint: contract.fingerprint,
};
fs.writeFileSync(path.join(output, 'frozen.json'), JSON.stringify(frozen, null, 2));

function response(request, level) {
  return { body: {
    model: request.model,
    answers: Object.fromEntries(Object.entries(request.questions).map(([key, q]) => {
      if (q.type === 'score') return [key, {
        type: 'score', score: level, confidence: 1,
        probabilities: { 0: level === 0 ? 1 : 0, 1: level === 1 ? 1 : 0, 2: level === 2 ? 1 : 0 },
        legend: Object.fromEntries(q.criteria.map((v, i) => [i, v])),
      }];
      assert.equal(q.type, 'choice');
      const choice = level === 2 ? 'yes' : level === 0 ? 'no' : 'unknown';
      return [key, { type: 'choice', choice, confidence: 1,
        probabilities: Object.fromEntries(['yes', 'no', 'unknown'].map(v => [v, v === choice ? 1 : 0])) }];
    })), usage: { input_tokens: 10, output_tokens: 2 },
  } };
}

async function check(name, run) {
  const start = performance.now();
  try { checks.push({ name, status: 'PASS', detail: await run(), ms: performance.now() - start }); }
  catch (error) { checks.push({ name, status: 'FAIL', error: error.stack, ms: performance.now() - start }); }
  console.log(`${checks.at(-1).status} ${name}`);
}

for (const tool of ['localFetch', 'localSearch']) {
  for (const [mode, level] of [['false-negative', 0], ['unknown', 1]]) {
    await check(`${tool}: ${mode} retains exact unfiltered recovery`, async () => {
      const ctx = await setup({ label: 'evidence-recovery', classificationResponder: r => response(r, level) });
      const record = { name: `${tool}:${mode}` };
      raw.push(record);
      let mcp;
      try {
        const file = path.join(ctx.fixture, 'fact.txt');
        fs.writeFileSync(file, fact + '\n');
        mcp = await startServer({ env: ctx.env });
        const options = { surface: 'mcp', mcp };
        const spec = { tool, kind: tool === 'localFetch' ? 'source' : 'files' };
        const query = tool === 'localFetch' ? { path: file, fullContent: true } : { path: file, matchString: 'Restoration', contextLines: 0 };
        const ordinary = record.ordinary = await walk(ctx, spec, query, options);
        const selected = record.selected = await walk(ctx, spec, { ...query,
          clasifyContext: { classifiers: ['Does this state the number of approvals required for restoration?'] } }, options);
        assert.equal(selected.rows.length, 0, 'injected model miss must remain visible as a selected-view miss');
        // The per-call disclosure of a filtered view: its receipt and the
        // unfiltered recovery lead (the "not proof of absence" rule is said
        // once, in the instructions).
        assert.ok(dataOf(selected.responses[0]).next?.unfiltered, 'selected empty view must lead to the unfiltered view');
        const receipt = dataOf(selected.responses[0]).selection;
        // A middle (related) score is a decided near-miss, hidden and counted
        // apart from unknown candidates.
        assert.ok(level === 1 ? receipt.relatedItems > 0 : receipt.excludedItems > 0, JSON.stringify(receipt));
        const before = ctx.calls();
        const recovered = record.recovered = await recoverUnfiltered(ctx, spec, selected.responses[0], options);
        assert.deepEqual(recovered.rows, ordinary.rows, 'recover complete original evidence, not only unknowns');
        assert.equal(ctx.calls(), before, 'recovery cannot spend inference');
        return { selectedFactRecall: { correct: 0, total: 1 }, recoveryExact: true, mockProviderRequests: ctx.calls(), paidProviderRequests: 0 };
      } finally { record.packets = ctx.mock.classificationRequests; record.mockProviderRequests = ctx.calls(); mcp?.close(); await ctx.close(); }
    });
  }
}

await check('boundary-spanning evidence requires whole-source fallback', async () => {
  const ctx = await setup({ label: 'evidence-boundary', classificationResponder: r => {
    const state = JSON.stringify(r.state);
    return response(r, boundaryFacts.every(f => state.includes(f)) ? 2 : 1);
  } });
  let mcp;
  const record = { name: 'boundary' };
  raw.push(record);
  try {
    const file = path.join(ctx.fixture, 'boundary.txt');
    fs.writeFileSync(file, boundaryText);
    mcp = await startServer({ env: { ...ctx.env, OCTOCODE_BETA: 'true' } });
    const spec = { tool: 'localFetch', kind: 'source' }, options = { surface: 'mcp', mcp };
    const query = { path: file, fullContent: true };
    const ordinary = record.ordinary = await walk(ctx, spec, query, options);
    const selected = record.selected = await walk(ctx, spec, { ...query,
      clasifyContext: { classifiers: ['Does Project Orion require two approvals?'] } }, options);
    // Chunks holding one fact each score related: decided, hidden, recoverable.
    assert.ok(dataOf(selected.responses[0]).selection.relatedItems > 0, JSON.stringify(dataOf(selected.responses[0]).selection));
    assert.equal(selected.rows.length, 0);
    const before = ctx.calls();
    const recovered = record.recovered = await recoverUnfiltered(ctx, spec, selected.responses[0], options);
    assert.deepEqual(recovered.rows, ordinary.rows);
    assert.equal(ctx.calls(), before);
    const whole = record.whole = await mcp.raw('clasify', { queries: [{ id: 'whole-source',
      source: { tool: 'localFetch', query }, questions: { fact: { type: 'choice',
        instructions: 'Does Project Orion require two approvals?',
        criteria: { yes: 'The source establishes that requirement.', no: 'The source contradicts it.', unknown: 'The source does not establish it.' },
      } } }] });
    assert.equal(whole.sc?.queries?.[0]?.answers?.fact?.choice, 'yes');
    return { selectedFactRecall: { correct: 0, total: 1 }, wholeSourceResolved: true, recoveryExact: true, mockProviderRequests: ctx.calls(), paidProviderRequests: 0 };
  } finally { record.packets = ctx.mock.classificationRequests; record.mockProviderRequests = ctx.calls(); mcp?.close(); await ctx.close(); }
});

await check('relationship grading preserves semantic misses despite successful transport', async () => {
  const result = gradeRelations(frozen.relationOracle, {
    'A->B': { choice: 'no', confidence: 1 }, 'B->C': { choice: 'unknown' },
    'A->C': { choice: 'yes', confidence: 1 }, 'B->A': { choice: 'no' },
  });
  assert.deepEqual([result.total, result.correct, result.incorrect, result.unknown], [4, 1, 2, 1]);
  return { semanticResult: result, modality: 'synthetic answer grading only; no image inference' };
});

fs.writeFileSync(path.join(output, 'raw.json'), JSON.stringify(raw, null, 2));
fs.writeFileSync(path.join(output, 'results.json'), JSON.stringify({ checks,
  paidProviderRequests: 0, liveProviderCost: 'not applicable',
  limitation: 'Injected judgments test recovery and grading. They do not measure model accuracy or classifier benefit.' }, null, 2));
process.exitCode = checks.some(c => c.status === 'FAIL') ? 1 : 0;
