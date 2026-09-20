import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

const here = dirname(fileURLToPath(import.meta.url));
const root = mkdtempSync(join(tmpdir(), 'jev-proxy-selftest-'));
const clients = [];
const ordinary = ['ghSearch', 'ghGetFileContent', 'ghSearchHistory', 'ghGetHistoryItem', 'artifactSearch'];
async function connect(arm, id) {
  const dir = join(root, id);
  mkdirSync(dir);
  const configPath = join(dir, 'config.json');
  writeFileSync(configPath, JSON.stringify({ version: 1, arm, entrypoint: join(here, 'fixtures/fake-mcp.mjs'),
    runDir: dir, cwd: dir, requestTimeoutMs: 5000 }));
  const env = { PATH: process.env.PATH, HOME: dir, OCTOCODE_HOME: dir,
    JEV_BENCH_CONFIG: configPath, OCTOCODE_JEV_KEY: 'fixture-key-no-provider' };
  const client = new Client({ name: 'proxy-selftest', version: '1' }, { capabilities: {} });
  const transport = new StdioClientTransport({ command: process.execPath, args: [join(here, 'mcp-proxy.mjs')], env, stderr: 'pipe' });
  transport.stderr?.resume();
  clients.push(client);
  await client.connect(transport);
  return { client, log: () => readFileSync(join(dir, 'calls.jsonl'), 'utf8').trim().split('\n').map(line => JSON.parse(line)) };
}
const call = (client, name, args) => client.callTool({ name, arguments: args });
const inline = value => ({ context: { value }, question: { type: 'noul', text: 'Fixture?' } });
try {
  const { client, log } = await connect('candidate', 'candidate');
  const catalog = await client.listTools();
  assert.deepEqual(catalog.tools.map(tool => tool.name), [...ordinary, 'jev']);
  assert.equal(catalog.tools[0].inputSchema.properties.queries.type, 'array');
  assert.equal(client.getInstructions(), `Fixture enabled names: ${[...ordinary, 'jev'].join(',')}`);
  const echo = await call(client, 'ghSearch', { queries: [{ query: 'fixture' }] });
  assert.deepEqual(echo, { content: [{ type: 'text', text: 'untouched\nfixture response' }],
    structuredContent: { calls: 1, echo: { queries: [{ query: 'fixture' }] } }, _meta: { fixture: true } });
  const forbidden = await call(client, 'jev', { queries: [{ context: { tool: 'localFetch', query: {} } }] });
  assert.equal(forbidden.isError, true);
  assert.match(forbidden.content[0].text, /benchmarkScopeRejected/);
  const tooMany = await call(client, 'ghSearch', { queries: Array.from({ length: 40 }, () => ({})) });
  assert.match(tooMany.content[0].text, /benchmarkBudgetExceeded/);
  const stillSecond = await call(client, 'ghGetFileContent', {});
  assert.equal(stillSecond.structuredContent.calls, 2, 'rejections must never reach downstream');
  await call(client, 'jev', { queries: [inline('same'), inline('same'), inline('same')] });
  const shared = log().filter(row => row.event === 'call').at(-1).providerUsage;
  assert.equal(shared.inputTokens, 12);
  assert.equal(shared.outputTokens, 3);
  assert.equal(shared.allocations.length, 1);
  await call(client, 'jev', { queries: [inline('mixed'), inline('mixed'), inline('mixed')] });
  const mixed = log().filter(row => row.event === 'call').at(-1).providerUsage;
  assert.equal(mixed.inputTokens, 12, 'successful owner covers malformed group answer consumption');
  assert.equal(mixed.allocations[0].ownerIndex, 1);
  await call(client, 'jev', { queries: [inline('fail')] });
  const failed = log().filter(row => row.event === 'call').at(-1).providerUsage;
  assert.equal(failed.complete, false);
  assert.equal(failed.inputTokens, null);
  assert.deepEqual(failed.unknownRows, [0]);
  await call(client, 'jev', { queries: [{ context: { tool: 'ghSearch', query: { query: 'nested' } } }] });
  const nested = log().filter(row => row.event === 'call').at(-1);
  assert.deepEqual(nested.cost, { ordinary: 1, jev: 1, nestedTools: ['ghSearch'] });
  assert.deepEqual(nested.countersAfter, { ordinary: 3, jev: 8 });
  const tooManyJev = await call(client, 'jev', { queries: Array.from({ length: 13 }, () => inline('same')) });
  assert.match(tooManyJev.content[0].text, /benchmarkBudgetExceeded/);
  await call(client, 'ghSearch', { queries: [{ query: 'fixture-key-no-provider' }] });
  assert.ok(!JSON.stringify(log()).includes('fixture-key-no-provider'), 'credentials must not enter receipts');
  assert.equal(log().filter(row => row.event === 'callStarted').length, 7);

  const baseline = await connect('baseline', 'baseline');
  assert.deepEqual((await baseline.client.listTools()).tools.map(tool => tool.name), ordinary);
  assert.equal((await call(baseline.client, 'jev', inline('x'))).isError, true);
  console.log('Proxy selftest passed: real stdio, catalog/instructions, response fidelity, scope, quotas, nested cost, shared/unknown usage, credential redaction.');
} finally {
  await Promise.allSettled(clients.map(client => client.close()));
  rmSync(root, { recursive: true, force: true });
}
