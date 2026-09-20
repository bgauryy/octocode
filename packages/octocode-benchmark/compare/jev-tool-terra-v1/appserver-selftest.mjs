import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runAppServer, approvalFor } from './appserver-runner.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const dir = mkdtempSync(join(tmpdir(), 'jev-appserver-selftest-'));
const fixtureModel = process.env.BENCH_FIXTURE_MODEL ?? 'gpt-5.6-terra';
let posts = 0;
let toolName;
const provider = createServer(async (req, res) => {
  let body = ''; for await (const chunk of req) body += chunk;
  if (!req.url.endsWith('/responses')) { res.writeHead(404); res.end(); return; }
  const request = JSON.parse(body);
  posts++;
  const tools = request.tools ?? request.input.find(item => item.type === 'additional_tools')?.tools ?? [];
  toolName ??= tools.find(tool => tool.name?.endsWith('ghSearch'))?.name ?? 'mcp__octocode__ghSearch';
  const item = posts === 1 ? { id: 'fc_fixture', type: 'custom_tool_call', call_id: 'call_fixture', name: 'exec', namespace: 'functions',
    input: `text(await tools.${toolName}({queries:[{query:"offline"}]}));`, status: 'completed' }
    : { id: 'msg_fixture', type: 'message', role: 'assistant', phase: 'final_answer', status: 'completed',
      content: [{ type: 'output_text', text: JSON.stringify({ answer: 'fixture complete' }), annotations: [] }] };
  const response = { id: `resp_fixture_${posts}`, object: 'response', status: 'completed', model: 'gpt-5.6-terra', output: [item],
    usage: { input_tokens: 20, output_tokens: 5, input_tokens_details: { cached_tokens: 0 }, output_tokens_details: { reasoning_tokens: 0 }, total_tokens: 25 } };
  res.writeHead(200, { 'content-type': 'text/event-stream' });
  for (const event of [
    { type: 'response.created', response: { ...response, status: 'in_progress', output: [] } },
    { type: 'response.output_item.added', output_index: 0, item: { ...item, status: 'in_progress' } },
    { type: 'response.output_item.done', output_index: 0, item },
    { type: 'response.completed', response },
  ]) res.write(`event: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`);
  res.end();
});
await new Promise(resolve => provider.listen(0, '127.0.0.1', resolve));
try {
  const runDir = join(dir, 'run'); mkdirSync(runDir);
  // config/read expands omitted optional transport fields to null. The runner must
  // disable this inherited server without copying those fields into thread config.
  writeFileSync(join(dir, 'config.toml'), '[mcp_servers.fixture_unrelated]\ncommand = "/usr/bin/false"\nrequired = true\n');
  const configPath = join(dir, 'proxy.json');
  writeFileSync(configPath, JSON.stringify({ version: 1, arm: 'candidate', entrypoint: join(here, 'fixtures/fake-mcp.mjs'), runDir, cwd: dir, requestTimeoutMs: 5000 }));
  const result = await runAppServer({ cwd: dir, env: { PATH: process.env.PATH, HOME: dir, CODEX_HOME: dir,
    OCTOCODE_HOME: join(dir, 'octocode'), JEV_BENCH_CONFIG: configPath, OCTOCODE_JEV_KEY: 'fixture-no-provider', BENCH_MOCK_KEY: 'fixture-not-real' },
    model: fixtureModel, effort: 'medium', prompt: 'Use ghSearch once, then return the fixture result.',
    outputSchema: { type: 'object', properties: { answer: { type: 'string' } }, required: ['answer'], additionalProperties: false },
    runDir, proxyPath: join(here, 'mcp-proxy.mjs'), deadlineMs: 15000,
    modelProvider: 'benchmark_loopback', providerConfig: { name: 'offline loopback fixture', base_url: `http://127.0.0.1:${provider.address().port}/v1`,
      wire_api: 'responses', env_key: 'BENCH_MOCK_KEY', requires_openai_auth: false, supports_websockets: false, request_max_retries: 0, stream_max_retries: 0 } });
  console.log(JSON.stringify({ result, posts, artifactDirectory: dir }));
  if (result.exitCode !== 0 || result.approvals !== 1) {
    for (const name of ['runner-error.jsonl', 'thread-settings.jsonl', 'approvals.jsonl', 'stderr.log']) {
      try { console.log(name, readFileSync(join(runDir, name), 'utf8').slice(-5000)); } catch {}
    }
    const events = readFileSync(join(runDir, 'events.jsonl'), 'utf8').trim().split('\n').map(JSON.parse);
    console.log(JSON.stringify(events.filter(event => event.method === 'mcpServer/elicitation/request' || event.method === 'item/started' || event.method === 'turn/completed')));
  }
  assert.equal(result.exitCode, 0);
  assert.equal(posts, 2);
  assert.equal(result.approvals, 1);
  assert.equal(result.prohibitedToolEvents, 0);
  assert.deepEqual(JSON.parse(readFileSync(join(runDir, 'answer.json'))), { answer: 'fixture complete' });
  assert.equal(result.usage[0].input_tokens, 40);
  const calls = readFileSync(join(runDir, 'calls.jsonl'), 'utf8').trim().split('\n').map(JSON.parse);
  assert.equal(calls.filter(row => row.event === 'call' && row.admitted).length, 1);
  const events = readFileSync(join(runDir, 'events.jsonl'), 'utf8').trim().split('\n').map(JSON.parse);
  assert.ok(events.find(row => row.configRead)?.serverNames.includes('fixture_unrelated'));
  assert.equal(approvalFor({ serverName: 'evil', threadId: 't', mode: 'form' }, new Map(), 't'), null);
  const pending = new Map([['item', { server: 'octocode', tool: 'ghSearch', arguments: { query: 'x' } }]]);
  const approval = { serverName: 'octocode', threadId: 't', mode: 'form', message: 'Allow the octocode MCP server to run tool "ghSearch"?',
    requestedSchema: { type: 'object', properties: {} }, _meta: { codex_approval_kind: 'mcp_tool_call', tool_params: { query: 'x' } } };
  assert.ok(approvalFor(approval, pending, 't'));
  assert.equal(approvalFor({ ...approval, _meta: {} }, pending, 't'), null);
  assert.equal(approvalFor({ ...approval, message: 'Authenticate this server' }, pending, 't'), null);
  assert.equal(approvalFor(approval, new Map(), 't'), null);
  console.log('App-server offline approval selftest passed.');
} finally {
  await new Promise(resolve => provider.close(resolve));
  if (!process.env.KEEP_BENCH_FIXTURE) rmSync(dir, { recursive: true, force: true });
}
