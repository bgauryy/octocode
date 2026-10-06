import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from './helpers.mjs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { randomUUID } from 'node:crypto';
import { binary, tempWorkspace } from './helpers.mjs';

// Hot coordination paths must use indexed lookups, not scans of every lease or record.
// Bounds allow for debug builds and loaded CI.
const BOUND_MS = 1500;

test('coordination and unified fetch stay indexed with 10^4 leases and 10^5 records', t => {
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
  const note = db.prepare("INSERT INTO records(path,[from],type,entityId,timestamp,data,key) VALUES(?,?,'document',?,?,?,?)");
  note.run(workspace, owner, 'early.md', Date.now(), JSON.stringify({ name: 'early.md', author: owner, context: { summary: 'Early fact', path: 'd3', kind: 'tree', expiresAt: Date.now() + 3600000 } }), 'early.md');
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
  // Seeding can outlast the 60 s presence TTL on a loaded host; refresh both live identities as a heartbeat would.
  db.prepare('UPDATE sessions SET expiresAt=? WHERE id IN (?,?)').run(Date.now() + 60000, owner, peer);
  db.exec('COMMIT');
  assert.ok(db.prepare('SELECT count(*) n FROM records').get().n >= 100000);
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
  assert.ok(tree.value.conflicts.length > 0 && tree.value.conflicts.length <= 100);
  assert.equal(tree.value.next.command, 'check_paths'); assert.ok(tree.bytes < 17 * 1024);
  const continuation = measure('check_paths next', peer, tree.value.next.command, tree.value.next.input);
  assert.ok(continuation.value.conflicts[0].id > tree.value.conflicts.at(-1).id);
  const context = measure('context', peer, 'context', { path: 'd3/f3.txt' });
  assert.deepEqual(context.value.items.map(item => item.name), ['early.md']); assert.equal(context.value.next ?? null, null);
  measure('share_document', owner, 'share_document', { name: 'late.md', content: 'proof', reasoning: 'Scale fixture' });
  assert.throws(() => run(peer, 'read_document', { name: 'late' }), /late\.md/);
  const mail = run(owner, 'send_message', { to: peer, body: 'Indexed incoming evidence', reasoning: 'Scale fixture' }).value;
  const incoming = measure('fetch incoming', peer, 'fetch', { incoming: true });
  assert.deepEqual(incoming.value.items.map(item => item.data.messageId), [mail.id]);
  const memory = run(owner, 'record', { type: 'memory', branch: 'scale', data: { content: 'Unique search needle', category: 'performance' } }).value;
  const searched = measure('fetch search', peer, 'fetch', { type: 'memory', branch: 'scale', search: '"search needle"', where: { category: 'performance' } });
  assert.deepEqual(searched.value.items, [memory]);
  const selected = measure('fetch payload', peer, 'fetch', { type: 'message', where: { messageId: mail.id } });
  assert.equal(selected.value.items[0].data.body, 'Indexed incoming evidence');
  t.diagnostic(JSON.stringify(timings));
});
