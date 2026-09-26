// Deterministic transport/SQLite fixtures, not proof of vendor outage recovery.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, chmodSync, mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, realpathSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { DatabaseSync } from 'node:sqlite';
import { installedBinary } from './artifact-checks.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const source = resolve(process.env.COMMUNICATION_BINARY ?? installedBinary());
const cycles = Number(process.env.COMMUNICATION_CYCLES ?? 30);
assert.ok(Number.isSafeInteger(cycles) && cycles >= 5 && cycles <= 200, 'COMMUNICATION_CYCLES must be 5..200');
const output = resolve(process.env.COMMUNICATION_OUTPUT ?? join(root, '../../.octocode/benchmarks/communication-recovery', new Date().toISOString().replaceAll(':', '-'), 'result.json'));
mkdirSync(dirname(output), {recursive: true});
const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'communication-recovery-')));
const binary = join(workspace, 'frozen-runtime');
copyFileSync(source, binary); chmodSync(binary, 0o755);
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const started = performance.now();
const report = {schemaVersion: 1, startedAt: new Date().toISOString(), platform: `${process.platform}-${process.arch}`, binarySha256: hash(readFileSync(binary)), harnessSha256: hash(readFileSync(fileURLToPath(import.meta.url))), cyclesRequested: cycles, passed: false, modelCalls: 0, cycles: [], failures: [], cleanupFailures: [], offers: []};
const database = join(workspace, 'coord.sqlite');
const flags = ['--workspace', workspace, '--database', database];
const childEnv = {...process.env};
for (const key of ['OPENCODE_SERVER_PASSWORD', 'OPENCODE_SERVER_USERNAME', 'OCTOCODE_OPENCODE_AUTH_ENDPOINT']) delete childEnv[key];
const children = new Set();
const allowedRetry = new Set();
const sentIds = new Set();
let db, sender, receiver, server, mode = 'ok', busy = false, preflights = 0;
const args = (command, input = {}, session) => [command, JSON.stringify(input), ...flags, ...(session ? ['--session', session] : [])];
function run(argv, timeoutMs = 10000) {
  const child = spawn(binary, argv, {env: childEnv, stdio: ['ignore', 'pipe', 'pipe']});
  const task = {child, stdout: '', stderr: ''};
  children.add(task);
  child.stdout.on('data', data => { task.stdout = (task.stdout + data).slice(-65536); });
  child.stderr.on('data', data => { task.stderr = (task.stderr + data).slice(-8192); });
  const timer = setTimeout(() => child.kill('SIGKILL'), timeoutMs);
  task.done = new Promise((res, rej) => {
    child.once('error', rej);
    child.once('close', (code, signal) => res({code, signal, stdout: task.stdout, stderr: task.stderr}));
  }).finally(() => { clearTimeout(timer); children.delete(task); });
  return task;
}
async function call(command, input = {}, session) {
  const result = await run(args(command, input, session)).done;
  assert.equal(result.code, 0, `${command}: ${result.stderr}`);
  return JSON.parse(result.stdout);
}
async function stop(task, signal = 'SIGTERM') {
  if (task.child.exitCode !== null || task.child.signalCode !== null) return task.done;
  task.child.kill(signal);
  const timer = setTimeout(() => task.child.kill('SIGKILL'), 1500);
  try { return await task.done; } finally { clearTimeout(timer); }
}
async function until(predicate, label) {
  const deadline = performance.now() + 5000;
  while (!predicate()) {
    if (performance.now() > deadline) throw Error(`Timeout: ${label}`);
    await delay(15);
  }
}
const dispatch = id => db.prepare('SELECT * FROM dispatches WHERE message=? AND recipient=?').get(id, receiver);
const offerCount = id => report.offers.filter(offer => offer.message === id).length;
const listen = () => run(['listen', ...flags, '--session', receiver, '--duration-ms', '6000'], 8000);
async function send(label) {
  const result = await call('send_message', {to: receiver, body: label, wake: 'action', reasoning: 'Exercise recovery ordering in an isolated transport fixture'}, sender);
  sentIds.add(result.id);
  return result.id;
}
async function visible(id, state) {
  assert.equal(dispatch(id)?.state, state);
  assert.equal(db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=? AND recipient=?').get(id, receiver).acknowledgedAt, null, 'Submission never implies handling');
  assert.ok((await call('inbox', {}, receiver)).items.some(message => message.id === id), `Message ${id} remains inspectable`);
}
async function handled(id) { await visible(id, 'submitted'); assert.equal((await call('ack', {message: id}, receiver)).acknowledged, true); }
async function retry(id) {
  const token = dispatch(id).token;
  allowedRetry.add(id);
  assert.equal((await call('retry_delivery', {message: id, reason: 'Fixture receiver state inspected; explicitly accept one duplicate offer'}, receiver)).ready, true);
  assert.equal((await call('dispatch', {}, receiver)).submitted, 1);
  assert.notEqual(dispatch(id).token, token, 'Explicit retry rotates the attempt token');
  assert.equal(offerCount(id), 2);
  await handled(id);
}
try {
  sender = (await call('join', {name: 'soak-sender', vendor: 'raw'})).id;
  receiver = (await call('join', {name: 'soak-recipient', vendor: 'opencode'})).id;
  db = new DatabaseSync(database);
  server = createServer(async (req, res) => {
    try {
      const url = new URL(req.url, 'http://localhost');
      assert.equal(url.searchParams.get('directory'), workspace);
      if (req.method === 'GET') {
        preflights++;
        if (mode === 'drop-preflight') { res.destroy(); return; }
        res.setHeader('content-type', 'application/json');
        res.end(JSON.stringify(url.pathname === '/session/status' ? busy ? {ses_soak: {type: 'busy'}} : {} : {id: 'ses_soak', directory: workspace}));
        return;
      }
      assert.equal(req.method, 'POST'); assert.equal(url.pathname, '/session/ses_soak/prompt_async');
      let text = ''; for await (const chunk of req) { text += chunk; assert.ok(text.length < 65536); }
      const body = JSON.parse(text); assert.equal(body.noReply, false);
      const context = body.parts[0].text;
      const messages = JSON.parse(context.slice(context.indexOf('\n') + 1));
      for (const message of messages) {
        assert.equal(dispatch(message.id).state, 'staged', 'Durable staging precedes external offer');
        report.offers.push({message: message.id, token: dispatch(message.id).token, mode});
      }
      if (mode === 'drop-after-offer') { res.destroy(); return; }
      if (mode === 'hold-after-offer') return;
      res.statusCode = 204; res.end();
    } catch (error) { report.failures.push(`Fake server: ${error.message}`); res.destroy(); }
  });
  await new Promise((res, rej) => { server.once('error', rej); server.listen(0, '127.0.0.1', res); });
  await call('attach', {transport: 'opencode', endpoint: `http://127.0.0.1:${server.address().port}`, vendorSession: 'ses_soak'}, receiver);
  const names = ['busy-stop-restart', 'disconnect-before-stage', 'uncertain-explicit-retry', 'killed-staged-listener', 'reconnect-writer-contention'];
  for (let i = 0; i < cycles; i++) {
    const cycleStart = performance.now(), scenario = names[i % names.length];
    mode = 'ok'; busy = false;
    await call('heartbeat', {}, sender); await call('heartbeat', {}, receiver);
    if (scenario === 'busy-stop-restart') {
      busy = true; const id = await send(`${i}: wait for idle`), before = preflights;
      const first = listen(); await until(() => preflights >= before + 2, 'busy preflight');
      assert.equal((await stop(first)).code, 0); assert.equal(dispatch(id), undefined); assert.equal(offerCount(id), 0);
      assert.ok((await call('inbox', {}, receiver)).items.some(message => message.id === id));
      busy = false; const restarted = listen(); await until(() => dispatch(id)?.state === 'submitted', 'restarted listener');
      assert.equal((await stop(restarted)).code, 0); await handled(id);
    } else if (scenario === 'disconnect-before-stage') {
      const id = await send(`${i}: preflight disconnect`); mode = 'drop-preflight';
      assert.notEqual((await run(args('dispatch', {}, receiver)).done).code, 0);
      assert.equal(dispatch(id), undefined); assert.equal(offerCount(id), 0);
      assert.ok((await call('inbox', {}, receiver)).items.some(message => message.id === id));
      mode = 'ok'; assert.equal((await call('dispatch', {}, receiver)).submitted, 1); await handled(id);
    } else if (scenario === 'uncertain-explicit-retry' || scenario === 'killed-staged-listener') {
      const id = await send(`${i}: lost receipt`);
      if (scenario === 'uncertain-explicit-retry') {
        mode = 'drop-after-offer'; assert.notEqual((await run(args('dispatch', {}, receiver)).done).code, 0);
        await visible(id, 'uncertain');
      } else {
        mode = 'hold-after-offer'; const worker = listen(); await until(() => offerCount(id) === 1, 'offered before abrupt stop');
        assert.equal((await stop(worker, 'SIGKILL')).signal, 'SIGKILL'); server.closeAllConnections(); await visible(id, 'staged');
      }
      mode = 'ok'; assert.equal((await call('dispatch', {}, receiver)).submitted, 0); assert.equal(offerCount(id), 1);
      const restarted = run(['listen', ...flags, '--session', receiver, '--duration-ms', '300']);
      assert.equal((await restarted.done).code, 0); assert.equal(offerCount(id), 1, 'Restart cannot replay an uncertain or staged attempt');
      await visible(id, scenario === 'uncertain-explicit-retry' ? 'uncertain' : 'staged');
      await retry(id);
    } else {
      const firstId = await send(`${i}: before reconnect`), worker = listen();
      await until(() => dispatch(firstId)?.state === 'submitted', 'first connection');
      server.closeAllConnections();
      db.exec('BEGIN IMMEDIATE');
      const pending = run(args('send_message', {to: receiver, body: `${i}: after reconnect`, wake: 'action', reasoning: 'Verify progress after the database writer releases'}, sender));
      try { await delay(100); assert.equal(pending.child.exitCode, null); } finally { db.exec('ROLLBACK'); }
      const result = await pending.done; assert.equal(result.code, 0, result.stderr);
      const secondId = JSON.parse(result.stdout).id; sentIds.add(secondId);
      await until(() => dispatch(secondId)?.state === 'submitted', 'connection recovered');
      assert.equal((await stop(worker)).code, 0); await handled(firstId); await handled(secondId);
    }
    report.cycles.push({index: i, scenario, passed: true, durationMs: performance.now() - cycleStart});
  }
  for (const id of sentIds) assert.equal(offerCount(id), allowedRetry.has(id) ? 2 : 1, `Unexpected duplicate or missing external offer for ${id}`);
  assert.equal(db.prepare('SELECT count(*) n FROM messages').get().n, sentIds.size);
  assert.equal(db.prepare('SELECT count(*) n FROM deliveries WHERE acknowledgedAt IS NULL').get().n, 0);
  assert.equal(hash(readFileSync(binary)), report.binarySha256);
  assert.equal(report.failures.length, 0);
  report.passed = true;
} catch (error) { report.failures.push(error.stack); process.exitCode = 1; }
finally {
  for (const task of [...children]) try { await stop(task); } catch (error) { report.cleanupFailures.push(error.message); }
  if (server) { server.closeAllConnections(); await new Promise(res => server.close(res)); }
  if (db?.isTransaction) db.exec('ROLLBACK');
  for (const id of [sender, receiver].filter(Boolean)) try { await call('leave', {}, id); } catch (error) { report.cleanupFailures.push(error.message); }
  if (db) {
    report.acknowledgedMessages = db.prepare('SELECT count(*) n FROM deliveries WHERE acknowledgedAt IS NOT NULL').get().n;
    report.pendingMessages = db.prepare('SELECT count(*) n FROM deliveries WHERE acknowledgedAt IS NULL').get().n;
    report.dispatchStates = db.prepare('SELECT state,count(*) n FROM dispatches GROUP BY state ORDER BY state').all();
  }
  db?.close();
  report.messages = sentIds.size;
  report.explicitRetries = allowedRetry.size;
  report.externalOffers = report.offers.length;
  report.unexpectedDuplicateOffers = [...sentIds].reduce((n, id) => n + Math.max(0, offerCount(id) - (allowedRetry.has(id) ? 2 : 1)), 0);
  report.childrenRemaining = children.size;
  report.durationMs = performance.now() - started;
  report.passed &&= report.cleanupFailures.length === 0 && children.size === 0;
  if (!report.passed) process.exitCode = 1;
  rmSync(workspace, {recursive: true, force: true});
  writeFileSync(output, JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({...report, offers: undefined, cycles: undefined, output}));
}
