import { test } from 'node:test';
import assert from 'node:assert/strict';
import { DatabaseSync } from 'node:sqlite';
import { spawn } from 'node:child_process';
import type { ChildProcess } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync, appendFileSync, existsSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execPath } from 'node:process';
import { createHash } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import { binary, execFileSync, root, tempDir, waitFor, withReasoning, only, isChildFailure } from './helpers.ts';
import type { TestContext } from 'node:test';
import type { JsonRecord } from './json-record.ts';

/** What a crash scenario records so a failed run can be replayed from its artifacts. */
interface Evidence {
  name: string;
  processes: JsonRecord[];
  commands: JsonRecord[];
  sources: Record<string, string>;
  recoveries?: JsonRecord[];
  frames?: JsonRecord[];
  final?: JsonRecord;
  checkpoint?: JsonRecord;
  passed?: boolean;
}

const fixtureScript = fileURLToPath(new URL('./fixtures/crash-boundary.ts', import.meta.url));
const skipped = process.platform === 'win32';
const boundaries = ['before-submit', 'uncertain-submit', 'after-submit-before-ack', 'during-complete'];
const hash = (file: string) => createHash('sha256').update(readFileSync(file)).digest('hex');

function setup(t: TestContext, name: string) {
  const workspace = tempDir('cc-crash-');
  const database = join(workspace, 'protocol.sqlite');
  const artifacts = process.env.COMMUNICATION_CRASH_ARTIFACT_DIR ? join(resolve(process.env.COMMUNICATION_CRASH_ARTIFACT_DIR), name) : null;
  if (artifacts) mkdirSync(artifacts, { recursive: true });
  const evidence: Evidence = { name, processes: [], commands: [], sources: { ...Object.fromEntries(['store.ts', 'dispatch.ts', 'database.ts', 'transport.ts'].map(file => [file, hash(join(root, 'src/runtime', file))])), 'crash-boundaries.test.ts': hash(fileURLToPath(import.meta.url)), 'crash-boundary.ts': hash(fixtureScript) } };
  const args = (command: string, input: Record<string, unknown>, session?: string) => [command, JSON.stringify(withReasoning(command, input)), '--workspace', workspace, '--database', database, ...(session ? ['--session', session] : [])];
  const call = (command: string, input: Record<string, unknown> = {}, session?: string): JsonRecord => {
    const entry: JsonRecord = { at: Date.now(), command, input, session };
    try { entry.output = execFileSync(binary, args(command, input, session), { encoding: 'utf8', timeout: 10000, stdio: ['pipe', 'pipe', 'pipe'] }); return JSON.parse(entry.output); }
    catch (error) { entry.error = String(error); assert.ok(isChildFailure(error)); entry.stdout = String(error.stdout ?? ''); entry.stderr = String(error.stderr ?? ''); throw error; }
    finally { evidence.commands.push(entry); if (artifacts) appendFileSync(join(artifacts, 'commands.jsonl'), `${JSON.stringify(entry)}\n`); }
  };
  const sender = call('join', { name: 'sender', vendor: 'fixture' });
  const receiver = call('join', { name: 'receiver', vendor: 'claude', vendorSession: 'fixture-session' });
  const healthy = call('join', { name: 'healthy', vendor: 'claude', vendorSession: 'healthy-session' });
  for (const actor of [sender, receiver, healthy]) call('heartbeat', { ttlMs: 600000 }, actor.id);
  const db = new DatabaseSync(database);
  db.exec('PRAGMA busy_timeout=5000');
  const endpoint = join(workspace, 'peer.sock');
  const framesFile = join(workspace, 'frames.jsonl');
  writeFileSync(framesFile, '');
  const frames = () => readFileSync(framesFile, 'utf8').split('\n').filter(Boolean).map((line): JsonRecord => JSON.parse(line));
  const children: ChildProcess[] = [];
  const child = (mode: string, marker: string, operationArgs: string[]) => {
    const process = spawn(execPath, [fixtureScript, mode, marker, ...operationArgs], { stdio: ['ignore', 'pipe', 'pipe'] });
    const record = { pid: process.pid, mode, stdout: '', stderr: '' };
    evidence.processes.push(record); children.push(process);
    process.stdout.on('data', chunk => { record.stdout += chunk; });
    process.stderr.on('data', chunk => { record.stderr += chunk; });
    process.on('exit', (code, signal) => Object.assign(record, { code, signal }));
    return process;
  };
  const stop = async (process: ChildProcess) => { if (process.exitCode === null && process.signalCode === null) process.kill('SIGKILL'); await waitFor(() => process.exitCode !== null || process.signalCode !== null); };
  const server = async (suffix: string) => { const marker = join(workspace, `server-${suffix}.json`); const process = child('server', marker, [endpoint, framesFile]); await waitFor(() => existsSync(marker)); return process; };
  const recoveryOwner = async () => { evidence.recoveries ??= []; const marker = join(workspace, `recovery-${evidence.recoveries.length}.json`); const process = child('recovery-owner', marker, [database, workspace, receiver.id]); await waitFor(() => existsSync(marker)); await waitFor(() => process.exitCode !== null || process.signalCode !== null); const result = JSON.parse(readFileSync(marker, 'utf8')); evidence.recoveries.push(result); return result; };
  t.after(async () => {
    for (const process of children) await stop(process);
    if (artifacts) {
      evidence.frames = frames();
      evidence.final = { messages: db.prepare('SELECT * FROM messages ORDER BY id').all(), deliveries: db.prepare('SELECT * FROM deliveries ORDER BY message,recipient').all(), dispatches: db.prepare('SELECT * FROM dispatches ORDER BY message,recipient').all(), integrity: db.prepare('PRAGMA integrity_check').get() };
      writeFileSync(join(artifacts, 'result.json'), JSON.stringify(evidence, null, 2));
      const exported = execFileSync(binary, ['db', 'export', JSON.stringify({ path: join(artifacts, 'database.sqlite') }), '--workspace', workspace, '--database', database], { encoding: 'utf8', timeout: 10000 });
      writeFileSync(join(artifacts, 'database-export.json'), exported);
    }
    db.close();
    rmSync(workspace, { recursive: true, force: true });
  });
  const sent = call('send_message', { to: receiver.id, question: `crash:${name}`, key: 'request', conversationId: name }, sender.id);
  const receipt = () => only<JsonRecord>(db.prepare('SELECT * FROM dispatches WHERE message=? AND recipient=?').get(sent.id, receiver.id));
  const delivery = () => only<JsonRecord>(db.prepare('SELECT * FROM deliveries WHERE message=? AND recipient=?').get(sent.id, receiver.id));
  async function pause(boundary: string, kill?: true): Promise<JsonRecord>;
  async function pause(boundary: string, kill: false): Promise<{ checkpoint: JsonRecord; process: ChildProcess }>;
  async function pause(boundary: string, kill = true) {
    const marker = join(workspace, 'paused.json');
    const command = boundary === 'during-complete' ? args('complete', { message: sent.id, reply:{conclusion:`answer:${name}`}, outcome: 'done' }, receiver.id) : args('dispatch', {}, receiver.id);
    const process = child(boundary, marker, command);
    await waitFor(() => existsSync(marker), { message: `Production boundary ${boundary} not reached` });
    const checkpoint = JSON.parse(readFileSync(marker, 'utf8'));
    evidence.checkpoint = checkpoint;
    if (artifacts) writeFileSync(join(artifacts, 'checkpoint.json'), JSON.stringify(checkpoint, null, 2));
    if (kill) await stop(process);
    return kill ? checkpoint : { checkpoint, process };
  }
  const health = async (expected?: string) => { await delay(1050); const state = call('health', { staleAfterMs: 1000 }); if (expected) assert.ok(state.issues.some((issue: JsonRecord) => issue.message === sent.id && issue.issue === expected), JSON.stringify(state)); return state; };
  const inspect = () => { call('binding', {}, receiver.id); return call('fetch', { current: true }, receiver.id); };
  const noReplay = () => { const before = frames().length; assert.equal(call('dispatch', {}, receiver.id).submitted, 0); assert.equal(call('dispatch', {}, receiver.id).submitted, 0); assert.equal(frames().length, before); };
  const unrelated = async () => { call('attach', { transport: 'claude', endpoint, vendorSession: 'healthy-session' }, healthy.id); const next = call('send_message', { to: healthy.id, question: `healthy:${name}`, key: 'healthy' }, sender.id); assert.equal(call('dispatch', {}, healthy.id).submitted, 1); await waitFor(() => frames().some((row: JsonRecord) => row.frame.message?.content.includes(`healthy:${name}`))); const reply = call('complete', { message: next.id, outcome:'done',reply:{conclusion:`healthy-answer:${name}`} }, healthy.id); assert.equal(reply.completed, true); };
  const complete = () => { const input = { message: sent.id, reply:{conclusion:`answer:${name}`}, outcome: 'done' }; const first = call('complete', input, receiver.id); assert.equal(first.completed, true); assert.equal(call('complete', input, receiver.id).id, first.id); assert.throws(() => call('complete', { ...input, reply:{conclusion:'conflicting answer'} }, receiver.id), /different content/); const answer = db.prepare('SELECT * FROM messages WHERE replyTo=?').all(sent.id); assert.equal(answer.length, 1); const [stored] = answer; assert.equal(only<JsonRecord>(stored).conversationId, name); assert.equal(only<JsonRecord>(stored).target, sender.id); assert.ok(delivery().acknowledgedAt); return first; };
  return { ...evidence, evidence, call, sender, receiver, healthy, db, server, recoveryOwner, pause, health, inspect, noReplay, unrelated, complete, stop, receipt, delivery, frames, sent, endpoint, name };
}

test('explicit acknowledged orphan recovery permits a fresh managed owner without falsifying its submit receipt', { skip: skipped, timeout: 120000 }, async t => {
  const f = setup(t, 'acknowledged-new-owner');
  await f.server('initial');
  f.call('attach', { transport: 'claude', endpoint: f.endpoint, vendorSession: 'fixture-session' }, f.receiver.id);
  await f.pause('uncertain-submit');
  await waitFor(() => f.frames().length === 1);
  const prior = f.receipt();
  f.complete();
  assert.match((await f.recoveryOwner()).error, /staged delivery/);
  f.call('attach', { transport: 'claude', endpoint: f.endpoint, vendorSession: 'fixture-session' }, f.receiver.id);
  const recovery = await f.recoveryOwner();
  assert.equal(recovery.error, undefined, JSON.stringify(recovery));
  assert.equal(recovery.recovered, true);
  const after = f.receipt();
  assert.equal(after.token, prior.token);
  assert.equal(after.attemptedAt, prior.attemptedAt);
  assert.equal(after.submittedAt, null);
  assert.equal(after.state, 'uncertain');
  assert.ok(after.error.includes('acknowledged'));
  f.noReplay();
  assert.equal(f.call('retry_delivery', { message: f.sent.id, reason: 'This request is already complete' }, f.receiver.id).ready, false);
  f.evidence.passed = true;
});

test('explicit unchanged raw attach recovers completed orphan stages before fresh managed startup', { skip: skipped, timeout: 120000 }, async t => {
  const f = setup(t, 'acknowledged-raw-owner');
  f.call('attach', { transport: 'raw' }, f.receiver.id);
  const offered = f.call('hook', { format: 'json', deferConfirm: true }, f.receiver.id);
  assert.equal(offered.items[0].id, f.sent.id);
  const prior = f.receipt();
  assert.equal(prior.state, 'staged');
  f.call('attach', { transport: 'raw' }, f.receiver.id);
  assert.equal(f.receipt().state, 'staged');
  f.complete();
  assert.match((await f.recoveryOwner()).error, /staged delivery/);
  f.call('attach', { transport: 'raw' }, f.receiver.id);
  const recovery = await f.recoveryOwner();
  assert.equal(recovery.error, undefined, JSON.stringify(recovery));
  assert.equal(f.receipt().state, 'uncertain');
  assert.equal(f.receipt().token, prior.token);
  assert.equal(f.receipt().submittedAt, null);
  assert.equal(f.call('hook', { format: 'json', deferConfirm: true }, f.receiver.id).items.length, 0);
  assert.equal(f.call('retry_delivery', { message: f.sent.id, reason: 'Already handled' }, f.receiver.id).ready, false);
  const baseline = f.call('health', { staleAfterMs: 1000 });
  const pending = f.call('send_message', { to: f.healthy.id, question: 'unrelated-pending', key: 'health-pending' }, f.sender.id);
  const health = await f.health();
  assert.equal(health.counts.uncertain, 0);
  assert.equal(health.counts.queuedAction, baseline.counts.queuedAction + 1);
  assert.equal(health.status, 'pending');
  assert.ok(!health.issues.some((issue: JsonRecord) => issue.message === f.sent.id));
  assert.equal(only<JsonRecord>(f.db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=?').get(pending.id)).acknowledgedAt, null);
  f.evidence.passed = true;
});

test('acknowledgement does not permit rebinding while the real staged delivery owner is still alive', { skip: skipped, timeout: 120000 }, async t => {
  const f = setup(t, 'acknowledged-live-owner');
  await f.server('initial');
  f.call('attach', { transport: 'claude', endpoint: f.endpoint, vendorSession: 'fixture-session' }, f.receiver.id);
  const paused = await f.pause('uncertain-submit', false);
  await waitFor(() => f.frames().length === 1);
  f.complete();
  f.call('attach', { transport: 'claude', endpoint: f.endpoint, vendorSession: 'fixture-session' }, f.receiver.id);
  assert.equal(f.receipt().state, 'staged');
  assert.throws(() => f.call('attach', { transport: 'claude', endpoint: f.endpoint, vendorSession: 'replacement-session' }, f.receiver.id), /staged delivery/);
  assert.equal(f.receipt().state, 'staged');
  await f.stop(paused.process);
  f.call('attach', { transport: 'claude', endpoint: f.endpoint, vendorSession: 'replacement-session' }, f.receiver.id);
  f.noReplay();
  f.evidence.passed = true;
});

for (const boundary of boundaries) {
  test(`real crash recovery ${boundary}`, { skip: skipped, timeout: 120000 }, async t => {
    const f = setup(t, boundary);
    let server = await f.server('initial');
    f.call('attach', { transport: 'claude', endpoint: f.endpoint, vendorSession: 'fixture-session' }, f.receiver.id);
    if (boundary === 'during-complete') assert.equal(f.call('dispatch', {}, f.receiver.id).submitted, 1);
    const checkpoint = await f.pause(boundary);
    if (boundary === 'before-submit') {
      assert.equal(f.receipt().state, 'staged'); assert.equal(f.frames().length, 0);
      await f.health('stalledOffer'); f.inspect(); f.noReplay();
      assert.throws(() => f.call('attach', { transport: 'claude', endpoint: f.endpoint, vendorSession: 'rebound-session' }, f.receiver.id), /staged delivery/);
      f.call('leave', {}, f.receiver.id); f.call('resume', { vendor: 'claude' }, f.receiver.id);
      f.noReplay();
      assert.equal(f.call('retry_delivery', { message: f.sent.id, reason: 'OS checkpoint and socket log prove this process died before any submit' }, f.receiver.id).ready, true);
      f.call('attach', { transport: 'claude', endpoint: f.endpoint, vendorSession: 'rebound-session' }, f.receiver.id);
      assert.equal(f.call('dispatch', {}, f.receiver.id).submitted, 1);
      await waitFor(() => f.frames().length === 1);
      f.complete(); await f.unrelated();
    } else if (boundary === 'uncertain-submit') {
      await waitFor(() => f.frames().length === 1);
      assert.equal(f.receipt().state, 'staged'); assert.equal(f.delivery().acknowledgedAt, null);
      await f.health('stalledOffer'); f.inspect(); f.noReplay();
      await f.unrelated(); f.complete();
      assert.equal(f.call('retry_delivery', { message: f.sent.id, reason: 'Already handled; never replay this accepted request' }, f.receiver.id).ready, false);
      f.call('attach', { transport: 'claude', endpoint: f.endpoint, vendorSession: 'rebound-after-handling' }, f.receiver.id);
      f.noReplay();
    } else if (boundary === 'after-submit-before-ack') {
      await waitFor(() => f.frames().length === 1);
      assert.equal(f.receipt().state, 'submitted'); assert.equal(f.delivery().acknowledgedAt, null);
      await f.stop(server); await f.health('overdueHandling'); f.inspect();
      server = await f.server('replacement');
      f.call('attach', { transport: 'claude', endpoint: f.endpoint, vendorSession: 'replacement-session' }, f.receiver.id);
      f.noReplay();
      assert.throws(() => f.call('retry_delivery', { message: f.sent.id, reason: 'Do not replay submitted context' }, f.receiver.id), /already submitted/);
      f.complete(); await f.unrelated();
    } else {
      assert.equal(checkpoint.inTransaction, true); assert.equal(checkpoint.uncommittedReplies.length, 1); assert.equal(checkpoint.uncommittedAcknowledged, 1);
      assert.equal(only<JsonRecord>(f.db.prepare('SELECT count(*) AS n FROM messages WHERE replyTo=?').get(f.sent.id)).n, 0);
      assert.equal(f.delivery().acknowledgedAt, null); assert.equal(f.receipt().state, 'submitted');
      f.inspect(); f.noReplay(); await f.unrelated(); f.complete();
      assert.equal(only<JsonRecord>(f.db.prepare('PRAGMA integrity_check').get()).integrity_check, 'ok');
    }
    f.evidence.passed = true;
  });
}
