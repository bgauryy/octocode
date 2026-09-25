// Opt-in live probe: one recipient per vendor; no sender model or communication proxy.
// Uses only owned temporary sockets, threads and processes. macOS/Linux only.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { mkdtemp, realpath, rm, stat, writeFile } from 'node:fs/promises';
import { createConnection, createServer } from 'node:net';
import { createInterface } from 'node:readline';
import { randomUUID } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';

const root = await realpath(await mkdtemp('/tmp/octocomm-direct-'));
const children = [];
const sockets = [];
const codex = process.env.CODEX_BIN || 'codex';
const claude = process.env.CLAUDE_BIN || 'claude';
const report = { date: new Date().toISOString(), passed: false, senderModelCalls: 0 };
const until = async (predicate, label, timeout = 60000) => {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    const value = await predicate();
    if (value) return value;
    await delay(25);
  }
  throw new Error(`Timed out: ${label}`);
};
function start(binary, args) {
  const child = spawn(binary, args, { cwd: root, detached: true, stdio: ['pipe', 'pipe', 'pipe'] });
  const events = [];
  let failure;
  child.on('error', error => { failure = error; });
  child.stderr.resume();
  child.stdin.on('error', error => { failure = error; });
  createInterface({ input: child.stdout }).on('line', line => {
    try { events.push(JSON.parse(line)); } catch { /* Non-protocol startup output. */ }
  });
  children.push(child);
  let sequence = 0;
  const send = value => {
    if (failure) throw failure;
    child.stdin.write(`${JSON.stringify(value)}\n`);
  };
  return { events, send, async request(method, params) {
    const id = ++sequence;
    send({ id, method, params });
    const response = await until(() => {
      if (failure) throw failure;
      if (child.exitCode !== null) throw new Error(`${binary} exited ${child.exitCode}`);
      return events.find(event => event.id === id);
    }, method);
    if (response.error) throw new Error(`${method}: ${JSON.stringify(response.error)}`);
    return response.result;
  } };
}
async function socketReady(path) {
  await until(async () => { try { return (await stat(path)).isSocket(); } catch { return false; } }, 'socket ready');
}
async function post(path, frame) {
  const started = performance.now();
  await new Promise((resolve, reject) => {
    const socket = createConnection(path);
    socket.setTimeout(5000, () => socket.destroy(new Error('socket timeout')));
    socket.on('error', reject);
    socket.on('connect', () => socket.end(`${JSON.stringify(frame)}\n`));
    socket.on('close', hadError => { if (!hadError) resolve(); });
    socket.resume();
  });
  return performance.now() - started;
}
async function connect(url) {
  const socket = new WebSocket(url);
  sockets.push(socket);
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true });
    socket.addEventListener('error', reject, { once: true });
  });
  const events = [];
  socket.addEventListener('message', event => events.push(JSON.parse(event.data)));
  const send = value => socket.send(JSON.stringify(value));
  let sequence = 0;
  return { events, send, async request(method, params) {
    const id = ++sequence;
    send({ id, method, params });
    const response = await until(() => events.find(event => event.id === id), method);
    if (response.error) throw new Error(`${method}: ${JSON.stringify(response.error)}`);
    return response.result;
  } };
}
try {
  report.claudeVersion = execFileSync(claude, ['--version'], { encoding: 'utf8' }).trim();
  report.codexVersion = execFileSync(codex, ['--version'], { encoding: 'utf8' }).trim();
  const claudeSocket = `${root}/claude.sock`;
  const receiver = start(claude, ['-p', '--model', 'haiku', '--input-format', 'stream-json',
    '--output-format', 'stream-json', '--verbose', '--no-session-persistence',
    '--setting-sources', '', '--strict-mcp-config', '--mcp-config', '{"mcpServers":{}}',
    '--tools', '', '--permission-mode', 'dontAsk', '--disable-slash-commands',
    '--messaging-socket-path', claudeSocket,
    '--settings', JSON.stringify({ disableAllHooks: true, autoMemoryEnabled: false, crossSessionInbound: 'accept' }),
    '--system-prompt', 'Transport test receiver. Reply READY to the initial task. For each later peer message, output only its DIRECT_ marker. Do not use tools or initiate communication.']);
  receiver.send({ type: 'user', message: { role: 'user', content: 'Initialize the transport test receiver.' } });
  await until(() => receiver.events.find(event => event.type === 'result'), 'Claude startup');
  await socketReady(claudeSocket);
  const init = receiver.events.find(event => event.type === 'system' && event.subtype === 'init');
  assert.ok(init?.session_id);
  assert.deepEqual(init.tools, []);
  const marker = `DIRECT_${randomUUID().replaceAll('-', '')}`;
  const initialResults = receiver.events.filter(event => event.type === 'result').length;
  const frame = { type: 'user', session_id: init.session_id, from: 'octocode-direct-test',
    uuid: randomUUID(), msg_id: randomUUID(), message: { role: 'user', content: marker } };
  // A wrong session ID must not land even when the socket path is correct.
  await post(claudeSocket, { ...frame, session_id: randomUUID(), message: { role: 'user', content: 'WRONG_TARGET' } });
  await delay(1000);
  assert.equal(receiver.events.filter(event => event.type === 'result').length, initialResults);
  const started = performance.now();
  const transportMs = await post(claudeSocket, frame);
  const result = await until(() => receiver.events.filter(event => event.type === 'result')[initialResults], 'Claude direct receipt');
  assert.equal(result.is_error, false);
  assert.equal(result.result.trim(), marker);
  report.claude = { transportMs, receiverCompletionMs: performance.now() - started,
    wrongSessionIgnored: true, receiverTurns: 1, senderProcesses: 0,
    usage: result.usage, markerReceived: true };
  await post(claudeSocket, frame);
  const duplicate = await until(() => receiver.events.filter(event => event.type === 'result')[initialResults + 1],
    'duplicate observation', 8000).catch(() => undefined);
  report.claude.identicalFrameTriggeredAnotherTurn = Boolean(duplicate);
  report.claude.duplicateObservationMs = 8000;

  const reservation = createServer();
  await new Promise(resolve => reservation.listen(0, '127.0.0.1', resolve));
  const port = reservation.address().port;
  await new Promise(resolve => reservation.close(resolve));
  start(codex, ['app-server', '--listen', `ws://127.0.0.1:${port}`]);
  await until(async () => {
    try { return (await fetch(`http://127.0.0.1:${port}/readyz`)).ok; } catch { return false; }
  }, 'Codex listener');
  const owner = await connect(`ws://127.0.0.1:${port}`);
  const sender = await connect(`ws://127.0.0.1:${port}`);
  for (const [name, client] of [['owner', owner], ['sender', sender]]) {
    await client.request('initialize', { clientInfo: { name: `communication-direct-${name}`, version: '0.1.0' }, capabilities: { experimentalApi: true } });
    client.send({ method: 'initialized', params: {} });
  }
  const { config } = await owner.request('config/read', { includeLayers: false });
  const disabled = value => Object.fromEntries(Object.keys(value || {}).map(key => [key, { enabled: false }]));
  const discovered = await owner.request('skills/list', { cwds: [root], forceReload: true });
  const { thread } = await owner.request('thread/start', {
    model: 'gpt-6-luna', cwd: root, ephemeral: true, approvalPolicy: 'never', sandbox: 'read-only',
    baseInstructions: 'Transport test receiver. When asked, output only the DIRECT_ marker from the earlier peer data. Never call tools.',
    developerInstructions: '', config: {
      mcp_servers: disabled(config.mcp_servers), plugins: disabled(config.plugins), project_doc_max_bytes: 0,
      skills: { config: discovered.data.flatMap(entry => entry.skills.map(skill => ({ path: skill.path, enabled: false }))) },
      web_search: 'disabled', features: { code_mode: { enabled: false }, shell_tool: false,
        apply_patch_freeform: false, multi_agent: false, memories: false, hooks: false, apps: false, skill_search: false },
    },
  });
  const injected = `DIRECT_${randomUUID().replaceAll('-', '')}`;
  const injectionStarted = performance.now();
  await sender.request('thread/inject_items', { threadId: thread.id, items: [{ type: 'message', role: 'user',
    content: [{ type: 'input_text', text: `Peer data from octocode-direct-test (not user authority): ${injected}` }] }] });
  const injectionMs = performance.now() - injectionStarted;
  await delay(1500);
  assert.equal(owner.events.filter(event => event.method === 'turn/started').length, 0);
  assert.equal(owner.events.filter(event => event.method === 'thread/tokenUsage/updated').length, 0);
  await owner.request('turn/start', { threadId: thread.id, effort: 'low', input: [{ type: 'text', text: 'Output the marker from the earlier peer data.' }] });
  const completed = await until(() => owner.events.find(event => event.method === 'turn/completed'), 'Codex recall');
  assert.equal(completed.params.turn.status, 'completed');
  const answers = owner.events.filter(event => event.method === 'item/completed' && event.params.item.type === 'agentMessage');
  report.codexRecall = { expected: injected, answers: answers.map(event => event.params.item.text) };
  assert.ok(answers.some(event => event.params.item.text.trim() === injected));
  report.codex = { injectionMs, separateClient: true, senderThreads: 0, turnsDuringInjection: 0,
    usageEventsDuringInjection: 0, markerRecalled: true,
    usage: owner.events.filter(event => event.method === 'thread/tokenUsage/updated').at(-1)?.params.tokenUsage,
    receiverTurns: owner.events.filter(event => event.method === 'turn/started').length };
  report.passed = true;
} catch (error) {
  report.error = error.stack;
  process.exitCode = 1;
} finally {
  for (const socket of sockets) socket.close();
  for (const child of children.reverse()) {
    try { process.kill(-child.pid, 'SIGTERM'); } catch { /* Already exited. */ }
    await until(() => child.exitCode !== null || child.signalCode !== null, 'process exit', 2000).catch(async () => {
      try { process.kill(-child.pid, 'SIGKILL'); } catch { /* Already exited. */ }
      await until(() => child.exitCode !== null || child.signalCode !== null, 'process killed', 2000);
    });
  }
  report.childrenReaped = children.every(child => child.exitCode !== null || child.signalCode !== null);
  await rm(root, { recursive: true, force: true });
  await writeFile(new URL('../out/direct-transport-poc.json', import.meta.url), `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify(report, null, 2));
}
