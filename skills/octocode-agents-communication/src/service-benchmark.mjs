import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { installedBinary } from './artifact-checks.mjs';

// Same harness for a frozen baseline and candidate; no vendor processes or models.
const root = fileURLToPath(new URL('../', import.meta.url));
const binary = resolve(process.env.COMMUNICATION_BINARY ?? installedBinary());
const samples = Number(process.env.COMMUNICATION_SAMPLES ?? 60);
assert.ok(Number.isInteger(samples) && samples >= 10 && samples <= 1000);
const output = resolve(process.env.COMMUNICATION_OUTPUT ?? join(root, '../../.octocode/benchmarks/communication-service/results', new Date().toISOString().replaceAll(':', '-'), 'result.json'));
mkdirSync(dirname(output), { recursive: true });
const workspace = join(dirname(output), 'workspace');
mkdirSync(workspace, { recursive: true });
const database = join(workspace, 'communication.sqlite');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const binding = ['--workspace', workspace, '--database', database];
const call = (command, input, session) => JSON.parse(execFileSync(binary, [command, JSON.stringify(input), ...binding, ...(session ? ['--session', session] : [])], { encoding: 'utf8', timeout: 15000, stdio: ['pipe', 'pipe', 'pipe'] }));
const manifest = {
  startedAt: new Date().toISOString(), binary, binarySha256: hash(readFileSync(binary)),
  harnessSha256: hash(readFileSync(fileURLToPath(import.meta.url))), samples,
  primary: 'Warm persistent MCP send_message median latency; candidate <= 80% of frozen baseline.',
  guardrails: ['No lost messages or duplicate IDs', 'All deliveries acknowledged', 'CLI send p95 <= baseline * 1.25 + 5ms', 'No model calls'],
  design: 'Alternating CLI/MCP order within each sample; 5 warmups. Separate fresh stores per run. Whole-run AB/BA replication needed for a performance conclusion.',
  measurements: 'CLI includes startup; MCP excludes one-time startup, reported separately. Storage acknowledgement, not recipient inference.',
  limitations: 'Local exploratory microbenchmark; host load and filesystem caches uncontrolled. No agent quality or provider caching claim.',
  platform: `${process.platform}/${process.arch}`, node: process.version,
};
writeFileSync(join(dirname(output), 'manifest.json'), JSON.stringify(manifest, null, 2));
const timings = { cliSend: [], mcpSend: [], mcpRetry: [], mcpInbox: [], mcpComplete: [] };
const pending = new Map();
let child, childClosed, db, seq = 0, stderr = '', result, sender, receiver;
function request(method, params = {}) {
  const id = ++seq;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { pending.delete(id); reject(Error(`MCP timeout: ${method}; ${stderr}`)); }, 15000);
    pending.set(id, { resolve: value => { clearTimeout(timer); resolve(value); }, reject: error => { clearTimeout(timer); reject(error); } });
    child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
  });
}
async function tool(name, args) {
  const value = await request('tools/call', { name, arguments: args });
  assert.equal(value.isError, undefined, JSON.stringify(value));
  return JSON.parse(value.content[0].text);
}
async function measure(name, operation, keep) {
  const start = performance.now(), value = await operation();
  if (keep) timings[name].push(performance.now() - start);
  return value;
}
try {
  sender = call('join', { name: 'service-sender', vendor: 'benchmark' }).id;
  receiver = call('join', { name: 'service-receiver', vendor: 'benchmark' }).id;
  const start = performance.now();
  child = spawn(binary, ['mcp', ...binding, '--session', sender]);
  childClosed = new Promise(resolve => child.once('close', resolve));
  const fail = error => { for (const task of pending.values()) task.reject(error); pending.clear(); };
  child.on('error', fail);
  child.stdin.on('error', fail);
  child.once('close', code => fail(Error(`MCP closed (${code}): ${stderr}`)));
  child.stderr.on('data', data => { stderr = (stderr + data).slice(-8192); });
  createInterface({ input: child.stdout }).on('line', line => {
    try {
      const response = JSON.parse(line), task = pending.get(response.id);
      if (!task) return;
      pending.delete(response.id);
      if (response.error) task.reject(Error(JSON.stringify(response.error))); else task.resolve(response.result);
    } catch (error) { fail(error); }
  });
  await request('initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'service-benchmark', version: '1' } });
  result = { startupMs: performance.now() - start };
  const tools = await request('tools/list');
  assert.ok(tools.tools.some(tool => tool.name === 'send_message'));
  db = new DatabaseSync(database, { readOnly: true });
  for (let i = -5; i < samples; i++) {
    const keep = i >= 0;
    for (const mode of i % 2 === 0 ? ['cli', 'mcp'] : ['mcp', 'cli']) {
      // Send to self to measure the same bound receive and acknowledgement APIs.
      const args = { replyRequired: false, to: sender, key: `${mode}-${i}`, body: `Review change ${i}.`, reasoning: 'Request review of the completed change', wake: 'passive' };
      const sent = await measure(`${mode}Send`, () => mode === 'cli' ? call('send_message', args, sender) : tool('send_message', args), keep);
      assert.deepEqual(await measure('mcpRetry', () => tool('send_message', args), keep), sent);
      const inbox = await measure('mcpInbox', () => tool('inbox', {}), keep);
      assert.deepEqual(inbox.items.map(item => item.id), [sent.id]);
      assert.equal((await measure('mcpComplete', () => tool('complete', { message: sent.id }), keep)).completed, true);
    }
    if (i % 10 === 0) for (const id of [sender, receiver]) call('heartbeat', {}, id);
  }
  assert.equal(db.prepare('SELECT count(*) n FROM messages').get().n, (samples + 5) * 2);
  assert.equal(db.prepare('SELECT count(*) n FROM deliveries WHERE acknowledgedAt IS NULL').get().n, 0);
  assert.equal(hash(readFileSync(binary)), manifest.binarySha256, 'binary changed during run');
  result = { ...result, passed: true, modelCalls: 0, lostMessages: 0, duplicates: 0 };
} catch (error) {
  result = { ...result, passed: false, error: error.stack };
  process.exitCode = 1;
} finally {
  if (child) {
    child.stdin.end();
    const timer = setTimeout(() => child.kill('SIGKILL'), 2000);
    await childClosed;
    clearTimeout(timer);
    result.childExit = { code: child.exitCode, signal: child.signalCode };
    if (child.exitCode !== 0) { result.passed = false; process.exitCode = 1; }
  }
  for (const id of [sender, receiver].filter(Boolean)) {
    try { call('leave', {}, id); } catch (error) { result.cleanupError = error.message; result.passed = false; process.exitCode = 1; }
  }
  db?.close();
  result.metrics = Object.fromEntries(Object.entries(timings).map(([name, values]) => {
    const sorted = [...values].sort((a, b) => a - b);
    return [name, { n: values.length, p50Ms: sorted[Math.ceil(sorted.length * .5) - 1], p95Ms: sorted[Math.ceil(sorted.length * .95) - 1], samplesMs: values }];
  }));
  result.manifest = manifest;
  writeFileSync(output, JSON.stringify(result, null, 2));
  console.log(JSON.stringify({ output, ...result, metrics: Object.fromEntries(Object.entries(result.metrics).map(([name, { samplesMs, ...value }]) => [name, value])) }));
}
