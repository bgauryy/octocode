import assert from 'node:assert/strict';
import { AsyncLocalStorage } from 'node:async_hooks';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { runScenario } from './scenarios.mjs';
import { withScenario } from './scenario-scope.mjs';

function fixture(t) {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'matrix-scope-'));
  t.after(() => fs.rmSync(cwd, { recursive: true, force: true }));
  const store = new AsyncLocalStorage();
  const mock = { url: 'http://fixture', model: { githubResponders: new Map() }, classificationResponders: new Map() };
  let sequence = 0;
  const surfaces = { ctx: { mock }, scenario: (options, run) => store.run({ ...options, token: `credential-${++sequence}` }, () => run({ token: store.getStore().token })) };
  const rejected = new Set(); const observations = []; const leads = [];
  const worker = {
    cwd, world: { dir: cwd }, context: { mock }, override: {}, schemaOf: () => ({}),
    call: async (_surface, _tool, input) => {
      const scope = store.getStore() ?? { token: 'shared' };
      const answer = rejected.has(scope.token) ? { status: 500 } : (mock.model.githubResponders.get(scope.token)?.(input) ?? worker.override.github?.(input) ?? { status: 200 });
      if (answer.status === 401) rejected.add(scope.token);
      observations.push({ ...scope, status: answer.status, body: answer.body });
      return { json: { results: [] }, status: answer.status };
    },
    withEnv: (_surface, _env, run) => run(),
    withScenario: (scenario, surface, env, run) => withScenario(surfaces, { name: scenario.id, tool: scenario.tool, surface, cwd, env }, {
      github: request => worker.override.github?.(request) ?? { status: 200 },
      classification: request => worker.override.provider?.(request),
    }, async () => { await run(); leads.push({ ...store.getStore(), responder: mock.model.githubResponders.has(store.getStore().token) }); }),
  };
  return { worker, observations, leads, mock, store, surfaces };
}

for (const surface of ['cli', 'mcp']) test(`a rejected ${surface} credential cannot poison the next scenario or its leads`, async t => {
  const { worker, observations, leads, mock } = fixture(t);
  const ledger = { observe() {} };
  const scenario = (id, status) => ({ id, tool: 'ghGetHistoryItem', env: () => ({ OCTOCODE_HOME: `home-${id}` }), github: () => ({ status, body: id }), run: async ({ call }) => { const result = await call('ghGetHistoryItem', {}); return result.status === status ? null : `expected ${status}, got ${result.status}`; } });
  assert.deepEqual(await runScenario(worker, scenario('rejected', 401), surface, ledger), []);
  assert.deepEqual(await runScenario(worker, scenario('history', 200), surface, ledger), []);
  assert.notEqual(observations[0].token, observations[1].token);
  assert.deepEqual(observations.map(row => [row.status, row.body, row.env.OCTOCODE_HOME]), [[401, 'rejected', 'home-rejected'], [200, 'history', 'home-history']]);
  assert.deepEqual(leads.map(row => [row.token, row.responder]), observations.map(row => [row.token, true]));
  assert.equal(mock.model.githubResponders.size, 0); assert.equal(mock.classificationResponders.size, 0);
});

test('throwing scenarios release both responders and the environment scope', async t => {
  const { surfaces, mock, store } = fixture(t);
  await assert.rejects(withScenario(surfaces, { surface: 'cli' }, { github: () => 1, classification: () => 2 }, async () => {
    assert.equal(mock.model.githubResponders.get(store.getStore().token)(), 1);
    assert.equal(mock.classificationResponders.get(store.getStore().token)(), 2);
    throw new Error('scenario failed');
  }), /scenario failed/);
  assert.equal(mock.model.githubResponders.size, 0); assert.equal(mock.classificationResponders.size, 0);
  assert.equal(store.getStore(), undefined);
});
