import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { installedBinary } from './artifact-checks.mjs';

// Size/query series, not a destructive retention or model benchmark.
const root = fileURLToPath(new URL('../', import.meta.url));
const binary = resolve(process.env.COMMUNICATION_BINARY ?? installedBinary());
const output = resolve(process.env.COMMUNICATION_OUTPUT ?? join(root, '../../.octocode/benchmarks/communication-storage/results', new Date().toISOString().replaceAll(':', '-'), 'result.json'));
const workspace = join(dirname(output), 'workspace'); mkdirSync(workspace, { recursive: true });
const database = join(workspace, 'store.sqlite'), binding = ['--workspace', workspace, '--database', database];
const hash = data => createHash('sha256').update(data).digest('hex');
const call = (command, input, session) => JSON.parse(execFileSync(binary, [command, JSON.stringify(input), ...binding, ...(session ? ['--session', session] : [])], { encoding: 'utf8', timeout: 10000, maxBuffer: 2 * 1024 * 1024 }));
const manifest = { startedAt: new Date().toISOString(), binarySha256: hash(readFileSync(binary)), harnessSha256: hash(readFileSync(fileURLToPath(import.meta.url))), messageCounts: [0, 1000, 10000], bodyBytes: 1024, querySamples: 30, modelCalls: 0, description: 'Single persistent MCP writer, unacknowledged self-deliveries retained; first inbox page p50/p95 plus one complete paginated read per size. Storage includes audit and WAL; descriptive single local run, no regression claim.' };
writeFileSync(join(dirname(output), 'manifest.json'), JSON.stringify(manifest, null, 2));
let id, child, closed, db, seq = 0, result = { passed: false, manifest, points: [] };
const pending = new Map();
function rpc(method, params = {}) {
  const requestId = ++seq;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { pending.delete(requestId); reject(Error(`MCP timeout ${method}`)); }, 15000);
    pending.set(requestId, { resolve: value => { clearTimeout(timer); resolve(value); }, reject: error => { clearTimeout(timer); reject(error); } });
    child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id: requestId, method, params })}\n`);
  });
}
async function tool(name, args) {
  const r = await rpc('tools/call', { name, arguments: args });
  assert.equal(r.isError, undefined, JSON.stringify(r));
  return JSON.parse(r.content[0].text);
}
const bytes = path => { try { return statSync(path).size; } catch (error) { if (error.code === 'ENOENT') return 0; throw error; } };
try {
  id = call('join', { name: 'storage-probe', vendor: 'raw' }).id;
  child = spawn(binary, ['mcp', ...binding, '--session', id]);
  closed = new Promise(resolve => child.once('close', resolve));
  let stderr = '';
  const fail = error => { for (const request of pending.values()) request.reject(error); pending.clear(); };
  child.on('error', fail); child.stdin.on('error', fail);
  child.stderr.on('data', data => { stderr = (stderr + data).slice(-8192); });
  child.once('close', code => fail(Error(`MCP closed ${code}: ${stderr}`)));
  createInterface({ input: child.stdout }).on('line', line => {
    try { const value = JSON.parse(line), task = pending.get(value.id); pending.delete(value.id); if (value.error) task?.reject(Error(JSON.stringify(value.error))); else task?.resolve(value.result); }
    catch (error) { fail(error); }
  });
  await rpc('initialize');
  db = new DatabaseSync(database, { readOnly: true });
  let count = 0;
  for (const targetCount of manifest.messageCounts) {
    const begin = performance.now();
    for (; count < targetCount; count++) {
      await tool('send_message', { to: id, replyRequired: false, body: 'x'.repeat(manifest.bodyBytes), key: `message-${count}`, wake: 'passive', reasoning: 'Measure retained message and audit growth' });
      if (count % 1000 === 0) call('heartbeat', {}, id);
    }
    const appendMs = performance.now() - begin, queryMs = [];
    for (let sample = 0; sample < manifest.querySamples; sample++) {
      const started = performance.now(), inbox = await tool('inbox', {}); queryMs.push(performance.now() - started);
      assert.ok(inbox.items.length <= Math.min(targetCount, 100));
      assert.equal(inbox.items.length === 0, targetCount === 0);
      assert.equal(inbox.next != null, inbox.items.length < targetCount);
    }
    const fullReadStart = performance.now(), ids = [];
    let page = await tool('inbox', {}), inboxPages = 0;
    for (;;) {
      inboxPages++;
      assert.ok(inboxPages <= targetCount + 1, 'Pagination must make bounded progress');
      for (const item of page.items) {
        assert.ok(ids.length === 0 || item.id > ids.at(-1), 'IDs must be strictly ordered without repeats');
        ids.push(item.id);
      }
      if (!page.next) break;
      assert.equal(page.next.command, 'inbox');
      page = await tool(page.next.command, page.next.input);
    }
    const inboxFullReadMs = performance.now() - fullReadStart;
    assert.equal(ids.length, targetCount);
    assert.equal(new Set(ids).size, targetCount);
    assert.deepEqual(ids, db.prepare('SELECT id FROM messages ORDER BY id').all().map(row => row.id));
    const snapshot = { messages: db.prepare('SELECT count(*) n FROM messages').get().n, deliveries: db.prepare('SELECT count(*) n FROM deliveries').get().n, audit: db.prepare('SELECT count(*) n FROM audit').get().n };
    assert.equal(snapshot.messages, targetCount); assert.equal(snapshot.deliveries, targetCount);
    queryMs.sort((a, b) => a - b);
    result.points.push({ ...snapshot, databaseBytes: bytes(database), walBytes: bytes(`${database}-wal`), appendMs, inboxPages, inboxFullReadMs, inboxVerifiedItems: ids.length, inboxP50Ms: queryMs[14], inboxP95Ms: queryMs[28], querySamplesMs: queryMs });
  }
  assert.equal(db.prepare('PRAGMA integrity_check').get().integrity_check, 'ok');
  assert.equal(hash(readFileSync(binary)), manifest.binarySha256);
  result.passed = true;
} catch (error) { result.error = error.stack; process.exitCode = 1; }
finally {
  if (child) { child.stdin.end(); const timer = setTimeout(() => child.kill('SIGKILL'), 2000); await closed; clearTimeout(timer); result.childExit = { code: child.exitCode, signal: child.signalCode }; if (child.exitCode !== 0) { result.passed = false; process.exitCode = 1; } }
  if (id) { try { call('leave', {}, id); } catch (error) { result.cleanupError = error.message; result.passed = false; process.exitCode = 1; } }
  db?.close();
  writeFileSync(output, JSON.stringify(result, null, 2));
  console.log(JSON.stringify({ output, ...result, points: result.points.map(({ querySamplesMs, ...point }) => point) }));
}
