import { test } from 'node:test';
import type { TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { chmodSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { contract, expand, golden, runtime } from './golden-support.ts';
import { tempWorkspace } from './helpers.ts';
import type { JsonRecord } from './json-record.ts';
import type { Stop } from '../src/runtime/timing.ts';

const { jsonText, timing, wire, ValidationError } = runtime;

// ---- JSON as the previous runtime wrote it -------------------------------------------------------------------------------------

test('dumps equals the recorded output byte for byte for every option set the runtime uses', () => {
  for (const { options, cases } of golden.dumps) assert.deepEqual(cases.map(({ value }: JsonRecord) => jsonText.dumps(value, options)), cases.map(({ expected }: JsonRecord) => expected), JSON.stringify(options));
});

test('dumps escapes lone surrogates and non-ASCII exactly like the recorded output', () => {
  assert.deepEqual(golden.lone.map(({ value }: JsonRecord) => jsonText.dumps(value)), golden.lone.map(({ expected }: JsonRecord) => expected));
});

test('dumps reproduces the recorded NaN handling and refuses unserializable values', () => {
  assert.equal(jsonText.dumps([NaN, Infinity, -Infinity]), '[NaN, Infinity, -Infinity]');
  assert.throws(() => jsonText.dumps(NaN, jsonText.COMPACT), ValidationError);
  assert.throws(() => jsonText.dumps({ a: Infinity }, { allowNan: false }), /not JSON compliant/);
  assert.throws(() => jsonText.dumps(undefined), TypeError);
  assert.throws(() => jsonText.dumps(() => 1), TypeError);
  assert.throws(() => jsonText.dumps(new Map()), TypeError);
  assert.equal(jsonText.dumps({ a: undefined, b: 1 }), '{"b": 1}');
  assert.equal(jsonText.dumps([undefined]), '[null]');
  assert.equal(jsonText.dumps(2n ** 70n), '1180591620717411303424');
  assert.equal(jsonText.dumps(-0), '0');
});

test('integer-like object keys follow JavaScript order, the one place key order differs from the previous runtime', () => {
  // Documented in JSON-SITES.md: unsorted output of objects with integer-like keys is the only byte divergence.
  assert.equal(jsonText.dumps({ b: 1, 2: 2, a: 3 }), '{"2": 2, "b": 1, "a": 3}');
  assert.equal(jsonText.dumps({ b: 1, 2: 2, a: 3 }, { sortKeys: true }), '{"2": 2, "a": 3, "b": 1}');
});

test('encode is compact UTF-8 without ASCII escaping', () => {
  assert.equal(jsonText.encode({ a: 'é😀', b: [1, 2.5] }).toString('utf8'), '{"a":"é😀","b":[1,2.5]}');
  assert.ok(Buffer.isBuffer(jsonText.encode(1)));
});

test('refusal frames match the recorded output', () => {
  for (const [id, expected] of golden.refusal) assert.deepEqual(wire.refusal(id), expected);
});

// ---- process helpers ----------------------------------------------------------------------------------------------

test('splitShellWords equals the recorded output', () => {
  assert.deepEqual(golden.shlex.map(({ line }: JsonRecord) => wire.splitShellWords(line)), golden.shlex.map(({ expected }: JsonRecord) => expected));
  assert.throws(() => wire.splitShellWords('a "b'), /No closing quotation/);
  assert.throws(() => wire.splitShellWords('a\\'), /No escaped character/);
});

test('executableArgv invokes script interpreters explicitly like the recorded output', t => {
  const directory = tempWorkspace(t, 'wire-', { real: true });
  const make = (name: string, content: string) => {
    const path = join(directory, name);
    writeFileSync(path, content);
    chmodSync(path, 0o755);
    return path;
  };
  make('sh-script', '#!/bin/sh\necho hi\n'); make('env-node', '#!/usr/bin/env node\nconsole.log(1)\n'); make('spaced', '#! /usr/bin/env  perl -u\n');
  make('plain', 'echo no shebang\n'); make('no-newline', '#!/bin/sh'); make('long', '#!' + '/x'.repeat(3000) + '\n'); make('empty-interpreter', '#!\nfoo\n');
  const actual = golden.executableArgv.commands.map((command: string) => wire.executableArgv(expand(directory, command), ['--flag', 'value']));
  assert.deepEqual(contract(directory, actual), golden.executableArgv.expected);
  const onPath = process.env.PATH;
  process.env.PATH = directory + ':' + onPath;
  try {
    assert.deepEqual(wire.executableArgv('sh-script', ['x']), ['/bin/sh', join(directory, 'sh-script'), 'x']);
  } finally {
    process.env.PATH = onPath;
  }
});

// ---- vendor process ---------------------------------------------------------------------------------------------

const VENDOR = `
const readline = require('node:readline');
const send = value => process.stdout.write(JSON.stringify(value) + '\\n');
process.on('SIGTERM', () => { if (process.env.IGNORE_TERM) return; process.exit(143); });
let waiting = null;
readline.createInterface({ input: process.stdin }).on('line', line => {
  const message = JSON.parse(line);
  if (waiting && message.id === waiting.ask) { send({ method: 'refusal-seen', params: message }); send({ id: waiting.reply, result: 'asked' }); waiting = null; return; }
  const method = message.method ?? message.type;
  if (method === 'echo') send({ id: message.id, result: message.params });
  else if (method === 'ask') { waiting = { ask: 'srv-1', reply: message.id }; send({ id: 'srv-1', method: 'tools/call', params: {} }); }
  else if (method === 'note') { send({ method: 'n1', params: {} }); send({ method: 'n2', params: {} }); send({ id: message.id, result: 'noted' }); }
  else if (method === 'flood') { for (let i = 0; i < 600; i += 1) send({ method: 'n' + i }); send({ id: message.id, result: 'flooded' }); }
  else if (method === 'fail') send({ id: message.id, error: { code: -1, message: 'nope' } });
  else if (method === 'get_state') send({ id: message.id, type: 'response', success: true, data: { state: 'idle', echoed: message.value } });
  else if (method === 'bad') send({ id: message.id, success: false, error: 'denied' });
  else if (method === 'big') process.stdout.write('x'.repeat(9 * 1024 * 1024) + '\\n');
  else if (method === 'partial') process.stdout.write('{"id":' + message.id + ',"result":"tail"}', () => process.exit(0));
  else if (method === 'garbage') process.stdout.write('not json\\n');
  else if (method === 'exit') process.exit(0);
  else if (method === 'pid') send({ id: message.id, result: { pid: process.pid, group: process.env.GROUP } });
  // 'hang' and anything else is never answered.
});
`;

async function vendor(t: TestContext, { deadline = null, stop, env = {} }: { deadline?: number | null; stop?: Stop; env?: Record<string, string> } = {}) {
  const directory = tempWorkspace(t, 'wire-', { real: true });
  const script = join(directory, 'vendor.cjs');
  writeFileSync(script, VENDOR);
  const child = new wire.Wire(process.execPath, [script], directory, Object.entries(env), deadline, stop);
  t.after(() => child.close());
  return child;
}

/** The vendor's own pid, as it reports it. */
async function pidOf(child: Awaited<ReturnType<typeof vendor>>): Promise<number> {
  const reply = await child.request('pid', {});
  assert.ok(typeof reply === 'object' && reply !== null && 'pid' in reply && typeof reply.pid === 'number');
  return reply.pid;
}

test('requests are answered, server requests are refused, notifications are queued in order', async t => {
  const child = await vendor(t);
  assert.deepEqual(await child.request('echo', { text: 'é😀', n: [1, 2.5] }), { text: 'é😀', n: [1, 2.5] });
  assert.equal(await child.request('ask', {}), 'asked');
  assert.deepEqual(await child.event(1000), { method: 'refusal-seen', params: wire.refusal('srv-1') });
  assert.equal(await child.request('note', {}), 'noted');
  assert.deepEqual(child.pending.map(item => item.method), ['n1', 'n2']);
  const first = await child.event(10);
  assert.ok(first);
  assert.deepEqual(first.method, 'n1');
  const second = await child.event(10);
  assert.ok(second);
  assert.deepEqual(second.method, 'n2');
  assert.equal(await child.event(50), null);
});

test('errors, Pi-style requests and sequence ids behave like the previous runtime', async t => {
  const child = await vendor(t);
  await assert.rejects(child.request('fail', {}), /Vendor request failed: \{"id": 1, "error": \{"code": -1, "message": "nope"\}\}/);
  assert.deepEqual(await child.piRequest('get_state', { value: 'ü' }), { state: 'idle', echoed: 'ü' });
  await assert.rejects(child.piRequest('bad', {}), /Vendor request failed: \{"id": "3", "success": false, "error": "denied"\}/);
});

test('a flood of notifications is bounded at 512 and never dropped silently', async t => {
  const child = await vendor(t);
  await assert.rejects(child.request('flood', {}), /Vendor notification queue full/);
  assert.equal(child.pending.length, 512);
});

test('deadline, stop signal and closed pipes end an exchange with the previous messages', async t => {
  const timed = await vendor(t, { deadline: timing.deadlineAfter(400) });
  await assert.rejects(timed.request('hang', {}), /Timed out: hang/);
  const controller = new AbortController();
  const stopped = await vendor(t, { stop: controller.signal });
  setTimeout(() => controller.abort(), 200);
  await assert.rejects(stopped.request('hang', {}), /Worker stopped/);
  const closed = await vendor(t);
  await closed.send({ method: 'exit' });
  await assert.rejects(closed.receive(5000), /Vendor process closed stdout/);
  // Like a queue, the failure is delivered once; afterwards nothing arrives.
  assert.equal(await closed.receive(100), null);
});

test('frames are bounded at 8 MiB in both directions', async t => {
  const oversizedRead = await vendor(t);
  await oversizedRead.send({ method: 'big', id: 1 });
  await assert.rejects(oversizedRead.receive(10000), wire.OversizedFrame);
  const oversizedWrite = await vendor(t);
  await assert.rejects(oversizedWrite.send({ method: 'echo', id: 1, params: 'x'.repeat(wire.MAX_FRAME) }), wire.OversizedFrame);
  const garbage = await vendor(t);
  await garbage.send({ method: 'garbage' });
  await assert.rejects(garbage.receive(5000), SyntaxError);
  const partial = await vendor(t);
  assert.equal(await partial.request('partial', {}), 'tail');
});

test('close ends the vendor and its process group, escalating when SIGTERM is ignored', async t => {
  const polite = await vendor(t);
  const pid = await pidOf(polite);
  assert.equal(pid, polite.pid);
  await polite.close();
  assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
  await assert.rejects(polite.send({ method: 'echo' }), /Vendor process closed/);
  await polite.close();

  const stubborn = await vendor(t, { env: { IGNORE_TERM: '1' } });
  const stubbornPid = await pidOf(stubborn);
  const started = Date.now();
  await stubborn.close();
  assert.ok(Date.now() - started >= 1500, 'waited for SIGTERM before SIGKILL');
  assert.throws(() => process.kill(stubbornPid, 0), { code: 'ESRCH' });
});

test('close is also reachable through async disposal', async t => {
  const child = await vendor(t);
  const pid = await pidOf(child);
  await child[Symbol.asyncDispose]();
  assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
});
