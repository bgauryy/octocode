// V9: data flow enters as cited evidence. A CodeQL path-problem SARIF log (tests/fixtures/codeql-js-app, written in
// CodeQL's output shape for the app beside it) and a Joern flow export become FLOWS_TO relations, step to step, and
// `trace <source file> FLOWS_TO+` reaches the sink with every step's location and the rule id.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { copyFileSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { tempDir } from './test-tmp.mjs';
import { enrichExport } from '../src/enrich.ts';
import { runQuery } from '../src/runtime/query.ts';

const app = fileURLToPath(new URL('./fixtures/codeql-js-app/', import.meta.url));
const REVISION = '3f2a9c1e5b7d4a6f8e0c2b4d6f8a0c2e4b6d8f0a';
const digest = bytes => createHash('sha256').update(bytes).digest('hex');

// The app's graph as a build saves it (only what trace reads), enriched with one provider artifact.
async function enriched(format, file, { revision = REVISION } = {}) {
  const dir = tempDir('graph-flows-');
  const base = join(dir, 'data.json');
  const data = JSON.stringify({
    section: app.replace(/\/$/, ''),
    files: ['src/api.js', 'src/service.js', 'src/db.js'].map(id => ({ id, language: 'javascript' })),
    symbols: [],
    imports: [],
    calls: [],
  });
  writeFileSync(base, data);
  copyFileSync(join(app, file), join(dir, file));
  writeFileSync(
    join(dir, 'manifest.json'),
    JSON.stringify({
      baseSha256: digest(data),
      revision,
      providers: [{ format, file, sha256: digest(readFileSync(join(dir, file))), revision }],
    })
  );
  const out = join(dir, 'enriched.json');
  await enrichExport(base, join(dir, 'manifest.json'), out);
  return JSON.parse(readFileSync(out, 'utf8'));
}

// The location a step arrives at, as `path:line:column`.
const where = step => `${step.details.to.path}:${step.details.to.line}:${step.details.to.column}`;

test('V9: trace follows a CodeQL path result from the source file to the sink with its step locations', async () => {
  const D = await enriched('codeql-sarif', 'codeql-results.sarif');
  assert.deepEqual(D.enrichment.kinds, ['FLOWS_TO']);
  const provider = D.enrichment.providers[0];
  assert.deepEqual([provider.results, provider.flows, provider.withoutFlow, provider.tools], [2, 1, 1, ['CodeQL 2.19.3']]);
  const answer = runQuery(D, 'trace src/api.js FLOWS_TO+');
  assert.equal(answer.ok, true, answer.error);
  // Every step of the thread flow, in CodeQL's order, each cited where it arrives.
  const sarif = JSON.parse(readFileSync(join(app, 'codeql-results.sarif'), 'utf8'));
  const expected = sarif.runs[0].results[0].codeFlows[0].threadFlows[0].locations.map(({ location: l }) => {
    const r = l.physicalLocation.region;
    return `${l.physicalLocation.artifactLocation.uri}:${r.startLine}:${r.startColumn}`;
  });
  const steps = answer.steps.toSorted((a, b) => a.details.step - b.details.step);
  assert.deepEqual(steps.map(where), expected);
  assert.ok(steps.every(s => s.details.ruleId === 'js/sql-injection' && s.relation === 'FLOWS_TO'));
  assert.deepEqual(steps.map(s => [s.path, s.lines]), expected.map(e => [e.split(':')[0], [Number(e.split(':')[1])]]));
  assert.equal(steps.at(-1).details.role, 'sink');
  assert.equal(steps[0].details.role, 'source');
  assert.equal(steps[0].source, 'src/api.js', 'the source file starts the flow');
  // The sink is reached: an endpoint at db.js line 8, the query's argument.
  const sink = answer.rows.find(r => r.endpoint === 'src/db.js#L8:16');
  assert.ok(sink, JSON.stringify(answer.rows.map(r => r.endpoint)));
  assert.deepEqual(sink.loc, { path: 'src/db.js', line: 8 });
  assert.ok(answer.edges.some(([a, b]) => a === 'src/api.js' && b === 'src/service.js'), 'the flow crosses files');
  // A step's own location answers too, and a walk from the sink goes nowhere.
  assert.equal(runQuery(D, 'trace src/db.js#L8:16 FLOWS_TO+').rows.length, 0);
  assert.equal(runQuery(D, 'trace src/service.js#L3:19 FLOWS_TO').rows[0].endpoint, 'src/service.js#L4:20');
});

test('V9: a SARIF log of another revision is refused', async () => {
  await assert.rejects(enriched('codeql-sarif', 'codeql-results.sarif', { revision: 'b'.repeat(40) }), /analyzed revision 3f2a9c1e/);
  // An abbreviation of the analyzed commit is the same revision.
  const D = await enriched('codeql-sarif', 'codeql-results.sarif', { revision: REVISION.slice(0, 12) });
  assert.equal(D.enrichment.relations.length, 10);
});

test('V9: Joern flows reach the same sink, one relation per step', async () => {
  const D = await enriched('joern-flows', 'joern-flows.json');
  const answer = runQuery(D, 'trace src/api.js FLOWS_TO+');
  assert.equal(answer.ok, true, answer.error);
  assert.ok(answer.rows.some(r => r.endpoint === 'src/db.js#L8:16'));
  const steps = answer.steps.toSorted((a, b) => a.details.step - b.details.step);
  assert.equal(steps.length, 7);
  assert.deepEqual(steps.map(s => [s.details.to.nodeType, s.details.to.method]).slice(2, 3), [['MethodParameterIn', 'findUser']]);
  assert.ok(steps.every(s => s.details.ruleId === 'user input to SQL query' && s.details.tool === 'joern'));
});

test('flow providers name the field a malformed export lacks', async () => {
  const { adapters } = await import('../src/providers.ts');
  const opts = { artifact: 'x.json', sourceRoot: '/repo' };
  assert.throws(() => adapters['joern-flows'].read([{ elements: [{ lineNumber: 3 }] }], opts), /for flow 0 element 0 file/);
  assert.throws(() => adapters['codeql-sarif'].read({ version: '2.0.0', runs: [] }, opts), /SARIF version 2\.1\.0/);
  assert.throws(
    () => adapters['codeql-sarif'].read({ version: '2.1.0', runs: [{ tool: { driver: { name: 'CodeQL' } }, results: [{ codeFlows: [{ threadFlows: [{ locations: [{ location: { physicalLocation: { artifactLocation: { uri: 'a.js' }, region: { startLine: 0 } } } }] }] }] }] }] }, opts),
    /positive integer for run 0 result 0 codeFlow 0 threadFlow 0 step 0 startLine/
  );
});
