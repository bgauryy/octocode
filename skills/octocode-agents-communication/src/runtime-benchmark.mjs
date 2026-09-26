import assert from 'node:assert/strict';
import { execFileSync, execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { performance } from 'node:perf_hooks';
import { promisify } from 'node:util';
import { DatabaseSync } from 'node:sqlite';

// Opt-in, model-free measurements. Build --release first; results include CLI startup.
const root = fileURLToPath(new URL('../', import.meta.url));
const cli = join(root, 'scripts/agents-communication');
const samples = Number(process.env.COMMUNICATION_SAMPLES ?? 40);
assert.ok(Number.isInteger(samples) && samples >= 5 && samples <= 1000, 'samples must be 5..1000');
const output = resolve(process.env.COMMUNICATION_OUTPUT ?? join(root, '../../.octocode/benchmarks/communication-runtime/results', new Date().toISOString().replaceAll(':', '-'), 'result.json'));
const workspace = join(dirname(output), 'workspace');
mkdirSync(workspace, { recursive: true });
const database = join(workspace, 'communication.sqlite');
const run = promisify(execFile), measurements = {}, identities = [];
const args = (command, input, session) => [command, JSON.stringify(input), '--workspace', workspace, '--database', database, ...(session ? ['--session', session] : [])];
const call = (command, input = {}, session) => execFileSync(cli, args(command, input, session), { encoding: 'utf8', timeout: 10000 });
const json = (command, input = {}, session) => JSON.parse(call(command, input, session));
function measured(name, operation) {
  const start = performance.now(), value = operation();
  (measurements[name] ??= []).push(performance.now() - start);
  return value;
}
const digest = value => createHash('sha256').update(value).digest('hex');
const target = execFileSync('rustc', ['-vV'], { encoding: 'utf8', timeout: 10000 }).match(/^host: (.+)$/m)?.[1];
assert.ok(target, 'Rust host target must be available for binary provenance');
const manifest = {
  goal: 'Measure local CLI storage, delivery and lease latency with correctness guards; no models.',
  sampleCount: samples, repetitions: 1, threshold: null,
  decision: 'Descriptive microbenchmark, not a before/after or generalization claim.',
  guards: ['identical retries store once', 'hooks do not repeat offered messages', 'idle hooks inject zero bytes', 'competing lease sets have one winner and no partial reservations'],
  node: process.version, platform: `${process.platform}/${process.arch}`,
  harnessHash: digest(readFileSync(fileURLToPath(import.meta.url))),
  binaryHash: digest(readFileSync(join(root, 'scripts/bin', target, `octocode-agents-communication${process.platform === 'win32' ? '.exe' : ''}`))),
  skillHash: digest(readFileSync(join(root, 'SKILL.md'))),
  startedAt: new Date().toISOString(),
};
writeFileSync(join(dirname(output), 'manifest.json'), JSON.stringify(manifest, null, 2));
let db, result;
try {
  for (const name of ['sender', 'receiver']) identities.push(json('join', { name, vendor: 'raw' }).id);
  const [sender, receiver] = identities;
  json('attach', { transport: 'raw' }, receiver);
  db = new DatabaseSync(database, { readOnly: true });
  let injectedBytes = 0;
  for (let i = 0; i < samples; i++) {
    const input = { to: receiver, body: `Change ${i} is ready for review.`, key: `sample-${i}`, wake: 'passive', reasoning: 'Hand off the completed change for review' };
    const sent = measured('send', () => json('send_message', input, sender));
    assert.deepEqual(measured('sendRetry', () => json('send_message', input, sender)), sent);
    const offered = measured('hookDelivery', () => call('hook', { format: 'json' }, receiver));
    injectedBytes += Buffer.byteLength(offered);
    assert.deepEqual(JSON.parse(offered).items.map(item => item.id), [sent.id]);
    assert.equal(measured('hookIdle', () => call('hook', { format: 'text' }, receiver)), '');
    measured('ack', () => json('ack', { message: sent.id }, receiver));
    const lease = measured('lock', () => json('lock', { path: `file-${i}`, reasoning: 'Reserve the file for a targeted change' }, sender));
    assert.equal(lease.ok, true);
    const conflict = measured('lockConflict', () => json('check_paths', { paths: [{ path: `file-${i}` }] }, receiver));
    assert.equal(conflict.ok, false);
    measured('unlock', () => json('unlock', { leaseId: lease.lease.id }, sender));
    if (i % 10 === 0) for (const id of identities) json('heartbeat', {}, id);
  }
  const raceCount = Math.min(samples, 20);
  for (let i = 0; i < raceCount; i++) {
    const paths = [{ path: `race-${i}-source` }, { path: `race-${i}-destination` }];
    const started = performance.now();
    const outcomes = await Promise.all(identities.map((session, n) => run(cli, args('lock_many', {
      paths: n ? [...paths].reverse() : paths, reasoning: 'Reserve both rename endpoints together',
    }, session), { encoding: 'utf8', timeout: 10000 }).then(r => JSON.parse(r.stdout))));
    (measurements.competingLockSets ??= []).push(performance.now() - started);
    assert.equal(outcomes.filter(r => r.ok).length, 1);
    assert.deepEqual(outcomes.find(r => !r.ok).heldLeaseIds, []);
    const winner = outcomes.find(r => r.ok);
    assert.equal(winner.leases.length, 2);
    const owner = identities[outcomes.indexOf(winner)];
    for (const lease of winner.leases) json('unlock', { leaseId: lease.id }, owner);
  }
  assert.equal(db.prepare('SELECT count(*) n FROM messages').get().n, samples);
  assert.equal(db.prepare('SELECT count(*) n FROM deliveries WHERE acknowledgedAt IS NULL').get().n, 0);
  assert.equal(db.prepare('SELECT count(*) n FROM leases').get().n, 0);
  const metrics = Object.fromEntries(Object.entries(measurements).map(([name, values]) => {
    const sorted = [...values].sort((a, b) => a - b);
    const percentile = p => sorted[Math.ceil(p * sorted.length) - 1];
    return [name, { n: values.length, p50Ms: percentile(.5), p95Ms: percentile(.95), maxMs: sorted.at(-1), samplesMs: values }];
  }));
  result = { passed: true, metrics, injectedBytes, emptyHookBytes: 0, duplicateMessages: 0, modelCalls: 0, raceCount, manifest };
} catch (error) {
  result = { passed: false, error: error.stack, manifest };
  process.exitCode = 1;
} finally {
  for (const id of identities) {
    try { json('leave', {}, id); } catch (error) { result.cleanupError = error.message; result.passed = false; process.exitCode = 1; }
  }
  db?.close();
  writeFileSync(output, JSON.stringify(result, null, 2));
  console.log(JSON.stringify({ passed: result.passed, output, error: result.error, metrics: result.metrics && Object.fromEntries(Object.entries(result.metrics).map(([name, { samplesMs, ...summary }]) => [name, summary])) }));
}
