import { test } from 'node:test';
import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';
import { AcpProbeClient, runProbe } from '../scripts/acp-probe.mjs';

// A wire peer, not a model: failures exercise framing and lifecycle independently.
function command(mode = 'normal') {
  return [process.execPath, '--input-type=module', '-e', String.raw`
    import { createInterface } from 'node:readline';
    import { spawn } from 'node:child_process';
    const mode = ${JSON.stringify(mode)};
    if (mode === 'stubborn') process.on('SIGTERM', () => {});
    const write = value => process.stdout.write(JSON.stringify({ jsonrpc: '2.0', ...value }) + '\n');
    let promptId;
    createInterface({ input: process.stdin }).on('line', line => {
      const q = JSON.parse(line);
      if (mode === 'timeout') return;
      if (mode === 'stubborn') return;
      if (mode === 'stderr-budget') { process.stderr.write('x'.repeat(4096)); return; }
      if (mode === 'eof') { process.stdout.end(); return; }
      if (mode === 'truncated') { process.stdout.end('{'); return; }
      if (mode === 'malformed') { process.stdout.write('nope\n'); return; }
      if (mode === 'utf8') { process.stdout.write(Buffer.from([0xff, 10])); return; }
      if (mode === 'oversized' || mode === 'stdout-budget') { process.stdout.write('x'.repeat(4096)); return; }
      if (mode === 'mixed') { write({ id: q.id, result: {}, error: {} }); return; }
      if (mode === 'unknown-id') { write({ id: 999, result: {} }); return; }
      if (q.method === 'initialize') {
        if (mode === 'escaped-descendant') {
          const escaped = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { detached: true, stdio: ['ignore', 1, 2] });
          escaped.unref();
          write({ id: q.id, result: { protocolVersion: 1, agentCapabilities: {}, _meta: { fixtureDescendantPid: escaped.pid } } });
          return;
        }
        write({ id: q.id, result: { protocolVersion: mode === 'version' ? 7 : 1,
          agentCapabilities: mode === 'no-resume' ? { loadSession: true } : { sessionCapabilities: { resume: {} } } } });
      } else if (q.method === 'session/resume') {
        if (mode === 'replay') write({ method: 'session/update', params: { sessionId: q.params.sessionId, update: { sessionUpdate: 'user_message_chunk', content: { type: 'text', text: 'old history' } } } });
        write({ id: q.id, result: { echoedConfiguration: q.params } });
      } else if (q.method === 'session/prompt') {
        promptId = q.id;
        if (mode === 'cancel') return;
        if (mode === 'permission') { write({ id: 'permission', method: 'session/request_permission', params: { sessionId: q.params.sessionId, options: [{ kind: 'allow_once', optionId: 'allow' }, { kind: 'reject_once', optionId: 'reject' }] } }); return; }
        if (mode === 'client-tool') { write({ id: 'tool', method: 'fs/read_text_file', params: { path: '/private' } }); return; }
        write({ method: 'session/update', params: { sessionId: mode === 'wrong-session' ? 'other' : q.params.sessionId, update: { sessionUpdate: 'agent_message_chunk', content: { type: 'text', text: 'reply' } } } });
        write({ id: q.id, result: { stopReason: 'end_turn' } });
      } else if (q.method === 'session/cancel') write({ id: promptId, result: { stopReason: 'cancelled' } });
      else if (q.id === 'permission') write({ id: promptId, result: { stopReason: q.result?.outcome?.optionId === 'reject' ? 'refusal' : 'unsafe' } });
      else if (q.id === 'tool') write({ id: promptId, result: { stopReason: q.error?.code === -32601 ? 'refusal' : 'unsafe' } });
      else { write({ id: q.id, error: { code: -32601, message: 'unknown' } }); }
    });
  `];
}
const session = { sessionId: 'existing', cwd: process.cwd(), mcpServers: [{ name: 'existing-tool', command: '/tool', args: [], env: [] }], configurationReviewed: true };
async function client(t, mode, options) {
  const c = new AcpProbeClient(command(mode), options);
  t.after(() => c.close());
  await c.initialize();
  return c;
}

test('ACP negotiates then resumes without load/new or changing supplied MCP configuration', async t => {
  const c = await client(t);
  const result = await c.resume(session);
  assert.deepEqual(result.echoedConfiguration, { sessionId: session.sessionId, cwd: session.cwd, mcpServers: session.mcpServers });
  assert.deepEqual(c.stats.requests, { initialize: 1, 'session/resume': 1 });
  assert.equal(c.stats.historyReplayChunks, 0);
  assert.equal((await c.prompt('one message', { allowModelTurn: true })).stopReason, 'end_turn');
  assert.equal(c.stats.requests['session/prompt'], 1);
  assert.equal(c.stats.notifications.agent_message_chunk, 1);
});

test('ACP refuses unsupported resume instead of replaying history', async t => {
  const c = await client(t, 'no-resume');
  await assert.rejects(c.resume(session), /resume unsupported/);
  assert.deepEqual(c.stats.requests, { initialize: 1 });
});

test('ACP requires explicit full configuration review, existing binding and model opt-in', async t => {
  const c = await client(t);
  await assert.rejects(c.resume({ ...session, configurationReviewed: false }), /reviewed/);
  await assert.rejects(c.resume({ ...session, mcpServers: undefined }), /reviewed/);
  await assert.rejects(c.prompt('no binding', { allowModelTurn: true }), /resumed session/);
  await c.resume(session);
  await assert.rejects(c.prompt('no permission'), /allowModelTurn/);
  await assert.rejects(c.prompt('passive', { passive: true, allowModelTurn: true }), /passive delivery unsupported/);
  assert.equal(c.stats.requests['session/prompt'], undefined);
});

test('ACP counts streaming output but retains no repeated conversation and denies permissions', async t => {
  const c = await client(t, 'permission');
  await c.resume(session);
  assert.equal((await c.prompt('action', { allowModelTurn: true })).stopReason, 'refusal');
  assert.equal(c.stats.permissionDenials, 1);
  assert.equal(JSON.stringify(c.stats).includes('action'), false);
});

test('ACP advertises no filesystem or terminal service and rejects unsolicited tool requests', async t => {
  const c = await client(t, 'client-tool');
  await c.resume(session);
  assert.equal((await c.prompt('action', { allowModelTurn: true })).stopReason, 'refusal');
});

test('ACP cancellation is a notification and waits for the existing prompt outcome', async t => {
  const c = await client(t, 'cancel');
  await c.resume(session);
  const response = c.prompt('action', { allowModelTurn: true });
  await assert.rejects(c.prompt('duplicate', { allowModelTurn: true }), /already in flight/);
  c.cancel();
  assert.equal((await response).stopReason, 'cancelled');
  assert.equal(c.stats.requests['session/prompt'], 1);
  assert.throws(() => c.cancel(), /no active prompt/);
});

for (const [mode, pattern] of [['malformed', /malformed/], ['utf8', /malformed/], ['oversized', /too large/], ['eof', /EOF/], ['truncated', /truncated/], ['timeout', /timed out.*uncertain/], ['mixed', /invalid response/], ['unknown-id', /unsolicited/], ['version', /incompatible/]]) {
  test(`ACP ${mode} fails boundedly and reaps its owned process`, async () => {
    const c = new AcpProbeClient(command(mode), { timeoutMs: 250, maxFrameBytes: 1024 });
    await assert.rejects(c.initialize(), pattern);
    await c.close();
    assert.notEqual(c.child.exitCode ?? c.child.signalCode, null);
    assert.equal(c.pending.size, 0);
  });
}

test('ACP stops if resume replays history or output belongs to a different session', async t => {
  const replay = await client(t, 'replay');
  await assert.rejects(replay.resume(session), /replayed conversation/);
  assert.equal(replay.stats.historyReplayChunks, 1);
  const wrong = await client(t, 'wrong-session');
  await wrong.resume(session);
  await assert.rejects(wrong.prompt('action', { allowModelTurn: true }), /another session/);
});

test('ACP process spawn errors and total byte limits do not leave pending work', async () => {
  const missing = new AcpProbeClient(['/path/to/nonexistent-octocode-acp']);
  await assert.rejects(missing.initialize(), /could not start/);
  await missing.close();
  const bounded = new AcpProbeClient(command(), { maxTotalBytes: 20 });
  await assert.rejects(bounded.initialize(), /byte budget/);
  await bounded.close();
});

test('ACP opt-in probe reports unsupported paths without issuing model calls or session/new', async () => {
  const result = await runProbe({ command: command('no-resume'), ...session });
  assert.equal(result.passed, false);
  assert.match(result.error, /resume unsupported/);
  assert.equal(result.senderModelCalls, 0);
  assert.equal(result.recipientPromptRequests, 0);
  assert.deepEqual(result.stats.requests, { initialize: 1 });
});

test('ACP stdout and stderr have separate finite byte budgets', async () => {
  for (const mode of ['stdout-budget', 'stderr-budget']) {
    const c = new AcpProbeClient(command(mode), { maxTotalBytes: 1024, maxFrameBytes: 8192 });
    await assert.rejects(c.initialize(), /byte budget/);
    await c.close();
  }
});

test('ACP teardown escalates for an owned process that ignores termination', async () => {
  const c = new AcpProbeClient(command('stubborn'), { timeoutMs: 100 });
  await assert.rejects(c.initialize(), /timed out/);
  await c.close();
  assert.equal(c.child.signalCode, 'SIGKILL');
});

test('ACP cleanup is bounded when a detached descendant retains stdio', async () => {
  const c = new AcpProbeClient(command('escaped-descendant'));
  let descendant;
  try {
    const result = await c.initialize();
    descendant = result._meta.fixtureDescendantPid;
    await Promise.race([c.close(), delay(1500).then(() => { throw Error('ACP cleanup hung on inherited pipes'); })]);
    assert.equal(c.stats.forcedStdioCleanup, true);
    assert.notEqual(c.child.exitCode ?? c.child.signalCode, null);
    // The probe reports forced pipe closure honestly; the fixture owns reaping.
    assert.doesNotThrow(() => process.kill(descendant, 0));
  } finally {
    if (descendant) {
      try { process.kill(descendant, 'SIGKILL'); }
      catch (error) { if (error.code !== 'ESRCH') throw error; }
    }
    await c.close();
  }
});
