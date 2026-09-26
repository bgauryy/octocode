// Opt-in live OpenCode integration. Requires Node 22+ and a built communication skill.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, writeFileSync, existsSync, copyFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';

const root = fileURLToPath(new URL('../', import.meta.url));
const output = resolve(process.env.COMMUNICATION_OUTPUT ?? join(root, '../../.octocode/benchmarks/communication-opencode/results', new Date().toISOString().replaceAll(':', '-')));
mkdirSync(output, {recursive: true});
const workspace = realpathSync(mkdtempSync('/tmp/communication-opencode-'));
const database = join(workspace, 'audit.sqlite');
const target = execFileSync('rustc', ['-vV'], {encoding: 'utf8'}).match(/^host: (.+)$/m)[1];
const sourceBinary = process.env.COMMUNICATION_BINARY ?? join(root, 'skills/octocode-agents-communication/scripts/bin', target, 'octocode-agents-communication');
const binary = join(workspace, 'communication');
copyFileSync(sourceBinary, binary);
const opencode = process.env.COMMUNICATION_OPENCODE_BINARY ?? 'opencode';
const model = process.env.COMMUNICATION_OPENCODE_MODEL ?? 'opencode/mimo-v2.6-flash-free';
const authenticated = process.env.COMMUNICATION_OPENCODE_AUTH !== '0';
const digest = value => createHash('sha256').update(value).digest('hex');
const children = [], agents = [], streams = [];
const binding = ['--workspace', workspace, '--database', database];
const report = {passed: false, workspace, database, startedAt: new Date().toISOString(), model, authenticated, routingModelCalls: 0, hostPromptsAfterStartup: 0, binarySha256: digest(readFileSync(binary)), harnessSha256: digest(readFileSync(fileURLToPath(import.meta.url))), scope: 'Real OpenCode recipients and provider inference, production Rust API delivery and MCP tools. Functional integration; not a comparative performance or model-quality benchmark.'};
writeFileSync(join(output, 'harness.mjs'), readFileSync(fileURLToPath(import.meta.url)));
const call = (command, input = {}, session) => JSON.parse(execFileSync(binary, [command, JSON.stringify(input), ...binding, ...(session ? ['--session', session] : [])], {encoding: 'utf8', timeout: 15000, stdio: ['pipe', 'pipe', 'pipe']}));
async function until(predicate, label, timeout = 180000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    for (const item of children) if (item.error || item.child.exitCode !== null || item.child.signalCode !== null) throw Error(`${item.name} exited: ${item.error ?? item.stderr}`);
    const value = await predicate(); if (value) return value;
    await delay(150);
  }
  throw Error(`Timed out: ${label}`);
}
function start(name, command, args, env = {}) {
  const child = spawn(command, args, {cwd: workspace, detached: true, env: {...process.env, ...env}, stdio: ['ignore', 'pipe', 'pipe']});
  const item = {name, child, stdout: '', stderr: ''}; children.push(item);
  child.on('error', e => { item.error = e.message; });
  child.stdout.on('data', data => { item.stdout = (item.stdout + data).slice(-65536); });
  child.stderr.on('data', data => { item.stderr = (item.stderr + data).slice(-65536); });
  return item;
}
async function port() {
  const server = createServer(); await new Promise(r => server.listen(0, '127.0.0.1', r));
  const value = server.address().port; await new Promise(r => server.close(r)); return value;
}
async function api(agent, path, data) {
  const response = await fetch(`${agent.endpoint}${path}`, {method: data === undefined ? 'GET' : 'POST', headers: {'content-type': 'application/json', ...agent.headers}, ...(data === undefined ? {} : {body: JSON.stringify(data)}), signal: AbortSignal.timeout(120000)});
  const text = await response.text();
  assert.ok(response.ok, `${path}: ${response.status} ${text.slice(0, 500)}`);
  return text ? JSON.parse(text) : null;
}
async function watch(agent) {
  const abort = new AbortController(); streams.push(abort);
  const response = await fetch(`${agent.endpoint}/event`, {headers: agent.headers, signal: abort.signal});
  assert.equal(response.status, 200);
  agent.events = [];
  agent.stream = (async () => {
    let buffer = '';
    for await (const chunk of response.body) {
      buffer += Buffer.from(chunk).toString();
      for (;;) {
        const split = buffer.indexOf('\n\n'); if (split < 0) break;
        const frame = buffer.slice(0, split); buffer = buffer.slice(split + 2);
        for (const line of frame.split('\n')) if (line.startsWith('data:')) agent.events.push(JSON.parse(line.slice(5)));
      }
    }
  })().catch(error => { if (!abort.signal.aborted) agent.streamError = error.message; });
}
let db, heartbeat, controller;
try {
  report.opencodeVersion = execFileSync(opencode, ['--version'], {encoding: 'utf8'}).trim();
  controller = call('join', {name: 'opencode-controller', vendor: 'test-host'});
  for (let i = 1; i <= 2; i++) agents.push({...call('join', {name: `opencode-${i}`, vendor: 'opencode'}), vendor: 'opencode'});
  agents.push({...call('join', {name: 'raw-agent', vendor: 'raw'}), vendor: 'raw'});
  heartbeat = setInterval(() => { try { for (const agent of [controller, ...agents]) call('heartbeat', {}, agent.id); } catch (error) { report.heartbeatError = error.message; } }, 10000);
  db = new DatabaseSync(database, {readOnly: true});
  const skill = JSON.parse(execFileSync(binary, ['skill'], {encoding: 'utf8'})).instructions;
  report.skillSha256 = digest(skill); report.skillBytes = Buffer.byteLength(skill);
  const task = `${skill}\n\nAssigned interoperability task: Initially respond READY to the host only. For each incoming peer QUESTION, discover your peers and read handoff.md once per session, then reply exactly once with ANSWER, the document verification word, and your available communication capabilities. Use replyTo only; omit to/topic. Use key answer-ID with the incoming ID, reasoning and wake action. Acknowledge after the reply succeeds. Other messages are informational: acknowledge without replying. Native delivery is under test: do not call inbox or hook, poll, send readiness to peers, or start unrelated work. End your turn after handling delivered messages.`;
  call('share_document', {name: 'handoff.md', reasoning: 'Verify OpenCode can consume shared coordination evidence', content: 'Coordination test. Verification word: SAFFRON. Discover collaborators and preserve reply correlation.\n'}, controller.id);
  for (const agent of agents.filter(a => a.vendor === 'opencode')) {
    agent.endpoint = `http://127.0.0.1:${await port()}`;
    const password = randomBytes(24).toString('hex');
    agent.headers = authenticated ? {authorization: `Basic ${Buffer.from(`opencode:${password}`).toString('base64')}`} : {};
    const home = join(workspace, agent.name); mkdirSync(home, {recursive: true});
    const config = {model, small_model: model, share: 'disabled', autoupdate: false, snapshot: false, instructions: [], default_agent: 'build', agent: {title: {disable: true}, summary: {disable: true}}, mcp: {communication: {type: 'local', command: [binary, 'mcp', ...binding, '--session', agent.id], enabled: true}}};
    const env = {HOME: home, XDG_CONFIG_HOME: join(home, 'config'), XDG_DATA_HOME: join(home, 'data'), XDG_CACHE_HOME: join(home, 'cache'), OPENCODE_CONFIG_CONTENT: JSON.stringify(config), OPENCODE_DISABLE_AUTOUPDATE: 'true', OPENCODE_DISABLE_CLAUDE_CODE: 'true', OPENCODE_DISABLE_PROJECT_CONFIG: 'true', ...(authenticated ? {OPENCODE_SERVER_PASSWORD: password, OPENCODE_SERVER_USERNAME: 'opencode'} : {})};
    start(`${agent.name}-server`, opencode, ['serve', '--hostname', '127.0.0.1', '--port', new URL(agent.endpoint).port], env);
    await until(async () => { try { return await api(agent, '/global/health'); } catch { return false; } }, `${agent.name} ready`, 30000);
    const created = await api(agent, '/session', {title: agent.name}); agent.vendorSession = created.id;
    agent.preflight = await api(agent, `/session/${created.id}`);
    agent.initialStatus = await api(agent, '/session/status');
    assert.equal(realpathSync(agent.preflight.directory), workspace);
    assert.equal((await api(agent, '/mcp')).communication.status, 'connected');
    await watch(agent);
    const initialized = await api(agent, `/session/${created.id}/message`, {agent: 'build', parts: [{type: 'text', text: `${task}\nInitialize; respond READY to this host without sending peer messages.`}]});
    writeFileSync(join(output, `${agent.name}-initialization.json`), JSON.stringify(initialized));
    assert.ok(initialized.parts.some(part => part.type === 'text' && part.text.includes('READY')), `${agent.name} initialization`);
    call('attach', {transport: 'opencode', endpoint: agent.endpoint, vendorSession: created.id}, agent.id);
    agent.listener = start(`${agent.name}-listener`, binary, ['listen', ...binding, '--session', agent.id], authenticated ? {OPENCODE_SERVER_PASSWORD: password, OPENCODE_SERVER_USERNAME: 'opencode', OCTOCODE_OPENCODE_AUTH_ENDPOINT: agent.endpoint} : {});
    await until(() => agent.listener.stdout.includes('listening'), `${agent.name} listening`, 30000);
  }
  const raw = agents.find(a => a.vendor === 'raw'); call('attach', {transport: 'raw'}, raw.id);
  const native = agents.filter(a => a.vendor === 'opencode');
  const before = new Map(await Promise.all(native.map(async agent => [agent.id, (await api(agent, `/session/${agent.vendorSession}/message`)).filter(m => m.info.role === 'assistant').length])));
  const passive = agents.map(agent => call('send_message', {to: agent.id, body: 'FYI: passive insertion; acknowledge on the next actionable message, without replying.', reasoning: 'Verify passive delivery costs no model turn', key: `passive-${agent.id}`, wake: 'passive'}, controller.id));
  await until(() => db.prepare("SELECT count(*) n FROM dispatches WHERE transport='opencode' AND state='submitted'").get().n === 2, 'passive submissions');
  await delay(2000);
  for (const agent of native) assert.equal((await api(agent, `/session/${agent.vendorSession}/message`)).filter(m => m.info.role === 'assistant').length, before.get(agent.id), 'Passive mail must not start inference');
  report.passiveWakeGuard = true;
  const requests = [];
  for (const sender of agents) for (const recipient of agents.filter(a => a.id !== sender.id)) {
    const input = {to: recipient.id, body: `QUESTION ${sender.name}: which communication capabilities can you use? Refer to handoff.md.`, key: `question-${recipient.id}`, reasoning: 'Discover collaborator capabilities', wake: 'action', conversationId: `pair-${sender.name}-${recipient.name}`};
    const message = call('send_message', input, sender.id); assert.deepEqual(call('send_message', input, sender.id), message);
    requests.push({...message, sender: sender.id, recipient: recipient.id, conversationId: input.conversationId});
  }
  async function rawDrain() {
    for (;;) {
      const batch = call('hook', {format: 'json'}, raw.id); if (!batch.items.length) break;
      for (const message of batch.items) {
        if (message.body.startsWith('QUESTION')) call('send_message', {replyTo: message.id, body: 'ANSWER SAFFRON: DB-backed messages, documents, peers and leases.', key: `answer-${message.id}`, reasoning: 'Answer requested capabilities', wake: 'action'}, raw.id);
        call('ack', {message: message.id}, raw.id);
      }
    }
  }
  const began = performance.now();
  await until(async () => { await rawDrain(); return requests.every(request => db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=? AND recipient=?').get(request.id, request.recipient)?.acknowledgedAt); }, 'question acknowledgements');
  for (const request of requests) {
    const replies = db.prepare('SELECT * FROM messages WHERE sender=? AND replyTo=?').all(request.recipient, request.id);
    assert.equal(replies.length, 1); assert.equal(replies[0].target, request.sender); assert.equal(replies[0].conversationId, request.conversationId); assert.match(replies[0].body, /^ANSWER[\s\S]*SAFFRON/);
  }
  report.questionRoundMs = performance.now() - began;
  const broadcast = call('notify_all', {body: 'FYI: evaluation complete; acknowledge remaining messages and this notice, without replying.', key: 'done', reasoning: 'Confirm broadcast reaches every collaborator', wake: 'action'}, controller.id);
  assert.equal(broadcast.recipients, 3);
  await until(async () => { await rawDrain(); return db.prepare('SELECT count(*) n FROM deliveries WHERE acknowledgedAt IS NULL').get().n === 0; }, 'all deliveries handled');
  await until(() => db.prepare("SELECT count(*) n FROM dispatches WHERE state<>'submitted'").get().n === 0, 'dispatch receipts');
  await delay(2000);
  assert.equal(db.prepare('SELECT count(*) n FROM messages').get().n, requests.length * 2 + passive.length + 1);
  report.sessions = [];
  for (const agent of native) {
    const messages = await api(agent, `/session/${agent.vendorSession}/message`);
    const tools = messages.flatMap(m => m.parts).filter(p => p.type === 'tool');
    assert.ok(tools.some(p => p.tool === 'communication_read_document' && p.state.status === 'completed' && JSON.stringify(p.state.output).includes('SAFFRON')), 'Recipient read the handoff using its bound MCP tool');
    assert.ok(!tools.some(p => /(?:^|_)(inbox|hook)$/.test(p.tool)), 'No manual inbox bypass');
    const usage = messages.filter(m => m.info.role === 'assistant').map(m => ({id: m.info.id, tokens: m.info.tokens, cost: m.info.cost}));
    for (const observation of usage) if (observation.tokens) call('record_usage', {key: observation.id, scope: 'request', inputTokens: observation.tokens.input, outputTokens: observation.tokens.output, cachedInputTokens: observation.tokens.cache.read, cacheWriteTokens: observation.tokens.cache.write}, agent.id);
    report.sessions.push({name: agent.name, vendorSession: agent.vendorSession, preflight: agent.preflight, initialStatus: agent.initialStatus, usage, tools: tools.map(p => ({tool: p.tool, status: p.state.status})), events: agent.events.length});
    writeFileSync(join(output, `${agent.name}-messages.json`), JSON.stringify(messages));
    writeFileSync(join(output, `${agent.name}-events.json`), JSON.stringify(agent.events));
  }
  assert.equal(report.heartbeatError, undefined);
  report.passed = true; report.requestEdges = requests.length; report.replyEdges = requests.length; report.broadcastRecipients = broadcast.recipients;
  report.messages = db.prepare('SELECT count(*) n FROM messages').get().n;
  report.dispatches = db.prepare('SELECT transport,state,count(*) n FROM dispatches GROUP BY transport,state').all();
} catch (error) { report.error = error.stack; process.exitCode = 1; }
finally {
  clearInterval(heartbeat); for (const stream of streams) stream.abort();
  if (db) { report.pending = db.prepare('SELECT message,recipient FROM deliveries WHERE acknowledgedAt IS NULL').all(); report.audit = db.prepare('SELECT kind,count(*) n FROM audit GROUP BY kind').all(); }
  for (const item of children.reverse()) {
    try { process.kill(-item.child.pid, 'SIGTERM'); } catch {}
    const ended = Date.now() + 2000; while (item.child.exitCode === null && item.child.signalCode === null && Date.now() < ended) await delay(50);
    if (item.child.pid && item.child.exitCode === null && item.child.signalCode === null) {
      try { process.kill(-item.child.pid, 'SIGKILL'); } catch {}
      const deadline = Date.now() + 2000;
      while (item.child.exitCode === null && item.child.signalCode === null && Date.now() < deadline) await delay(50);
    }
    writeFileSync(join(output, `${item.name}-stdout.txt`), item.stdout); writeFileSync(join(output, `${item.name}-stderr.txt`), item.stderr);
  }
  for (const agent of agents.filter(a => a.vendor === 'opencode')) {
    writeFileSync(join(output, `${agent.name}-events.json`), JSON.stringify(agent.events ?? []));
    writeFileSync(join(output, `${agent.name}-preflight.json`), JSON.stringify({session: agent.preflight, status: agent.initialStatus}));
  }
  report.childrenReaped = children.every(item => !item.child.pid || item.child.exitCode !== null || item.child.signalCode !== null);
  if (!report.childrenReaped || children.some(item => item.error)) { report.passed = false; process.exitCode = 1; }
  for (const agent of [controller, ...agents].filter(Boolean)) { try { call('leave', {}, agent.id); } catch (error) { report.cleanupError = error.message; report.passed = false; process.exitCode = 1; } }
  db?.close();
  if (existsSync(database)) { try { execFileSync(binary, ['db', 'export', JSON.stringify({path: join(output, 'audit.sqlite')}), '--database', database]); } catch (error) { report.snapshotError = error.message; report.passed = false; process.exitCode = 1; } }
  report.completedAt = new Date().toISOString(); writeFileSync(join(output, 'result.json'), JSON.stringify(report, null, 2)); console.log(JSON.stringify({output, ...report}, null, 2));
}
