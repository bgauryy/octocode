import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { randomUUID } from 'node:crypto';
import { binary, tempWorkspace } from './helpers.mjs';

// Hot coordination paths must use indexed lookups, not scans of every lease or audit row.
// Bounds allow for debug builds and loaded CI.
const BOUND_MS = 1500;

test('lock, check_paths, context and documents stay indexed with 10^4 leases and 10^5 audit rows', t => {
  const workspace = tempWorkspace(t, 'communication-scale-', { real: true });
  const database = join(workspace, 'scale.sqlite');
  const run = (session, command, input) => {
    const started = performance.now();
    const output = execFileSync(binary, [command, JSON.stringify(input), '--workspace', workspace, '--database', database, ...(session ? ['--session', session] : [])], { encoding: 'utf8', maxBuffer: 1 << 26 });
    return { ms: performance.now() - started, bytes: output.length, value: JSON.parse(output) };
  };
  const owner = run(null, 'join', { name: 'owner', vendor: 'raw' }).value.id;
  const peer = run(null, 'join', { name: 'peer', vendor: 'raw' }).value.id;
  const db = new DatabaseSync(database);
  db.exec('BEGIN');
  const note = db.prepare("INSERT INTO audit(session,kind,entityId,at,data,key) VALUES(?,'document.created',?,?,?,?)");
  note.run(owner, 'early.md', Date.now(), JSON.stringify({ name: 'early.md', author: owner, context: { summary: 'Early fact', path: 'd3', kind: 'tree', expiresAt: Date.now() + 3600000 } }), 'early.md');
  const session = db.prepare('INSERT INTO sessions(id,workspace,name,vendor,expiresAt) VALUES(?,?,?,?,1)');
  const others = Array.from({ length: 1000 }, () => randomUUID());
  for (const id of others) session.run(id, workspace, 'history', 'raw');
  const message = db.prepare("INSERT INTO messages(sender,target,body,key,expiresAt,reasoning,wake) VALUES(?,?,?,?,1,'r','passive')");
  const delivery = db.prepare('INSERT INTO deliveries(message,recipient,acknowledgedAt) VALUES(?,?,1)');
  for (let i = 0; i < 50000; i++) delivery.run(message.run(others[i % 1000], others[(i + 1) % 1000], 'b', `k${i}`).lastInsertRowid, others[(i + 1) % 1000]);
  const lease = db.prepare("INSERT INTO leases(workspace,path,kind,owner,expiresAt,reasoning,pathKey) VALUES(?,?,'file',?,?,'r',?)");
  for (let i = 0; i < 10000; i++) {
    const path = join(workspace, `d${i % 16}`, `f${i}.txt`);
    lease.run(workspace, path, owner, Date.now() + 600000, path.toLowerCase());
  }
  db.exec('COMMIT');
  assert.ok(db.prepare('SELECT count(*) n FROM audit').get().n >= 100000);
  db.close();
  const timings = {};
  const measure = (label, session, command, input) => { const result = run(session, command, input); timings[label] = Math.round(result.ms); assert.ok(result.ms < BOUND_MS, `${label} took ${result.ms} ms`); return result; };
  run(owner, 'heartbeat', {}); run(peer, 'heartbeat', {});
  const granted = measure('lock_many', peer, 'lock_many', { reasoning: 'Scale fixture', paths: Array.from({ length: 32 }, (_, i) => ({ path: `free/f${i}.txt` })) });
  assert.equal(granted.value.ok, true); assert.equal(granted.value.leases[0].path, 'free/f0.txt');
  const denied = measure('lock conflict', peer, 'lock', { reasoning: 'Scale fixture', path: 'D15/F9999.TXT' });
  assert.equal(denied.value.ok, false); assert.equal(denied.value.conflict.path, 'd15/f9999.txt');
  const checked = measure('check_paths', peer, 'check_paths', { paths: Array.from({ length: 32 }, (_, i) => ({ path: `d15/f${9999 - 16 * i}.txt` })) });
  assert.equal(checked.value.conflicts.length, 32);
  const tree = measure('check_paths tree', peer, 'check_paths', { paths: [{ path: '.', kind: 'tree' }] });
  assert.equal(tree.value.conflicts.length, 100); assert.equal(tree.value.truncated, true); assert.ok(tree.bytes < 64 * 1024);
  const context = measure('context', peer, 'context', { path: 'd3/f3.txt' });
  assert.deepEqual(context.value.items.map(item => item.name), ['early.md']); assert.equal(context.value.next ?? null, null);
  measure('share_document', owner, 'share_document', { name: 'late.md', content: 'proof', reasoning: 'Scale fixture' });
  assert.throws(() => run(peer, 'read_document', { name: 'late' }), /late\.md/);
  t.diagnostic(JSON.stringify(timings));
});
