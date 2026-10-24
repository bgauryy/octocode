// Hot coordination paths use indexed lookups, not scans of every lease, record, message or delivery: every statement the
// store runs for them is checked with EXPLAIN QUERY PLAN against a seeded database. The wall-clock variant over 10^5
// records is opt-in (yarn test:nightly, tests/nightly/storage-timing.test.ts).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { randomUUID } from 'node:crypto';
import type { JsonRecord } from './json-record.ts';
import { fixture } from './store-support.ts';

/** Tables that grow with history; a hot path may search them through an index but never scan them. */
const LARGE = /^SCAN (leases|records|messages|deliveries|documents|dispatches)\b/;
/** The index each hot command must search by (assets/schema.sql); a dropped or bypassed index fails here. */
const SEARCHES: Record<string, RegExp[]> = {
  lock: [/^SEARCH leases USING (COVERING )?INDEX leases_path /],
  lock_many: [/^SEARCH leases USING (COVERING )?INDEX leases_path /],
  check_paths: [/^SEARCH leases USING (COVERING )?INDEX leases_path /],
  context: [/^SEARCH d USING (COVERING )?INDEX documents_scan /],
  fetch: [/USING (COVERING )?INDEX records_type /, /USING (COVERING )?INDEX records_entity /, /^SCAN records_search VIRTUAL TABLE/, /USING (COVERING )?INDEX deliveries_inbox /],
};

/** Two thousand messages, deliveries and live leases, a hundred expired sessions and one scoped document note. */
function seed(db: DatabaseSync, workspace: string, owner: string) {
  db.exec('BEGIN');
  db.prepare("INSERT INTO records(path,[from],type,entityId,timestamp,data,key) VALUES(?,?,'document',?,?,?,?)")
    .run(workspace, owner, 'early.md', Date.now(), JSON.stringify({ name: 'early.md', author: owner, context: { summary: 'Early fact', path: 'd3', kind: 'tree', expiresAt: Date.now() + 3600000 } }), 'early.md');
  const others = Array.from({ length: 100 }, () => randomUUID());
  const session = db.prepare('INSERT INTO sessions(id,workspace,name,vendor,expiresAt) VALUES(?,?,?,?,1)');
  for (const id of others) session.run(id, workspace, 'history', 'raw');
  const message = db.prepare("INSERT INTO messages(sender,target,body,key,expiresAt,reasoning,wake) VALUES(?,?,?,?,1,'r','passive')");
  const delivery = db.prepare('INSERT INTO deliveries(message,recipient,acknowledgedAt) VALUES(?,?,1)');
  for (let i = 0; i < 2000; i++) delivery.run(message.run(others[i % 100], others[(i + 1) % 100], 'b', `k${i}`).lastInsertRowid, others[(i + 1) % 100]);
  const lease = db.prepare("INSERT INTO leases(workspace,path,kind,owner,expiresAt,reasoning,pathKey) VALUES(?,?,'file',?,?,'r',?)");
  for (let i = 0; i < 2000; i++) {
    const path = join(workspace, `d${i % 16}`, `f${i}.txt`);
    lease.run(workspace, path, owner, Date.now() + 600000, path.toLowerCase());
  }
  db.exec('COMMIT');
}

/** The query plan of every statement that reads or changes rows, grouped by the command that ran it. */
function plans(db: DatabaseSync, statements: readonly { command: string; sql: string; args: readonly unknown[] }[]) {
  const byCommand = new Map<string, string[]>();
  for (const statement of statements) {
    if (!/^\s*(SELECT|UPDATE|DELETE|WITH)\b/i.test(statement.sql)) continue;
    // The store binds booleans as 0/1; EXPLAIN takes the same values.
    const args = statement.args.map(value => typeof value === 'boolean' ? Number(value) : value);
    const details = (db.prepare('EXPLAIN QUERY PLAN ' + statement.sql).all(...(args as never[])) as { detail: string }[]).map(row => `${row.detail}\t${statement.sql.replace(/\s+/g, ' ')}`);
    byCommand.set(statement.command, [...byCommand.get(statement.command) ?? [], ...details]);
  }
  return byCommand;
}

test('coordination and unified fetch search large tables only through indexes', async t => {
  const f = fixture(t);
  const owner = await f.join('owner'), peer = await f.join('peer');
  const db = new DatabaseSync(f.file);
  t.after(() => db.close());
  seed(db, f.workspace, owner);

  // Record every statement the store runs, labelled by the command that ran it.
  const statements: { command: string; sql: string; args: readonly unknown[] }[] = [];
  let command = '';
  const store = f.store.db;
  for (const method of ['get', 'all', 'run'] as const) {
    const original = store[method].bind(store);
    store[method] = ((sql: string, args: readonly unknown[] = []) => { statements.push({ command, sql, args }); return original(sql, args as never); }) as never;
  }
  const run = (session: string, name: string, input: Record<string, unknown>) => { command = name; return f.call(session, name, input); };

  const granted = await run(peer, 'lock_many', { paths: Array.from({ length: 32 }, (_, i) => ({ path: `free/f${i}.txt` })) });
  assert.equal(granted.ok, true); assert.equal(granted.leases[0].path, 'free/f0.txt');
  const denied = await run(peer, 'lock', { path: 'D15/F1999.TXT' });
  assert.equal(denied.ok, false); assert.equal(denied.conflict.path, 'd15/f1999.txt');
  const checked = await run(peer, 'check_paths', { paths: Array.from({ length: 32 }, (_, i) => ({ path: `d15/f${1999 - 16 * i}.txt` })) });
  assert.equal(checked.conflicts.length, 32);
  const tree = await run(peer, 'check_paths', { paths: [{ path: '.', kind: 'tree' }] });
  assert.ok(tree.conflicts.length > 0 && tree.conflicts.length <= 100);
  assert.equal(tree.next.command, 'check_paths'); assert.ok(Buffer.byteLength(JSON.stringify(tree)) < 17 * 1024);
  const continuation = await run(peer, tree.next.command, tree.next.input);
  assert.ok(continuation.conflicts[0].id > tree.conflicts.at(-1).id);
  const context = await run(peer, 'context', { path: 'd3/f3.txt' });
  assert.deepEqual(context.items.map((item: JsonRecord) => item.name), ['early.md']); assert.equal(context.next ?? null, null);
  await run(owner, 'share_document', { name: 'late.md', content: 'proof' });
  await assert.rejects(run(peer, 'read_document', { name: 'late' }), /late\.md/);
  const mail = await run(owner, 'send_message', { to: peer, body: 'Indexed incoming evidence' });
  const incoming = await run(peer, 'fetch', { incoming: true });
  assert.deepEqual(incoming.items.map((item: JsonRecord) => item.data.messageId), [mail.id]);
  const memory = await run(owner, 'record', { type: 'memory', branch: 'scale', data: { content: 'Unique search needle', category: 'performance' } });
  const searched = await run(peer, 'fetch', { type: 'memory', branch: 'scale', search: '"search needle"', where: { category: 'performance' } });
  assert.deepEqual(searched.items, [memory]);
  const selected = await run(peer, 'fetch', { type: 'message', where: { messageId: mail.id } });
  assert.equal(selected.items[0].data.body, 'Indexed incoming evidence');

  const byCommand = plans(db, statements);
  const scans = [...byCommand].flatMap(([name, details]) => details.filter(detail => LARGE.test(detail)).map(detail => `${name}: ${detail}`));
  assert.deepEqual(scans, [], 'hot paths scan no large table');
  for (const [name, searches] of Object.entries(SEARCHES)) {
    for (const search of searches) assert.ok(byCommand.get(name)?.some(detail => search.test(detail)), `${name} searches ${search}:\n${(byCommand.get(name) ?? []).join('\n')}`);
  }
});
