import assert from 'node:assert/strict';
import path from 'node:path';
import { test } from 'node:test';
import { advancedContext } from './advanced-context.mjs';
import { createDriver } from './driver.mjs';
import { startSurfaces, withoutReadinessPings } from './surfaces.mjs';
import { contract, contractValidators } from '../contract.mjs';
import { FIXTURES } from '../paths.mjs';

const tool = 'ghGetFileContent';
const guide = { owner: 'octo', repo: 'demo-docs', path: 'guide.txt', ranges: [{ startLine: 1, endLine: 3 }] };

test('tool-work accounting excludes only the declared startup readiness GET', () => {
  const startup = { request: 'GET /v1/models', token: 'same-runtime' };
  const work = [{ request: 'POST /v1/evaluate', token: 'same-runtime' },
    { request: 'GET /api/v3/repos/octo/demo/contents/file', token: 'same-runtime' },
    { request: 'GET /package', token: null }];
  const raw = [startup, ...work];
  assert.deepEqual(withoutReadinessPings(raw), work);
  assert.deepEqual(raw, [startup, ...work], 'raw startup and execution evidence stays intact');
});

function context({ surfaces, server, cwd, records, timings }) {
  const validators = contractValidators();
  const driver = createDriver({ validate: validators.validateOutput, validateInput: validators.validateInput,
    tools: new Set(contract.tools.map(entry => entry.name)), cliOnly: new Set(['ghCloneRepo']),
    schemaOf: name => contract.tools.find(entry => entry.name === name).outputSchema });
  return advancedContext({ name: tool, cwd, advancedCalls: [], suite: { surfaces, driver, ...validators },
    record: (label, ok, detail) => records.push({ label, ok, detail }),
    callOn: surface => async (name, input) => {
      const result = surface === 'cli' ? await surfaces.cliCall(name, input, { cwd, credentialTool: tool }) : await surfaces.mcpCall(server, name, input);
      timings.push({ surface, ms: result.ms });
      return result;
    },
  });
}

async function checkScenario(ctx, surfaces, surface, label) {
  const before = surfaces.ctx.mock.requestsFrom.length;
  let cursor;
  await ctx.scenario(label, surface, async () => {
    const report = await ctx.explore(tool, { queries: [guide], responseChars: 1 }, surface);
    assert.deepEqual(report.failures, []);
    assert.equal(report.responses.length, 3);
    cursor = report.responses[1].input;
    assert.equal(typeof cursor.cursor, 'string');
    const another = await ctx.call({ queries: [{ owner: 'octo', repo: 'demo', path: 'src/greeter.ts' }] }, surface);
    assert.ok(another.json.results[0].data.content.includes('function greet'));
  });
  const requests = surfaces.ctx.mock.requestsFrom.slice(before);
  assert.ok(requests.some(entry => entry.request.includes('/contents/guide.txt')));
  assert.ok(requests.some(entry => decodeURIComponent(entry.request).includes('/contents/src/greeter.ts')));
  const tokens = new Set(requests.map(entry => entry.token));
  assert.equal(tokens.size, 1, 'seed and subsequent acquisition retain one scenario credential');
  return { token: [...tokens][0], cursor };
}

test('independent scenarios isolate credentials while issued cursors retain access and accounting', { timeout: 60000 }, async () => {
  const cwd = path.join(FIXTURES, 'tool-health');
  const surfaces = await startSurfaces(cwd);
  let server;
  try {
    server = await surfaces.mcpServer(tool, cwd);
    const records = [], timings = [];
    const ctx = context({ surfaces, server, cwd, records, timings });
    for (const surface of ['cli', 'mcp']) {
      const first = await checkScenario(ctx, surfaces, surface, 'first');
      const second = await checkScenario(ctx, surfaces, surface, 'second');
      assert.notEqual(first.token, second.token, 'independent scenarios receive different credentials');
      const outside = surface === 'cli' ? await surfaces.cliCall(tool, first.cursor) : await surfaces.mcpCall(server, tool, first.cursor);
      assert.equal(outside.json.errorCode, 'responseSnapshotExpired', 'scenario credential is restored and cannot open another scope cursor');
    }
    assert.equal(timings.length, 16, 'all scenario seed and continuation calls use normal accounting');
    assert.equal(records.filter(record => record.label.startsWith('advanced.input.')).length, timings.length);
    assert.ok(records.every(record => record.ok), JSON.stringify(records.filter(record => !record.ok)));
    assert.deepEqual(surfaces.ctx.mock.unhandled, []);
  } finally { server?.close(); await surfaces.close(); }
});
