import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { join } from 'node:path';
import test from 'node:test';
import type { TestContext } from 'node:test';
import type { SqlValue } from '../src/runtime/database.ts';
import type { JsonRecord } from './json-record.ts';
import { only } from './helpers.ts';
import { fixture, runtime } from './store-support.ts';

const { dispatch, dispatchOutcomes, hostHooks } = runtime;

async function pair(t: TestContext, vendor = 'generic') {
  const f = fixture(t);
  const sender = await f.join('sender');
  const receiver = (await f.call('', 'join', { name: 'receiver', vendor })).id;
  return { ...f, sender, receiver, send: (body: string, extra: Record<string, unknown> = {}) => f.call(sender, 'send_message', { to: 'receiver', body, ...extra }) };
}

const rows = (f: Pick<ReturnType<typeof fixture>, 'store'>, sql: string, ...params: SqlValue[]) => f.store.db.all(sql, params);

test('stage never replays a staged or uncertain offer; ready rows are offered again', async t => {
  const f = await pair(t);
  f.store.attach(f.receiver, { transport: 'raw' });
  const sent = await f.send('one fact');
  const first = f.store.stage(f.receiver, 'raw');
  assert.deepEqual(first.map(item => item.id), [sent.id]);
  assert.match(first[0].dispatchToken, /^[0-9a-f-]{36}$/);
  assert.deepEqual(f.store.stage(f.receiver, 'raw'), [], 'a staged offer is not replayed');
  f.store.finishDispatch(f.receiver, first, 'receipt lost');
  assert.equal(rows(f, 'SELECT state,error FROM dispatches')[0].state, 'uncertain');
  assert.deepEqual(f.store.stage(f.receiver, 'raw'), [], 'an uncertain offer is not replayed');
  assert.throws(() => f.store.finishDispatch(f.receiver, [{ ...first[0], dispatchToken: 'other' }]), /Unknown or superseded dispatch token/);
  assert.equal(f.store.retryDelivery(f.receiver, { message: sent.id, reason: 'operator retry' }).ready, true);
  const again = f.store.stage(f.receiver, 'raw');
  assert.equal(again.length, 1);
  f.store.releaseDispatch(f.receiver, again, dispatchOutcomes.BUDGET_DEFERRAL);
  assert.equal(rows(f, 'SELECT state FROM dispatches')[0].state, 'ready');
  const third = f.store.stage(f.receiver, 'raw');
  f.store.finishDispatch(f.receiver, third);
  f.store.finishDispatch(f.receiver, third);
  assert.equal(rows(f, 'SELECT state FROM dispatches')[0].state, 'submitted');
  assert.throws(() => f.store.retryDelivery(f.receiver, { message: sent.id, reason: 'again' }), /already submitted/);
});

test('context lists requests apart from notices and keeps the peer-data rule first', async t => {
  const f = await pair(t);
  f.store.attach(f.receiver, { transport: 'raw' });
  await f.send('plain fyi');
  const items = f.store.stage(f.receiver, 'raw');
  const text = dispatch.context(items);
  assert.ok(text.startsWith(dispatch.CONTEXT_RULE));
  assert.match(text, /Requests: \[\]\. Notices\/answers: \[\d+\]\./);
  assert.equal(dispatch.withPeers([], 'peers only'), 'peers only');
  assert.equal(dispatch.withPeers(items, 'peers').startsWith('peers ' + dispatch.CONTEXT_RULE), true);
});

test('attach validates the binding and refuses a second identity for one native receiver', async t => {
  const f = await pair(t);
  assert.throws(() => f.store.attach(f.receiver, { transport: 'opencode', endpoint: 'http://127.0.0.1:1' }), /vendorSession/);
  const bound = f.store.attach(f.receiver, { transport: 'opencode', endpoint: 'http://127.0.0.1:1', vendorSession: 'ses_x' });
  assert.equal(bound.transport, 'opencode');
  assert.equal(bound.capabilities.acceptanceReceipt, 'http');
  const other = await f.join('other');
  assert.throws(() => f.store.attach(other, { transport: 'opencode', endpoint: 'http://127.0.0.1:1', vendorSession: 'ses_x' }), /already registered as/);
  assert.throws(() => f.store.attach(f.receiver, { transport: 'grok', endpoint: '/tmp/none.sock', vendorSession: 'not-a-uuid' }), /ENOENT|socket/);
  await f.send('staged');
  f.store.stage(f.receiver, 'opencode');
  assert.throws(() => f.store.attach(f.receiver, { transport: 'raw' }), /staged delivery/);
});

async function opencode(t: TestContext, workspace: string, { fail = false } = {}) {
  const posts: { url?: string; body: JsonRecord }[] = [];
  const server = createServer((request, response) => {
    const chunks: Buffer[] = [];
    request.on('data', chunk => chunks.push(chunk));
    request.on('end', () => {
      const body = chunks.length ? JSON.parse(Buffer.concat(chunks).toString()) : null;
      const json = (value: unknown) => { response.writeHead(200, { 'Content-Type': 'application/json' }); response.end(JSON.stringify(value)); };
      const url = request.url ?? '';
      if (request.method === 'POST') {
        posts.push({ url: request.url, body });
        if (fail) { response.writeHead(500); return response.end(); }
        if (url.includes('prompt_async')) { response.writeHead(204); return response.end(); }
        return json({ info: { sessionID: 'ses_a', role: 'user', id: 'msg_1' }, parts: [{ type: 'text', text: body.parts[0].text }] });
      }
      if (url.startsWith('/session/status')) return json({});
      json({ id: 'ses_a', directory: workspace });
    });
  });
  t.after(() => { server.closeAllConnections(); server.close(); });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  assert.ok(address && typeof address === 'object');
  return { endpoint: `http://127.0.0.1:${address.port}`, posts };
}

test('once delivers a batch through a native endpoint and records one submitted receipt', async t => {
  const f = await pair(t);
  const { endpoint, posts } = await opencode(t, f.workspace);
  f.store.attach(f.receiver, { transport: 'opencode', endpoint, vendorSession: 'ses_a' });
  const clients = new dispatch.DeliveryClients();
  t.after(() => clients.disconnect());
  assert.deepEqual(await dispatch.once(f.store, f.receiver, clients), { submitted: 0 });
  const sent = await f.send('native fact');
  const result = await dispatch.once(f.store, f.receiver, clients);
  assert.deepEqual(result, { submitted: 1, messages: [sent.id], modelCalls: 0, transport: 'opencode', receipt: 'http', recipientTurnRequested: false });
  assert.equal(posts.length, 1);
  assert.match(posts[0].body.parts[0].text, /native fact/);
  assert.match(posts[0].body.parts[0].text, new RegExp(dispatch.CONTEXT_RULE.slice(0, 20)));
  assert.equal(rows(f, 'SELECT state FROM dispatches')[0].state, 'submitted');
  assert.deepEqual(await dispatch.once(f.store, f.receiver, clients), { submitted: 0 });
  assert.equal(posts.length, 1, 'a submitted message is never resent');
});

test('a failed offer is uncertain, surfaced as Transient and never auto-replayed', async t => {
  const f = await pair(t);
  const { endpoint, posts } = await opencode(t, f.workspace, { fail: true });
  f.store.attach(f.receiver, { transport: 'opencode', endpoint, vendorSession: 'ses_a' });
  const clients = new dispatch.DeliveryClients();
  t.after(() => clients.disconnect());
  await f.send('will fail');
  await assert.rejects(dispatch.once(f.store, f.receiver, clients), error => error instanceof dispatch.Transient && /HTTP 500; inspect before retrying/.test(error.message));
  const [row] = rows(f, 'SELECT state,error FROM dispatches');
  assert.equal(row.state, 'uncertain');
  assert.match(String(row.error), /HTTP 500/);
  assert.deepEqual(await dispatch.once(f.store, f.receiver, clients), { submitted: 0 });
  assert.equal(posts.length, 1);
});

test('an unreachable endpoint is Transient and stages nothing', async t => {
  const f = await pair(t);
  f.store.attach(f.receiver, { transport: 'opencode', endpoint: 'http://127.0.0.1:1', vendorSession: 'ses_a' });
  await f.send('no listener');
  const clients = new dispatch.DeliveryClients();
  await assert.rejects(dispatch.once(f.store, f.receiver, clients), dispatch.Transient);
  assert.equal(rows(f, 'SELECT * FROM dispatches').length, 0);
});

test('dispatch holds the delivery-owner lock for the session while it runs', async t => {
  const f = await pair(t);
  const { endpoint } = await opencode(t, f.workspace);
  f.store.attach(f.receiver, { transport: 'opencode', endpoint, vendorSession: 'ses_a' });
  await f.send('one shot');
  const held = await runtime.ownerLock.claimDeliveryOwner(f.store.database, f.receiver);
  await assert.rejects(dispatch.dispatch(f.open(), f.receiver), /Another delivery owner/);
  assert.equal(rows(f, 'SELECT * FROM dispatches').length, 0, 'a refused owner stages nothing');
  await held.release();
  assert.equal((await dispatch.dispatch(f.store, f.receiver)).submitted, 1);
  assert.equal(runtime.ownerLock.deliveryOwnerLive(f.store.database, f.receiver), false, 'released on return');
});

test('the hook command stages raw mail once and can leave the receipt for the host', async t => {
  const f = await pair(t);
  f.store.attach(f.receiver, { transport: 'raw' });
  const sent = await f.send('hook fact');
  const out: JsonRecord[] = [];
  const io = { output: (value: JsonRecord) => out.push(value), print: (text: string) => out.push({ text }) };
  await dispatch.hook(f.store, f.receiver, { format: 'json', deferConfirm: true }, io);
  assert.equal(out[0].items[0].id, sent.id);
  assert.match(out[0].items[0].dispatchToken, /^[0-9a-f-]{36}$/);
  assert.match(out[0].context, /hook fact/);
  assert.equal(rows(f, 'SELECT state FROM dispatches')[0].state, 'staged');
  f.store.finishDispatch(f.receiver, [{ id: sent.id, dispatchToken: out[0].items[0].dispatchToken }]);
  await dispatch.hook(f.store, f.receiver, { format: 'json' }, io);
  assert.deepEqual(out[1], { items: [] });
  await assert.rejects(dispatch.hook(f.store, f.receiver, { consumer: 'x', format: 'text', managed: false }, io).then(() => f.store.attach(f.receiver, { transport: 'opencode', endpoint: 'http://127.0.0.1:1', vendorSession: 'ses_z' })).then(() => dispatch.hook(f.store, f.receiver, {}, io)), /Hook requires a raw attachment/);
});

function hookRuntime(events: JsonRecord[]) {
  const output: JsonRecord[] = [];
  return {
    output,
    runtime: { output: (value: JsonRecord) => output.push(value), open: (path: string, workspace: string, options: { readOnly?: boolean; create?: boolean } = {}) => new runtime.store.Store(path, workspace, options.readOnly ?? false, options.create ?? false), readInput: () => Buffer.from(JSON.stringify(events.shift())) },
  };
}

async function hookEvent(f: Pick<ReturnType<typeof fixture>, 'workspace' | 'file'>, vendor: string, event: JsonRecord) {
  const { output, runtime: io } = hookRuntime([event]);
  await hostHooks.run({ vendor, workspace: f.workspace, database: f.file }, io);
  return output[0];
}

test('host hooks: session start binds an identity, tool events deliver mail once, session end leaves', async t => {
  const f = await pair(t);
  const base = { session_id: 'claude-session-1', cwd: f.workspace, permission_mode: 'default' };
  const started = await hookEvent(f, 'claude', { ...base, hook_event_name: 'SessionStart' });
  assert.equal(started.hookSpecificOutput.hookEventName, 'SessionStart');
  assert.match(started.hookSpecificOutput.additionalContext, /Communication identity: [0-9a-f-]{36}\. CLI flags: --session [0-9a-f-]{36} --workspace \./);
  const [identity] = rows(f, "SELECT id,vendor,vendorSession FROM sessions WHERE vendorSession='claude-session-1'");
  assert.equal(identity.vendor, 'claude');
  assert.equal(rows(f, 'SELECT transport FROM attachments WHERE session=?', identity.id)[0].transport, 'raw');
  assert.deepEqual(rows(f, "SELECT data FROM records WHERE type='host.inbound'").map(row => JSON.parse(String(row.data))), [{ vendor: 'claude', permissionMode: 'default' }]);
  assert.deepEqual(await hookEvent(f, 'claude', { ...base, hook_event_name: 'UserPromptSubmit' }), {});
  assert.deepEqual(await hookEvent(f, 'claude', { ...base, hook_event_name: 'PostToolUse' }), {}, 'nothing to deliver');
  await f.call(f.sender, 'send_message', { to: identity.id, body: 'hook-delivered peer' });
  const delivered = await hookEvent(f, 'claude', { ...base, hook_event_name: 'PostToolUse' });
  assert.equal(delivered.hookSpecificOutput.hookEventName, 'PostToolUse');
  assert.match(delivered.hookSpecificOutput.additionalContext, /hook-delivered peer/);
  assert.equal(rows(f, 'SELECT state FROM dispatches')[0].state, 'submitted');
  assert.deepEqual(await hookEvent(f, 'claude', { ...base, hook_event_name: 'PostToolUse' }), {});
  assert.deepEqual(await hookEvent(f, 'claude', { ...base, hook_event_name: 'SessionEnd' }), {});
  assert.equal(Number(only(rows(f, 'SELECT expiresAt FROM sessions WHERE id=?', identity.id)[0]).expiresAt) <= Date.now(), true);
});

test('host hooks answer with an empty output for subagents, foreign events, wrong workspaces and bad input', async t => {
  const f = await pair(t);
  const base = { session_id: 's-1', cwd: f.workspace };
  assert.deepEqual(await hookEvent(f, 'claude', { ...base, hook_event_name: 'PostToolUse', agent_id: 'sub' }), {});
  assert.deepEqual(await hookEvent(f, 'codex', { ...base, hook_event_name: 'Notification' }), {});
  assert.deepEqual(await hookEvent(f, 'codex', { ...base, hook_event_name: 'PostToolUseFailure' }), {});
  const outside = join(f.workspace, '..');
  assert.deepEqual(await hookEvent(f, 'claude', { session_id: 's-1', cwd: outside, hook_event_name: 'SessionStart' }), {});
  assert.deepEqual(await hookEvent(f, 'cursor', { sessionId: 'x', hook_event_name: 'sessionStart', workspace_roots: [f.workspace] }), {});
  const { output, runtime: io } = hookRuntime([]);
  io.readInput = () => Buffer.from('[1]');
  await hostHooks.run({ vendor: 'claude', workspace: f.workspace, database: f.file }, io);
  assert.deepEqual(output, [{}]);
  await assert.rejects(Promise.resolve().then(() => hostHooks.vendor({ vendor: 'nope', workspace: f.workspace })), /--vendor claude\|codex\|cursor\|grok required/);
});

test('host context budgets: oversized bodies become fetch references; released rows return to ready', async t => {
  const f = await pair(t);
  f.store.attach(f.receiver, { transport: 'raw' });
  await f.send('x'.repeat(7900));
  const items = f.store.stage(f.receiver, 'hook:claude');
  const small = hostHooks.hostContext('identity', items, hostHooks.HOST_CONTEXT_LIMIT);
  assert.match(small, /Peer bodies exceed the host context budget/);
  assert.match(small, /"bodyOmitted":true/);
  assert.equal(small.includes('xxxxxxxxxx'), false);
  assert.equal(hostHooks.hostContext('identity', [], 10), 'identity');
  f.store.releaseDispatch(f.receiver, items, dispatchOutcomes.BUDGET_DEFERRAL);
  assert.equal(rows(f, 'SELECT state,error FROM dispatches')[0].error, dispatchOutcomes.BUDGET_DEFERRAL);
});

test('host hook config: quoting, event sets and handler options per vendor', t => {
  const printed: string[] = [];
  const original = process.stderr.write.bind(process.stderr);
  process.stderr.write = chunk => { printed.push(String(chunk)); return true; };
  t.after(() => { process.stderr.write = original; });
  const output: JsonRecord[] = [];
  const base = fixture(t);
  hostHooks.config({ vendor: 'codex', workspace: base.workspace, database: base.file }, { output: (value: JsonRecord) => output.push(value), launcher: ['node', '/opt/cli.js'] });
  hostHooks.config({ vendor: 'cursor', workspace: base.workspace, database: base.file }, { output: (value: JsonRecord) => output.push(value), launcher: ['node', '/opt/my cli.js'] });
  hostHooks.config({ vendor: 'grok', workspace: base.workspace, database: base.file }, { output: (value: JsonRecord) => output.push(value), launcher: ['node'] });
  const [codex, cursor, grok] = output;
  assert.deepEqual(Object.keys(codex.hooks), ['SessionStart', 'UserPromptSubmit', 'PostToolUse', 'SessionEnd']);
  assert.equal(codex.hooks.SessionStart[0].hooks[0].additionalContextLimit, hostHooks.CODEX_CONTEXT_LIMIT);
  assert.match(codex.hooks.SessionStart[0].hooks[0].command, /^node \/opt\/cli\.js host-hook --vendor codex --workspace /);
  assert.equal(cursor.version, 1);
  assert.deepEqual(Object.keys(cursor.hooks), ['sessionStart', 'beforeSubmitPrompt', 'postToolUse', 'postToolUseFailure', 'sessionEnd']);
  assert.match(cursor.hooks.sessionStart[0].command, /^node '\/opt\/my cli\.js' host-hook/);
  assert.deepEqual(Object.keys(grok.hooks), ['SessionStart', 'UserPromptSubmit', 'PostToolUse', 'PostToolUseFailure', 'SessionEnd']);
  assert.equal(printed.length, 3);
  assert.equal(JSON.parse(printed[0]).type, 'leaseGuard');
  assert.equal(hostHooks.quote("it's"), "'it'\"'\"'s'");
  assert.throws(() => hostHooks.quote('a\nb'), /control characters/);
  assert.equal(hostHooks.windowsCommandLine(['a b', 'c"d', '', 'e']), '"a b" c\\"d "" e');
  assert.equal(hostHooks.relative('/w/x/y', '/w'), 'x/y');
  assert.equal(hostHooks.relative('/w', '/w/x/y'), '../..');
  assert.equal(hostHooks.relative('/w', '/w'), '.');
  assert.equal(hostHooks.relative('/a', '/b'), '/a');
});
