// Deterministic protocol replay: measures wire bytes/calls, not provider tokens.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import assert from 'node:assert/strict';
import { DatabaseSync } from 'node:sqlite';

const [baselinePath, candidatePath, outputPath] = process.argv.slice(2);
if (!baselinePath || !candidatePath || !outputPath) throw Error('Usage: node scripts/context-batching-benchmark.mjs BASELINE CANDIDATE OUTPUT');
const sha = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const result = { kind: 'deterministic CLI replay; no inference or token estimate', hashes: { baseline: sha(baselinePath), candidate: sha(candidatePath), harness: sha(new URL(import.meta.url)) }, cases: [] };
for (const count of [1, 6, 16, 33]) {
  const pair = { messages: count };
  for (const [arm, path] of [['baseline', baselinePath], ['candidate', candidatePath]]) {
    const workspace = mkdtempSync(join(tmpdir(), 'communication-context-replay-'));
    const database = join(workspace, 'audit.sqlite');
    const binary = resolve(path);
    const call = (command, input = {}, session) => JSON.parse(execFileSync(binary,
      [command, JSON.stringify(input), '--workspace', workspace, '--database', database, ...(session ? ['--session', session] : [])],
      { encoding: 'utf8', timeout: 10000, stdio: 'pipe' }));
    let db;
    try {
      const sender = call('join', { name: 'sender', vendor: 'generic' }).id;
      const receiver = call('join', { name: 'receiver', vendor: 'generic' }).id;
      call('attach', { transport: 'raw' }, receiver);
      const ids = Array.from({ length: count }, (_, i) => call('send_message', { to: receiver, body: `Decision ${i}: preserve the shared interface.`, reasoning: 'Coordinate pending implementation work', key: `decision-${i}`, conversationId: 'review' }, sender).id);
      const batches = [], seen = []; let contextBytes = 0, ackCalls = 0;
      for (let attempt = 0; attempt <= count; attempt++) {
        const batch = call('hook', { format: 'json' }, receiver);
        if (!batch.items.length) break;
        const batchIds = batch.items.map(x => x.id); seen.push(...batchIds); batches.push(batchIds.length);
        contextBytes += Buffer.byteLength(batch.context);
        for (const item of batch.items) assert.equal(item.body, `Decision ${ids.indexOf(item.id)}: preserve the shared interface.`);
        if (arm === 'candidate') { call('ack', { messages: batchIds }, receiver); ackCalls++; }
        else for (const id of batchIds) { call('ack', { message: id }, receiver); ackCalls++; }
      }
      assert.deepEqual(seen, ids);
      assert.equal(call('hook', { format: 'json' }, receiver).context, '');
      db = new DatabaseSync(database, { readOnly: true });
      assert.equal(db.prepare('SELECT count(*) AS n FROM deliveries WHERE acknowledgedAt IS NULL').get().n, 0);
      assert.equal(db.prepare("SELECT count(*) AS n FROM audit WHERE kind='delivery.acknowledged'").get().n, count);
      pair[arm] = { batches, envelopes: batches.length, contextBytes, ackCalls, auditAcks: count };
    } finally { db?.close(); rmSync(workspace, { recursive: true, force: true }); }
  }
  pair.envelopeReduction = 1 - pair.candidate.envelopes / pair.baseline.envelopes;
  pair.contextByteReduction = 1 - pair.candidate.contextBytes / pair.baseline.contextBytes;
  pair.ackCallReduction = 1 - pair.candidate.ackCalls / pair.baseline.ackCalls;
  if (count > 1) assert.ok(pair.envelopeReduction >= 0.5);
  if (count === 16) assert.ok(pair.ackCallReduction >= 0.9);
  result.cases.push(pair);
}
result.passed = true;
writeFileSync(outputPath, JSON.stringify(result, null, 2) + '\n');
console.log(JSON.stringify(result));
