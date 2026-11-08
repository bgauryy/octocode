// The schema-driven behavior matrix, on the built CLI and MCP. Rows come from the
// published query schemas (schema-matrix/derive.mjs): each finite value alone, then
// all pairs. Each row walks every required continuation and compares the union with
// fixture truth from an oracle that shares no code with the tool. A value with no
// row, or with no judge and no stated reason, fails acceptance under its own name.
//
// Usage: node harness/schema-matrix.mjs [tool[,tool]...]
// Environment: OCTOCODE_TEST_WORKERS overrides the machine-sized worker count.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { FIXTURES, ROOT } from './paths.mjs';
import { writeResults } from './mcp-client.mjs';
import { contract, contractValidators } from './contract.mjs';
import { nativeBinaryStamp } from './native-binary.mjs';
import { createDriver } from './tool-health/driver.mjs';
import { transportPagesProblem } from './tool-health/compare.mjs';
import { startSurfaces, tokenFor } from './tool-health/surfaces.mjs';
import { createInputCoverage } from './tool-health/input-coverage.mjs';
import { planRefusals, planTool, unplanned } from './schema-matrix/derive.mjs';
import { drain, workerCount } from './schema-matrix/pool.mjs';
import { createLedger } from './schema-matrix/ledger.mjs';
import { runRow } from './schema-matrix/runner.mjs';
import { replayLeads } from './schema-matrix/lead-replay.mjs';
import { buildLocalWorld, fixtureTexts } from './schema-matrix/world-local.mjs';
import { buildGraphWorld } from './schema-matrix/world-graph.mjs';
import { addSectionPullRequest, corpusAnswer } from './schema-matrix/world-github.mjs';
import { classificationAnswer } from './tool-health/mock-routes.mjs';
import { coverageReport, discriminationProblems } from './schema-matrix/acceptance.mjs';
import { PNG } from './tool-health/classification-checks.mjs';
import { SPECS } from './schema-matrix/specs.mjs';
import { runScenario } from './schema-matrix/scenarios.mjs';
import { SCENARIOS } from './schema-matrix/scenario-list.mjs';
import { withScenario } from './schema-matrix/scenario-scope.mjs';
import { snapshotBuild } from './build-snapshot.mjs';
snapshotBuild();

const started = performance.now();
const say = line => process.stdout.write(`${line}\n`);
process.setMaxListeners(0); // One signal listener per MCP server, one server per worker.
const selected = process.argv.slice(2).flatMap(arg => arg.split(',')).filter(Boolean);
const tools = contract.tools.filter(tool => !selected.length || selected.includes(tool.name));
const FIXTURE = path.join(FIXTURES, 'tool-health');
const { validateOutput, validateInput } = contractValidators({ precompile: true });
const schemas = new Map(contract.tools.map(tool => [tool.name, tool.outputSchema]));
const driver = createDriver({ validate: validateOutput, validateInput, validateTransport: transportPagesProblem, tools: new Set(contract.tools.map(tool => tool.name)), schemaOf: name => schemas.get(name) });
const coverage = createInputCoverage(tools);
const ledger = createLedger(contract);

// OCTOCODE_MATRIX_ROWS=none runs the scenarios alone (values then read as uncovered).
const plan = process.env.OCTOCODE_MATRIX_ROWS === 'none' ? [] : tools.flatMap(tool => (SPECS[tool.name] ? [...planTool(tool, SPECS[tool.name]), ...planRefusals(tool, SPECS[tool.name])] : []));
for (const row of plan) {
  SPECS[row.tool].normalize?.(row);
  row.input = { queries: [row.query], ...(row.responseChars && { responseChars: row.responseChars }) };
}
const missing = tools.flatMap(tool => unplanned(tool, plan.filter(row => row.tool === tool.name)));

async function startWorker(index) {
  const surfaces = await startSurfaces(FIXTURE, { unmetered: true });
  const world = buildLocalWorld(surfaces.ctx.tmp);
  const cwd = surfaces.ctx.tmp;
  fs.cpSync(path.join(FIXTURE, 'advanced-local'), path.join(world.dir, 'lsp'), { recursive: true });
  fs.writeFileSync(path.join(world.dir, 'lsp/code/use.test.ts'), "import { add } from './model';\nadd(1, 2);\n");
  buildGraphWorld(path.join(world.dir, 'graph'));
  fs.writeFileSync(path.join(world.dir, 'pixel.png'), PNG);
  const server = await surfaces.mcpServer('ghCloneRepo', cwd);
  const { mock } = surfaces.ctx;
  mock.model.repos.push({ owner: 'octo', name: 'matrix', description: 'Matrix fixture', language: 'TypeScript', stars: 1, files: fixtureTexts() });
  const sectionCommit = addSectionPullRequest(mock.model);
  const providerHeaders = [];
  const responders = {
    github: request => worker.override.github?.(request) ?? corpusAnswer(request),
    classification: (request, headers) => {
      providerHeaders.push(headers);
      return worker.override.provider?.(request, headers) ?? { body: classificationAnswer(request) };
    },
  };
  const worker = {
    index, surfaces: ['cli', 'mcp'], driver, world, cwd, server, pending: [], owned: surfaces, override: {}, schemaOf: name => schemas.get(name),
    context: { world, cwd, mock, registry: surfaces.ctx.registry, providerHeaders, home: path.join(surfaces.ctx.tmp, 'home'), graphId: null, sectionCommit },
    normalize: text => text.replaceAll(surfaces.ctx.tmp, '<tmp>').replaceAll(surfaces.ctx.mock.url, '<mock>'),
    call: async (surface, tool, input) => {
      const result = surface === 'mcp' ? await surfaces.mcpCall(server, tool, input) : await surfaces.cliCall(tool, input, { cwd, credentialTool: 'ghCloneRepo' });
      coverage.observe(tool, surface, input, result);
      return result;
    },
    withScenario: (scenario, surface, env, run) => withScenario(surfaces, { tool: scenario.tool ?? 'schema', name: scenario.id, surface, cwd, env }, responders, async () => {
      await run();
      failures.push(...await replayLeads(worker, ledger, { allow: scenario.allow }));
    }),
  };
  for (const surface of ['cli', 'mcp']) {
    mock.model.githubResponders.set(tokenFor('ghCloneRepo', surface), responders.github);
    mock.classificationResponders.set(tokenFor('ghCloneRepo', surface), responders.classification);
  }
  if (plan.some(row => row.tool === 'graphQuery')) {
    const built = await worker.call('cli', 'graphBuild', { queries: [{ path: 'matrix-world/graph' }] });
    worker.context.graphId = built.json?.results?.[0]?.data?.id;
  }
  return worker;
}

const stamp = nativeBinaryStamp();
const count = workerCount();
const report = { at: new Date().toISOString(), provenance: { contract: contract.fingerprint, native: stamp }, machine: { parallelism: os.availableParallelism(), memoryBytes: os.totalmem(), workers: count }, rows: plan.length };
const workers = await Promise.all(Array.from({ length: count }, (_, index) => startWorker(index)));
const failures = [];
const proved = new Map();
const prove = (row, surface, values) => { for (const [field, value] of values) (proved.get(`${row.tool}\0${field}\0${JSON.stringify(value)}`) ?? proved.set(`${row.tool}\0${field}\0${JSON.stringify(value)}`, new Set()).get(`${row.tool}\0${field}\0${JSON.stringify(value)}`)).add(surface); };
const discrimination = discriminationProblems(plan, SPECS, workers[0].context);
const queue = [...plan];
let done = 0;

await drain(workers, queue, async (worker, row) => {
  const outcome = await runRow(worker, SPECS[row.tool], row, ledger);
  failures.push(...outcome.failures);
  if (outcome.hasOracle) for (const surface of outcome.passed) prove(row, surface, outcome.provedValues);
  failures.push(...await replayLeads(worker, ledger));
  done += 1;
  if (done % 100 === 0) process.stderr.write(`${done}/${plan.length} rows, ${failures.length} failures\n`);
});

const scenarioQueue = selected.length ? SCENARIOS.filter(scenario => selected.includes(scenario.tool)) : [...SCENARIOS];
await drain(workers, scenarioQueue.flatMap(scenario => (scenario.surfaces ?? ['cli', 'mcp']).map(surface => ({ scenario, surface }))), async (worker, { scenario, surface }) => {
  const problems = await runScenario(worker, scenario, surface, ledger);
  failures.push(...problems);
  process.stderr.write(`${scenario.id} ${surface}: ${problems.length ? 'FAILED' : 'ok'}\n`);
});

await Promise.all(workers.map(async worker => { worker.server.close(); await worker.owned.close(); }));

const values = coverageReport({ tools, proved, specs: SPECS, surfaces: ['cli', 'mcp'] });
const stable = nativeBinaryStamp()?.sha256 === stamp?.sha256;
Object.assign(report, { seconds: Number(((performance.now() - started) / 1000).toFixed(1)), values: values.summary, inputValues: coverage.report().summary, uncovered: values.uncovered, acceptedOnly: values.acceptedOnly, unplanned: missing, discrimination, failures, stable, ...ledger.report(['cli', 'mcp']) });
const file = writeResults('schema-matrix', report);
for (const tool of tools) {
  const own = failures.filter(failure => failure.tool === tool.name);
  const uncovered = values.uncovered.filter(entry => entry.tool === tool.name);
  const line = `${tool.name.padEnd(17)} ${plan.filter(row => row.tool === tool.name).length} rows, ${own.length} failures, ${uncovered.length} values uncovered`;
  say(`${own.length || uncovered.length ? 'FAIL' : 'PASS'} ${line}`);
}
const seen = new Set();
for (const failure of failures) {
  const signature = `${failure.tool} ${failure.check} ${failure.surface}`;
  if (seen.has(`${signature} ${failure.detail}`)) continue;
  seen.add(`${signature} ${failure.detail}`);
  say(`FAIL ${failure.id} ${failure.check} ${failure.surface}: ${String(failure.detail).slice(0, 300)}\n     input ${JSON.stringify(failure.input).slice(0, 400)}`);
}
for (const entry of values.uncovered) say(`FAIL uncovered ${entry.tool}.${entry.field}=${JSON.stringify(entry.value)} on ${entry.surfaces.join(',')}`);
for (const entry of discrimination) say(`FAIL no-effect ${entry.tool}.${entry.field}: every value gives the same truth`);
if (!stable) say('FAIL provenance: native artifact changed during the run');
say(`schema-matrix: ${plan.length} rows, ${failures.length} failures, values proved cli ${values.summary.cli.proved}/${values.summary.total}, mcp ${values.summary.mcp.proved}/${values.summary.total} in ${report.seconds}s on ${count} workers — ${path.relative(ROOT, file)}`);
process.exitCode = failures.length || values.uncovered.length || discrimination.length || !stable ? 1 : 0;
