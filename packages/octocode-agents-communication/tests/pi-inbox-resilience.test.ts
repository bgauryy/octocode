import { test } from 'node:test';
import type { TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { registerPiInbox } from '../dist/hosts/pi-inbox.js';
import type { JsonRecord } from './json-record.ts';
import { binary, execFileSync, fakePi, jsonCall, tempWorkspace } from './helpers.ts';
import type { PiContext } from '../src/hosts/shared/command.ts';

function fixture(t: TestContext) {
  const workspace = tempWorkspace(t, 'pi-resilience-'), database = join(workspace, 'db.sqlite');
  const ledger: JsonRecord[] = [];
  const { host: pi, handlers } = fakePi(message => { ledger.push({ type: 'custom_message', ...message }); });
  const ctx: PiContext = { cwd: workspace, sessionManager: { getSessionId: () => 'pi-test', getSessionFile: () => join(workspace, 'pi-session.jsonl'), getEntries: () => ledger } };
  return { workspace, database, run: jsonCall(binary, workspace, database), pi, ctx, fire: (name: string, event: JsonRecord = {}) => handlers.get(name)?.(event, ctx) };
}

const dispatchState = (f: ReturnType<typeof fixture>, session: string, message: JsonRecord) => JSON.parse(execFileSync(binary, ['fetch', JSON.stringify({ types: ['dispatch.staged', 'dispatch.submitted'], current: true, from: session, where: { messageId: message.id } }),
  '--workspace', f.workspace, '--database', f.database, '--session', session], { encoding: 'utf8' })).items[0].type.slice('dispatch.'.length);

test('Pi skips a malformed ledger line, reports it once, and still confirms later receipts', async t => {
  const errors = t.mock.method(console, 'error', () => {});
  const f = fixture(t), controller = registerPiInbox(f.pi, { binary, database: f.database });
  await f.fire('session_start');
  try {
    const session = controller.getBinding()?.session;
    assert.ok(session, 'the controller is bound to a session');
    const sender = f.run('join', { vendor: 'raw', name: 'sender' }).id;
    const message = f.run('send_message', {parts: [{ text: 'after a bad line' }],  to: session, reasoning: 'Test ledger scan past a malformed entry' }, sender);
    const { items } = f.run('hook', { format: 'json', deferConfirm: true, consumer: 'pi:pi-test' }, session);
    assert.equal(dispatchState(f, session, message), 'staged');
    const receipt = { type: 'custom_message', customType: 'octocode-peer', details: { session, database: f.database, receipts: items.map(({ id, dispatchToken }: JsonRecord) => ({ id, dispatchToken })) } };
    writeFileSync(join(f.workspace, 'pi-session.jsonl'), [JSON.stringify({ type: 'session', id: 'pi-test' }), '{"type":"message","text":', 'null', JSON.stringify(receipt)].join('\n') + '\n');
    await f.fire('session_start');
    assert.equal(dispatchState(f, session, message), 'submitted');
    assert.equal(errors.mock.calls.filter(call => /malformed Pi ledger/.test(String(call.arguments[0]))).length, 1);
  } finally { await f.fire('session_shutdown'); }
});

test('A failed usage record is reported without failing the host event', async t => {
  const errors = t.mock.method(console, 'error', () => {});
  const f = fixture(t), runtime = join(f.workspace, 'runtime.mjs');
  writeFileSync(runtime, `import { spawnSync } from 'node:child_process';
if (process.argv[2] === 'record_usage') { process.stderr.write('usage store unavailable'); process.exit(1); }
const result = spawnSync(process.execPath, [${JSON.stringify(binary)}, ...process.argv.slice(2)], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
process.stdout.write(result.stdout); process.stderr.write(result.stderr); process.exit(result.status);
`);
  registerPiInbox(f.pi, { binary: runtime, database: f.database });
  await f.fire('session_start');
  try {
    await f.fire('message_end', { message: { role: 'assistant', timestamp: 1, usage: { input: 1, output: 2, cacheRead: 0, cacheWrite: 0 } } });
    assert.ok(errors.mock.calls.some(call => /Communication usage:.*usage store unavailable/s.test(String(call.arguments[0]))));
  } finally { await f.fire('session_shutdown'); }
});
