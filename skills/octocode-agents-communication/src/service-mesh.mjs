// Opt-in real-host test. Routing uses existing-recipient APIs, never a sender model.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, writeFileSync, existsSync, cpSync, readdirSync } from 'node:fs';
import { createServer, createConnection } from 'node:net';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { nativeTurnFinished, nativeTurnFailure } from './native-turns.mjs';
import { nativeResults, verifyContributionReads } from './collaboration-evidence.mjs';
import { installedBinary, verifyStartup } from './artifact-checks.mjs';
import { hookMessages } from './hook-messages.mjs';
import { interviewEvidence, grokInterviewIdle } from './interview-evidence.mjs';

const opencodeCommand = process.env.COMMUNICATION_OPENCODE_COMMAND?.trim();
const opencodeModel = process.env.COMMUNICATION_OPENCODE_MODEL ?? 'opencode/mimo-v2.6-flash-free';
const piModel = process.env.COMMUNICATION_PI_MODEL?.trim();
const explicitVendors = process.env.COMMUNICATION_VENDORS;
const vendors = explicitVendors === undefined ? ['claude', 'codex', 'grok', 'pi', ...(opencodeCommand ? ['opencode'] : [])] : explicitVendors.split(',').map(v => v.trim());
assert.ok(vendors.length && vendors.every(v => ['claude', 'codex', 'grok', 'pi', 'opencode'].includes(v)) && new Set(vendors).size === vendors.length, 'COMMUNICATION_VENDORS must contain distinct supported native vendors.');
const rawPeer = explicitVendors === undefined;
const agentOriginated = process.env.COMMUNICATION_AGENT_ORIGINATED === '1';
assert.ok(!agentOriginated || (!rawPeer && vendors.every(v => ['claude', 'codex', 'grok', 'pi'].includes(v))), 'Agent-originated collaboration requires explicit native vendors with tool receipts.');
const copies = Number(process.env.COMMUNICATION_COPIES ?? 2);
assert.ok([1, 2].includes(copies), 'COMMUNICATION_COPIES must be 1 or 2');
const interviewEnabled = process.env.COMMUNICATION_INTERVIEW === '1';
const completionCheck = process.env.COMMUNICATION_COMPLETION_CHECK === '1';
const scopedSkill = process.env.COMMUNICATION_SCOPED_SKILL === '1';
const taskFamily = process.env.COMMUNICATION_TASK_FAMILY ?? 'review';
const featureCheck = process.env.COMMUNICATION_FEATURE_CHECK === '1';
assert.ok(!featureCheck || (agentOriginated && taskFamily === 'review'), 'Feature check requires agent-originated review');
const initialization = 'This is a readiness check only. Reply READY as plain text and end the turn. No tools or work are authorized in this turn. A later native-delivered message with a numeric ID and body beginning START will assign work; this instruction is not that message.';
assert.ok(['review', 'handoff', 'code-review'].includes(taskFamily), 'Unknown task family');
const vendorCopies = JSON.parse(process.env.COMMUNICATION_VENDOR_COPIES ?? '{}');
assert.ok(vendorCopies && !Array.isArray(vendorCopies) && typeof vendorCopies === 'object', 'Vendor copies must be an object');
assert.ok(Object.entries(vendorCopies).every(([vendor,n]) => vendors.includes(vendor) && [1,2].includes(n)), 'Invalid vendor copy count');
const copiesFor = vendor => vendorCopies[vendor] ?? copies;
const codeReview = taskFamily === 'code-review';
const peerCount = vendors.reduce((sum,vendor) => sum + copiesFor(vendor), 0) + Number(rawPeer);
assert.ok(!agentOriginated || peerCount >= 2, 'Agent-originated collaboration requires at least two peers.');
assert.ok(!codeReview || agentOriginated, 'Code review requires native agent-originated collaboration');
const plan = { vendors, rawPeer, agentOriginated, featureCheck, peers: peerCount, controllerIdentities: 1, requestEdges: codeReview ? peerCount : peerCount * (peerCount - 1), routingModelCalls: 0 };
if (process.argv.includes('--plan')) { console.log(JSON.stringify(plan)); process.exit(0); }
if (vendors.includes('pi')) assert.ok(piModel, 'Set COMMUNICATION_PI_MODEL to an authenticated Pi model.');
if (vendors.includes('opencode')) assert.ok(opencodeCommand, 'Set COMMUNICATION_OPENCODE_COMMAND for OpenCode.');
const root = fileURLToPath(new URL('../', import.meta.url));
const output = resolve(process.env.COMMUNICATION_OUTPUT ?? join(root, '../../.octocode/benchmarks/communication-service-mesh/results', new Date().toISOString().replaceAll(':', '-')));
mkdirSync(output, { recursive: true });
const workspace = realpathSync(mkdtempSync('/tmp/communication-service-mesh-'));
const database = join(workspace, 'audit.sqlite');
if (featureCheck) execFileSync('git', ['init', '--quiet', workspace]);
const sourceBinary = process.env.COMMUNICATION_BINARY ?? installedBinary();
const binary = join(output, 'communication');
cpSync(sourceBinary, binary);
const selectedTools = featureCheck ? 'peers,set_status,send_message,inbox,complete,subscribe,lock,renew,unlock,lock_many,share_document,read_document,activity,context,locks' : agentOriginated ? 'peers,send_message,complete,read_document,share_document' + (codeReview ? ',set_status' : '') + (completionCheck ? ',inbox' : '') : process.env.COMMUNICATION_MINIMAL_TOOLS === '1' ? 'peers,send_message,complete,read_document' : undefined;
const digest = value => createHash('sha256').update(value).digest('hex');
const children = [], sockets = [], agents = [];
writeFileSync(join(output, 'harness.mjs'), readFileSync(fileURLToPath(import.meta.url)));
writeFileSync(join(output, 'native-turns.mjs'), readFileSync(new URL('./native-turns.mjs', import.meta.url)));
writeFileSync(join(output, 'collaboration-evidence.mjs'), readFileSync(new URL('./collaboration-evidence.mjs', import.meta.url)));
writeFileSync(join(output, 'interview-evidence.mjs'), readFileSync(new URL('./interview-evidence.mjs', import.meta.url)));
const report = { plan, passed: false, routingModelCalls: 0, receiverModelCalls: null, workspace, database, startedAt: new Date().toISOString(), binarySha256: digest(readFileSync(binary)), harnessSha256: digest(readFileSync(fileURLToPath(import.meta.url))), models: { claude: 'haiku', codex: 'gpt-6-luna', grok: 'grok-4.7-build-fast', pi: piModel }, evidence: 'Integration test, not a provider performance comparison or coding-quality evaluation. Receiver inference is real; routing creates no sender or relay model.' };
const piExtension = join(output, 'pi-adapter/pi-inbox.mjs');
if (vendors.includes('pi')) {
  mkdirSync(join(output, 'pi-adapter/hooks'), {recursive:true});
  report.piAdapterSha256 = {};
  for (const file of ['pi-inbox.mjs', 'pi-extension.mjs', 'hooks/lease-check.mjs']) {
    const contents = readFileSync(join(root, 'scripts', file));
    writeFileSync(join(output, 'pi-adapter', file), contents);
    report.piAdapterSha256[file] = digest(contents);
  }
}
const binding = ['--workspace', workspace, '--database', database];
const call = (command, input = {}, session) => JSON.parse(execFileSync(binary, [command, JSON.stringify(input), ...binding, ...(session ? ['--session', session] : [])], { encoding: 'utf8', timeout: 15000, stdio: ['pipe', 'pipe', 'pipe'] }));
report.completionCheck = { enabled: completionCheck, hosts: completionCheck ? ['claude', 'pi'] : [], policy: 'One bounded recovery per work cycle; no automatic message completion.' };
let aborted = false, cleaningUp = false;
process.once('SIGTERM', () => { aborted = true; });
const until = async (predicate, label, timeout = 120000) => {
  const end = Date.now() + timeout;
  while (Date.now() < end) { if (aborted && !cleaningUp) throw Error('Host requested bounded trial shutdown'); const value = await predicate(); if (value) return value; await delay(100); }
  throw Error(`Timed out: ${label}`);
};
function start(name, command, args, env = {}) {
  const child = spawn(command, args, { cwd: workspace, env: { ...process.env, ...env }, detached: true });
  const item = { name, child, events: [], stderr: '', stdout: '', send: value => child.stdin.write(`${JSON.stringify(value)}\n`) };
  children.push(item);
  child.stderr.on('data', data => { item.stderr = (item.stderr + data).slice(-16384); });
  child.on('error', error => { item.error = error.message; });
  child.stdin.on('error', () => {});
  createInterface({ input: child.stdout }).on('line', line => {
    try { item.events.push(JSON.parse(line)); } catch { item.stdout = (item.stdout + line + '\n').slice(-16384); }
  });
  return item;
}
async function connect(url) {
  const socket = new WebSocket(url); sockets.push(socket);
  await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }); });
  const events = []; let sequence = 0;
  socket.addEventListener('message', event => events.push(JSON.parse(event.data)));
  const send = value => socket.send(JSON.stringify(value));
  return { events, send, async request(method, params) {
    const id = ++sequence; send({ id, method, params });
    const reply = await until(() => events.find(event => event.id === id), method);
    if (reply.error) throw Error(JSON.stringify(reply.error));
    return reply.result;
  } };
}
async function connectGrok(path) {
  const socket = createConnection(path); sockets.push(socket);
  await new Promise((resolve, reject) => { socket.once('connect', resolve); socket.once('error', reject); });
  const events = []; let sequence = 0, buffer = Buffer.alloc(0), failure;
  const sendFrame = frame => { const body = Buffer.from(JSON.stringify(frame)); const header = Buffer.alloc(4); header.writeUInt32BE(body.length); socket.write(Buffer.concat([header, body])); };
  const send = frame => sendFrame({type: 'acp', payload: JSON.stringify({jsonrpc: '2.0', ...frame})});
  socket.on('error', error => { failure = error; });
  socket.on('data', chunk => {
    buffer = Buffer.concat([buffer, chunk]);
    while (buffer.length >= 4) {
      const size = buffer.readUInt32BE(0);
      if (size > 8 * 1024 * 1024) { failure = Error('Grok frame exceeds limit'); socket.destroy(); return; }
      if (buffer.length < size + 4) return;
      try {
        const frame = JSON.parse(buffer.subarray(4, size + 4)); buffer = buffer.subarray(size + 4);
        const event = frame.type === 'acp' ? JSON.parse(frame.payload) : frame; events.push(event);
        // The fixture owns authorization. Unexpected native approvals are a failing configuration.
        if (event.method && event.id != null) send({id: event.id, error: {code: -32601, message: 'Unexpected interactive request in communication fixture'}});
      } catch (error) { failure = error; socket.destroy(); return; }
    }
  });
  sendFrame({type: 'register', client_type: 'octocode-mesh-owner', mode: 'stdio', capabilities: {}});
  await until(() => { if (failure) throw failure; return events.some(e => e.type === 'registered' && e.ready && e.leader_protocol_version === 1) || events.some(e => e.type === 'leader_ready'); }, 'Grok leader registration');
  return {events, send, async request(method, params, timeout = 240000) {
    const id = ++sequence; send({id, method, params});
    const reply = await until(() => { if (failure) throw failure; return events.find(e => e.id === id && !e.method); }, method, timeout);
    if (reply.error) throw Error(JSON.stringify(reply.error));
    return reply.result;
  }};
}
async function openCodeApi(agent, path, data, timeoutMs = 120000) {
  const response = await fetch(`${agent.endpoint}${path}`, {method: data === undefined ? 'GET' : 'POST', headers: {'content-type': 'application/json', ...agent.headers}, ...(data === undefined ? {} : {body: JSON.stringify(data)}), signal: AbortSignal.timeout(timeoutMs)});
  const text = await response.text();
  assert.ok(response.ok, `${path}: ${response.status} ${text.slice(0, 500)}`);
  return text ? JSON.parse(text) : null;
}
// Reflection is a separate phase in the same native sessions, never a repair turn.
function coordinationSnapshot() {
  const state = Object.fromEntries(['messages', 'deliveries', 'documents'].map(table =>
    [table, db.prepare(`SELECT * FROM ${table} ORDER BY rowid`).all()]));
  const files = [];
  const walk = directory => {
    if (!existsSync(directory)) return;
    for (const entry of readdirSync(directory, {withFileTypes: true}).sort((a,b)=>a.name.localeCompare(b.name))) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.isFile()) files.push([path, digest(readFileSync(path))]);
    }
  };
  walk(join(workspace, '.octocode/communication'));
  return digest(JSON.stringify({state, files}));
}
async function interviewAgent(agent) {
  const prompt = 'The work evaluation is frozen and your communication identity has been revoked. This is a text-only retrospective in this same original session. Do not call any tools, send messages, finish pending work, edit or publish documents. In at most 250 words: explain the request/notice/complete distinction you actually observed; describe coordination and context costs; report your mistakes or ambiguities honestly; name the highest-value improvement. Separate features used from features merely read about. Do not claim unrun tests. Reply in plain text only.';
  const source = agent.rpc?.events ?? agent.process?.events ?? agent.events ?? [];
  const offset = source.length;
  let response;
  if (agent.vendor === 'claude') {
    agent.process.send({type:'user',message:{role:'user',content:prompt}});
    await until(()=>source.slice(offset).some(e=>e.type==='result'), `${agent.name} reflection`, 120000);
  } else if (agent.vendor === 'codex') {
    const started = await agent.rpc.request('turn/start', {threadId:agent.vendorSession,input:[{type:'text',text:prompt}]});
    await until(()=>source.slice(offset).some(e=>e.method==='turn/completed' && e.params?.turn?.id===started.turn.id), `${agent.name} reflection`, 120000);
  } else if (agent.vendor === 'grok') {
    response = await agent.rpc.request('session/prompt', {sessionId:agent.vendorSession,prompt:[{type:'text',text:prompt}],_meta:{verbatim:true}}, 120000);
  } else if (agent.vendor === 'pi') {
    agent.process.send({id:'reflection',type:'prompt',message:prompt});
    await until(()=>source.slice(offset).some(e=>e.type==='agent_settled'), `${agent.name} reflection`, 120000);
  } else if (agent.vendor === 'opencode') {
    response = await openCodeApi(agent, `/session/${agent.vendorSession}/message`, {parts:[{type:'text',text:prompt}]});
  } else throw Error('No native interview adapter');
  const events = source.slice(offset);
  const nativeFailure = nativeTurnFailure(agent.vendor, events, agent.vendorSession);
  if (nativeFailure) throw Error(nativeFailure);
  const {text, toolsUsed} = interviewEvidence(events, response);
  return {name:agent.name,vendor:agent.vendor,vendorSession:agent.vendorSession,text,toolsUsed,
    passed:Boolean(text)&&!toolsUsed,events,response};
}
async function interviewOriginalSessions() {
  const before=coordinationSnapshot();
  // Freeze readiness before stopping listeners or revoking identities. A queued
  // retrospective cannot become a continuation of unfinished evaluation work.
  const interviewIdle = new Map(agents.filter(agent => agent.vendor === 'grok')
    .map(agent => [agent.id, grokInterviewIdle(agent.rpc?.events ?? [], agent.vendorSession)]));
  // Freeze work verdict and evidence before changing lifecycle or asking anything.
  writeFileSync(join(output,'work-result.json'),JSON.stringify({...report,frozenAt:new Date().toISOString()},null,2),{flag:'wx'});
  execFileSync(binary,['db','export',JSON.stringify({path:join(output,'work-audit.sqlite')}),'--database',database],{stdio:'pipe'});
  const documentRoot=join(workspace,'.octocode/communication');
  if(existsSync(documentRoot))cpSync(documentRoot,join(output,'work-documents'),{recursive:true});
  for(const agent of agents)writeFileSync(join(output,`${agent.name}-work-events.json`),JSON.stringify(agent.rpc?.events??agent.process?.events??agent.events??[]),{flag:'wx'});
  for(const agent of agents) if(agent.listener){
    try{process.kill(-agent.listener.child.pid,'SIGTERM');}catch{}
    await until(()=>agent.listener.child.exitCode!==null||agent.listener.child.signalCode!==null,`${agent.name} delivery stopped`,5000);
  }
  for(const agent of agents)call('leave',{},agent.id);
  assert.equal(coordinationSnapshot(),before,'Work state changed while revoking interview mutation authority');
  const outcomes=await Promise.all(agents.filter(a=>a.vendor!=='raw').map(async agent=>{
    let result;
    if (agent.vendor === 'grok' && !interviewIdle.get(agent.id)) {
      result={name:agent.name,vendor:agent.vendor,vendorSession:agent.vendorSession,passed:false,
        unavailable:'unfinished_native_turn',error:'Native session was not verifiably idle at the frozen work boundary; no reflection prompt sent.'};
    } else try{result=await interviewAgent(agent);}catch(error){result={name:agent.name,vendor:agent.vendor,vendorSession:agent.vendorSession,passed:false,error:error.message};}
    writeFileSync(join(output,`${agent.name}-reflection.json`),JSON.stringify(result,null,2));
    return {name:result.name,vendor:result.vendor,passed:result.passed,error:result.error,unavailable:result.unavailable};
  }));
  const unchanged=before===coordinationSnapshot();
  report.interviews={separateFromWorkVerdict:true,originalSessions:true,sideEffectsUnchanged:unchanged,agents:outcomes,passed:unchanged&&outcomes.every(r=>r.passed)};
  assert.ok(unchanged,'Reflection must not mutate messages, deliveries or documents');
}

async function watchOpenCode(agent) {
  const abort = new AbortController(); sockets.push({close: () => abort.abort()});
  const response = await fetch(`${agent.endpoint}/event`, {headers: agent.headers, signal: abort.signal});
  assert.equal(response.status, 200); agent.events = [];
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

let db, timer, controller;
try {
  if (process.env.COMMUNICATION_EXPECTED_BINARY_SHA256) assert.equal(report.binarySha256, process.env.COMMUNICATION_EXPECTED_BINARY_SHA256, 'Use the approved frozen runtime');
  report.startup = verifyStartup(binary, {coldStart: true});
  report.vendorVersions = Object.fromEntries(vendors.filter(v => ['claude', 'codex', 'grok'].includes(v)).map(v => [v, execFileSync(v, ['--version'], {encoding:'utf8', timeout:10000}).trim()]));
  controller = call('join', { name: 'mesh-controller', vendor: 'test-host' });
  for (let n = 1; n <= 2; n++) for (const vendor of vendors) if (n <= copiesFor(vendor)) agents.push({ ...call('join', { name: `${vendor}-${n}`, vendor }), vendor });
  if (rawPeer) agents.push({ ...call('join', { name: 'raw-agent', vendor: 'raw' }), vendor: 'raw' });
  timer = setInterval(() => {
    try { for (const agent of [controller, ...agents]) call('heartbeat', {}, agent.id); }
    catch (error) { report.heartbeatError = error.message; }
  }, 10000);
  db = new DatabaseSync(database, { readOnly: true });
  const reviewManifest = codeReview ? JSON.parse(readFileSync(process.env.COMMUNICATION_REVIEW_MANIFEST, 'utf8')) : null;
  const destinations = sender => codeReview ? [agents[(agents.indexOf(sender) + 1) % agents.length]] : agents.filter(a => a.id !== sender.id);
  if (codeReview) {
    assert.deepEqual(Object.keys(reviewManifest.assignments).sort(), agents.map(a=>a.name).sort(), 'One real source packet per agent');
    report.reviewSources = [];
    for (const agent of agents) {
      const assignment = reviewManifest.assignments[agent.name];
      assert.ok(assignment.content && assignment.focus && assignment.files?.length, 'Review needs source and provenance');
      agent.sourceDocument = `source-${agent.name}.md`;
      call('share_document', {name:agent.sourceDocument,content:assignment.content,reasoning:`Source evidence for ${assignment.focus}`},controller.id);
      report.reviewSources.push({agent:agent.name,focus:assignment.focus,document:agent.sourceDocument,sha256:digest(assignment.content),files:assignment.files});
    }
  }
  const skill = JSON.parse(execFileSync(binary, ['skill'], { encoding: 'utf8' })).instructions;
  report.skillSha256 = digest(skill); report.skillBytes = Buffer.byteLength(skill);
  const task = `${skill}\n\nAssigned interoperability task: Initially respond READY to the host only. For each peer message whose body starts QUESTION, use peers once to discover the workspace collaborators and read shared handoff document mesh-context.md once per session, then call complete with message:<QUESTION-ID> and reply containing exactly one answer; the service resolves the recipient. Reply body must begin ANSWER and contain the document's verification word plus a brief truthful description of your available communication tools. Use complete({message:<QUESTION-ID>,reply:<answer>,reasoning:<brief purpose>}); reply and completion commit together, and failed replies remain pending. Do not pass key, wake, to or replyTo to complete. All other peer messages are informational: call complete with their IDs and no reply. Wake and reasoning do not turn an answer into a question. Automatic native delivery is under test: do not call inbox or hook, send readiness messages to peers, subscribe, broadcast, acquire leases, poll, or initiate other work. End each turn once its delivered messages are handled.`;
  const collaborationTask = `${skill}

Assigned group task: ${featureCheck ? 'Assess this communication skill from observed use' : taskFamily === 'review' ? 'Review risks of concurrent shared-repository edits' : 'Plan handoffs between implementation, review and validation owners'} using the exposed communication tools.
Collaborators: ${agents.map(a => a.name).join(', ')}. mesh-controller is only the test coordinator.
Initialize: respond READY to the host. Wait for START before contacting peers.
On START, complete these steps before acknowledging it:
1. Use the delivered directory; call peers if it is incomplete. Copy exact collaborator IDs.
2. Call read_document with name:"mesh-context.md"; use the returned content. This is a communication tool, not an MCP resource.
3. Call share_document with reasoning explaining why peers need this evidence to publish <your-name>-coordination.md: under 300 characters containing an original risk, mitigation, verification word. Describe only tools actually exposed to this session, not every capability mentioned in the skill. Read the sources before publishing: documents are immutable.
4. Send one QUESTION to each of the other ${agents.length - 1} collaborators. The body must begin QUESTION <your-name>: and request an improvement to your published document using the exact document.name returned by share_document. Keep this same document name in every outgoing QUESTION; only the recipient changes. Do not substitute the recipient's document. Use to:<discovered-ID>, key:question-<recipientID>, conversationId:mesh-<your-name>-<recipient-name>, wake:action and meaningful reasoning. These sends must be your own tool calls.
5. Call complete with message:<START-ID> only after all sends succeed; never send_message to mesh-controller.
On each QUESTION: read the named contributor document completely, following next to the terminal page; reuse that verified revision for later questions. Then call complete with message:<QUESTION-ID>, reply:<one useful ANSWER beginning ANSWER COPPER>, and no optional fields. Include an improvement and your exposed-tool description; no key, wake, to/topic or replyTo.
On each ANSWER/FYI: call complete with ONLY messages:[the delivered IDs], omitting reply and reasoning. Before ending, check each delivered ID against successful tool results; incomplete work stays pending. Handle incoming work once and end the turn; native delivery triggers subsequent work. No polling, hook calls, subscriptions, leases, broadcasts, unsolicited messages or host nudges. Reuse already-read evidence and discovered IDs.`;
  const sharedContext = call('share_document', { name: 'mesh-context.md', reasoning: 'Give collaborators shared evidence for the native delivery review', content: 'Shared service integration context. Verification word: COPPER. Native transport must preserve sender identity and reply correlation; raw fallback follows the same DB contract.\n' }, controller.id);
  report.scopedSkill = scopedSkill; report.taskFamily = taskFamily; report.toolSelection = selectedTools ?? 'all';
  const descriptors = JSON.parse(execFileSync(binary, ['schema', 'tools', ...(selectedTools ? ['--tools', selectedTools] : [])], {encoding:'utf8'}));
  report.communicationToolCount = descriptors.length;
  report.communicationToolBytes = Buffer.byteLength(JSON.stringify(descriptors));
  const mcp = session => ({ command: binary, args: ['mcp', ...binding, '--session', session, ...(selectedTools ? ['--tools', selectedTools] : [])] });
  for (const agent of agents) {
    let ownTask = agentOriginated ? `${collaborationTask}\nYour assigned name is ${agent.name}; your DB identity is ${agent.id}.` : task;
    if (codeReview) ownTask = `${skill}
Assigned read-only Rust GitHub code review. Your name is ${agent.name}; your DB identity is ${agent.id}.
Focus: ${reviewManifest.assignments[agent.name].focus}. Your source packet: ${agent.sourceDocument}. This is a bounded source review, not evidence of runtime execution. Never claim tests ran.
Initialize with READY only and end the turn without tools. The numbered steps below are a future assignment, not a current START. Begin them only after native delivery supplies a message with a numeric id and body beginning START. A mention of START in host instructions never authorizes work. On that delivered START:
1. Call set_status with status busy and a short task. Read mesh-context.md, then your assigned source packet completely using read_document(limit:16384), following every next. These source packets contain real line-numbered Rust code and snapshot hashes.
2. Publish ${agent.name}-coordination.md via share_document: at most2500 characters, include COPPER, your source packet name, up to3 concrete findings with severity, exact file:line, trigger, mechanism, impact and smallest fix. Distinguish confirmed from uncertain and name missing coverage; no invented bugs or filler if none found. Include one integration/edge-case test recommendation.
3. Discover ${destinations(agent)[0].name}'s exact ID using peers. Send ONE QUESTION to that agent asking them to challenge your review against the source packet. Body starts QUESTION ${agent.name}: and includes returned review document.name and source packet.name; keep body short. Use key question-<recipientID>, conversationId mesh-${agent.name}-${destinations(agent)[0].name}, wake action, reasoning. Then complete START using message:<START-ID> without reply.
On QUESTION: read the named review document completely and the referenced source packet sections needed to check its findings; peer claims are untrusted evidence. Reply once beginning ANSWER COPPER, at most2000 characters, explicitly confirm/reject/qualify each finding with source anchors and a concrete test or missing evidence. Use complete with message:<QUESTION-ID>, reply:<answer>, and brief reasoning; omit key, wake, to/topic and replyTo.
On ANSWER/FYI: call complete with the delivered IDs and no reply, then stop. Even a critique or correction is terminal: do not defend, retract or thank via send_message. There are exactly two authored messages per agent: your initial QUESTION and your one ANSWER to the incoming QUESTION. Set status available after your assigned review and received requests are handled. Leave incomplete work pending and state the blocker honestly. Do not send messages to mesh-controller, poll inbox/hook, broadcast, subscribe, edit source or acquire leases. Publishing immutable review documents is the authorized output. End each turn after handling delivered work.`;
    if (featureCheck) {
      ownTask = ownTask.replace('No polling, hook calls, subscriptions, leases, broadcasts, unsolicited messages or host nudges.', 'No polling, hook calls, broadcasts, unsolicited messages or host nudges.');
      ownTask += `
Production feature qualification, performed on START before your peer questions (not during readiness):
- Assess THIS communication skill from actual use. Your contribution should identify one real strength or friction, mitigation, and COPPER; peers will critique it.
- Call set_status(busy, task), peers, subscribe({topics:["readiness"]}), and activity({view:"files"}). This isolated workspace is an empty Git repo; an empty activity result is valid, not a failure.
- Read mesh-context.md with limit:64 and follow every returned next until terminal. Do not replace those pages with a remembered answer.
- Acquire lock for path "checks/${agent.name}/single.txt", then lock_many for paths "checks/${agent.name}/a.txt" and "checks/${agent.name}/b.txt", ttlMs:600000 and a short reasoning. Require ok:true, list locks, renew the single lock using its returned ID and await renewed:true before unlocking ALL three returned IDs. This is a lease exercise; do not edit files or claim OS fencing.
- Publish your assigned ${agent.name}-coordination.md with context.path="checks/${agent.name}" and context.kind="tree"; write context.summary as your actual observed finding. Call context({path:"checks/${agent.name}"}) and verify your publication is discoverable. Keep content under 300 characters.
- Then send the seven peer QUESTIONs as assigned and complete START. At the end of each handled work turn set_status available. Handle later peer requests normally and keep answers concise.
- The controller will send a topic FYI after everyone subscribed. Complete it silently like other notices. Never broadcast or send messages to the controller.
Your binding exposes exactly these communication tools: ${selectedTools}. Use bound tools; no CLI or host-native substitute is needed.`;
    }
    if (scopedSkill) {
      const profile = JSON.parse(execFileSync(binary, ['skill', '--vendor', agent.vendor], {encoding:'utf8'})).instructions;
      ownTask = ownTask.replace(skill, profile);
      agent.profile = {bytes:Buffer.byteLength(profile),sha256:digest(profile)};
    }
    if (completionCheck) ownTask += '\nA configured completion check may report pending IDs. Handle known bodies from existing context. Only if missing, recover that one body with inbox(message:ID); never poll or re-read the whole inbox. A blocked task can remain pending with an honest explanation.';
    if (agent.vendor === 'claude') {
      agent.vendorSession = randomUUID();
      const endpoint = join(workspace, `${agent.name}.sock`);
      call('attach', { transport: 'claude', endpoint, vendorSession: agent.vendorSession }, agent.id);
      const quote = value => "'" + value.replaceAll("'", "'\\''") + "'";
      const hookCommand = [binary, 'completion-check', '-', ...binding, '--session', agent.id].map(quote).join(' ');
      const settings = {disableAllHooks:!completionCheck,autoMemoryEnabled:false,crossSessionInbound:'accept',...(completionCheck ? {hooks:{Stop:[{hooks:[{type:'command',command:hookCommand,timeout:10}]}]}} : {})};
      agent.process = start(agent.name, 'claude', ['-p', '--session-id', agent.vendorSession, ...(completionCheck ? ['--include-hook-events'] : []), '--model', 'haiku', '--input-format', 'stream-json', '--output-format', 'stream-json', '--verbose', '--setting-sources', '', '--strict-mcp-config', '--mcp-config', JSON.stringify({ mcpServers: { communication: mcp(agent.id) } }), '--tools', '', '--allowedTools', 'mcp__communication__*', '--permission-mode', 'dontAsk', '--disable-slash-commands', '--no-session-persistence', '--messaging-socket-path', endpoint, '--settings', JSON.stringify(settings), '--system-prompt', ownTask]);
      agent.process.send({ type: 'user', message: { role: 'user', content: initialization } });
      await until(() => agent.process.events.some(e => e.type === 'result'), `${agent.name} initialization`);
      agent.vendorSession = agent.process.events.find(e => e.type === 'system' && e.subtype === 'init').session_id;
      call('attach', { transport: 'claude', endpoint, vendorSession: agent.vendorSession }, agent.id);
    } else if (agent.vendor === 'codex') {
  const reserve = createServer(); await new Promise(resolve => reserve.listen(0, '127.0.0.1', resolve));
  const port = reserve.address().port; await new Promise(resolve => reserve.close(resolve));
  start(`${agent.name}-server`, 'codex', ['app-server', '--listen', `ws://127.0.0.1:${port}`]);
  await until(async () => { try { return (await fetch(`http://127.0.0.1:${port}/readyz`)).ok; } catch { return false; } }, 'Codex server');
  const cx = agent.rpc = await connect(`ws://127.0.0.1:${port}`);
  await cx.request('initialize', { clientInfo: { name: 'service-mesh-owner', version: '1' }, capabilities: { experimentalApi: true } });
  cx.send({ method: 'initialized', params: {} });
  const { config } = await cx.request('config/read', { includeLayers: false });
  const disabled = value => Object.fromEntries(Object.keys(value || {}).map(key => [key, { enabled: false }]));
  const skills = await cx.request('skills/list', { cwds: [workspace], forceReload: true });

      const { thread } = await cx.request('thread/start', { model: 'gpt-6-luna', cwd: workspace, ephemeral: true, approvalPolicy: 'never', sandbox: 'read-only', baseInstructions: ownTask, developerInstructions: '', config: {
        mcp_servers: { ...disabled(config.mcp_servers), communication: { ...mcp(agent.id), enabled: true } }, plugins: disabled(config.plugins), project_doc_max_bytes: 0,
        skills: { config: skills.data.flatMap(entry => entry.skills.map(s => ({ path: s.path, enabled: false }))) }, web_search: 'disabled',
        features: { code_mode: { enabled: false }, shell_tool: false, apply_patch_freeform: false, multi_agent: false, memories: false, hooks: false, apps: false, skill_search: false },
      } });
      agent.vendorSession = thread.id;
      const inventory = await until(async () => {
        const response = await cx.request('mcpServerStatus/list', { threadId: thread.id, detail: 'toolsAndAuthOnly' });
        const server = response.data.find(server => server.name === 'communication');
        return server && !server.toolsError && Object.values(server.tools).some(tool => tool.name === 'send_message') ? server : false;
      }, `${agent.name} tool inventory`);
      agent.toolInventory = Object.values(inventory.tools).map(tool => tool.name);
      for (const name of ['peers', 'read_document', 'send_message', 'complete']) assert.ok(agent.toolInventory.includes(name), `${agent.name} missing ${name}`);
      await cx.request('turn/start', {threadId: thread.id, input: [{type: 'text', text: initialization}]});
      await until(() => {
        const failure = nativeTurnFailure('codex', cx.events, thread.id);
        if (failure) throw Error(`${agent.name}: ${failure}`);
        return nativeTurnFinished('codex', cx.events, thread.id);
      }, `${agent.name} initialization`);
      assert.ok(cx.events.some(e => e.method === 'item/completed' && e.params?.threadId === thread.id && e.params.item?.type === 'agentMessage' && e.params.item.text.includes('READY')), `${agent.name} initialization response`);
      call('attach', { transport: 'codex', endpoint: `ws://127.0.0.1:${port}`, vendorSession: thread.id }, agent.id);
    } else if (agent.vendor === 'grok') {
      const endpoint = join(workspace, `${agent.name}.sock`);
      start(`${agent.name}-server`, 'grok', ['agent', '--leader-socket', endpoint, 'leader', '--no-auto-update', '--relay-on-demand', '--no-exit-on-disconnect']);
      await until(() => existsSync(endpoint), `${agent.name} leader socket`);
      const gx = agent.rpc = await connectGrok(endpoint);
      const initialized = await gx.request('initialize', {protocolVersion: 1, clientInfo: {name: 'octocode-mesh-owner', version: '1'}, clientCapabilities: {}});
      agent.vendorInfo = initialized.agentInfo;
      const created = await gx.request('session/new', {cwd: workspace, mcpServers: [{name: 'communication', ...mcp(agent.id), env: []}], _meta: {
        modelId: 'grok-4.7-build-fast', yoloMode: true, systemPromptOverride: `${ownTask}\n\nGrok tool bridge: call use_tool with tool_name communication__<command> and tool_input containing that command's JSON. Use only the selected communication tools${selectedTools ? ': ' + selectedTools : ''}, respecting their schemas. send_message starts requests using to; complete({message:ID,reply:text}) answers and completes requests atomically. complete({messages:[IDs]}) handles informational messages without replies. These tools are bound to your identity; no session argument is needed.`,
        agentProfile: {name: 'communication', description: 'Bound communication receiver', tools: ['use_tool'], disallowedTools: ['Agent(*)'], skills: [], discoverSkills: false, agentsMd: false, injectDefaultTools: false},
      }});
      agent.vendorSession = created.sessionId;
      if (agentOriginated) {
        try { agent.metadataProbe = await gx.request('_x.ai/session/info', {sessionId: created.sessionId}, 5000); }
        catch (error) { agent.metadataProbe = {error: error.message}; }
      }
      agent.sessionConfig = created.configOptions;
      await gx.request('session/prompt', {sessionId: created.sessionId, prompt: [{type: 'text', text: initialization}], _meta: {verbatim: true}});
      call('attach', {transport: 'grok', endpoint, vendorSession: created.sessionId}, agent.id);
    } else if (agent.vendor === 'opencode') {
      const reserve = createServer(); await new Promise(r => reserve.listen(0, '127.0.0.1', r));
      const port = reserve.address().port; await new Promise(r => reserve.close(r));
      agent.endpoint = `http://127.0.0.1:${port}`;
      const password = randomBytes(24).toString('hex');
      agent.headers = {authorization: `Basic ${Buffer.from(`opencode:${password}`).toString('base64')}`};
      agent.listenerEnv = {OPENCODE_SERVER_PASSWORD: password, OPENCODE_SERVER_USERNAME: 'opencode', OCTOCODE_OPENCODE_AUTH_ENDPOINT: agent.endpoint};
      const home = join(workspace, agent.name); mkdirSync(home, {recursive: true});
      const config = {model: opencodeModel, small_model: opencodeModel, share: 'disabled', autoupdate: false, snapshot: false, instructions: [], default_agent: 'build', agent: {title: {disable: true}, summary: {disable: true}}, mcp: {communication: {type: 'local', command: [binary, ...mcp(agent.id).args], enabled: true}}};
      agent.server = start(`${agent.name}-server`, opencodeCommand, ['serve', '--hostname', '127.0.0.1', '--port', String(port)], {HOME: home, XDG_CONFIG_HOME: join(home, 'config'), XDG_DATA_HOME: join(home, 'data'), XDG_CACHE_HOME: join(home, 'cache'), OPENCODE_CONFIG_CONTENT: JSON.stringify(config), OPENCODE_DISABLE_AUTOUPDATE: 'true', OPENCODE_DISABLE_CLAUDE_CODE: 'true', OPENCODE_DISABLE_PROJECT_CONFIG: 'true', ...agent.listenerEnv});
      await until(async () => {
        assert.ok(agent.server.child.exitCode === null && agent.server.child.signalCode === null && !agent.server.error, `${agent.name} server exited before readiness: ${agent.server.error ?? agent.server.stderr}`);
        try { return await openCodeApi(agent, '/global/health', undefined, 1000); } catch (error) { agent.readinessError = error.message; return false; }
      }, `${agent.name} server`, 30000);
      const created = await openCodeApi(agent, '/session', {title: agent.name}); agent.vendorSession = created.id;
      agent.sessionMetadata = await openCodeApi(agent, `/session/${created.id}`);
      assert.equal(realpathSync(agent.sessionMetadata.directory), workspace);
      assert.equal((await openCodeApi(agent, '/mcp')).communication.status, 'connected');
      await watchOpenCode(agent);
      const initialized = await openCodeApi(agent, `/session/${created.id}/message`, {agent: 'build', parts: [{type: 'text', text: `${task}\n${initialization}`}]});
      if (initialized.info?.error) throw Error(`${agent.name}: ${JSON.stringify(initialized.info.error)}`);
      assert.ok(initialized.parts.some(p => p.type === 'text' && p.text.includes('READY')), `${agent.name} initialization`);
      call('attach', {transport: 'opencode', endpoint: agent.endpoint, vendorSession: created.id}, agent.id);
      report.models.opencode = opencodeModel;
      report.opencodeVersion = execFileSync(opencodeCommand, ['--version'], {encoding: 'utf8'}).trim();
    } else if (agent.vendor === 'pi') {
      agent.process = start(agent.name, 'pi', ['--mode', 'rpc', '--model', piModel, '--thinking', 'off', '--system-prompt', ownTask, '--session', join(workspace, `${agent.name}.jsonl`), '-ne', '--no-skills', '--no-prompt-templates', '--no-context-files', '--no-builtin-tools', '--extension', piExtension], { OCTOCODE_COMMUNICATION_BINDING: JSON.stringify({ binary, workspace, database, session: agent.id, disableCacheWarming: true, completionCheck, ...(selectedTools ? {tools:selectedTools} : {}) }) });
      await until(() => db.prepare('SELECT 1 FROM attachments WHERE session=?').get(agent.id), `${agent.name} extension`);
      agent.process.send({ id: 'startup', type: 'prompt', message: initialization });
      await until(() => agent.process.events.some(e => e.type === 'agent_settled'), `${agent.name} initialization`);
    } else call('attach', { transport: 'raw' }, agent.id);
  }
  assert.equal(db.prepare('SELECT count(*) n FROM messages').get().n, 0, 'No unsolicited startup messages');
  // Start the production delivery owner, then prove passive mail cannot start work.
  for (const agent of agents.filter(a => ['claude', 'codex', 'grok', 'opencode'].includes(a.vendor))) {
    agent.listener = start(`${agent.name}-listener`, binary, ['listen', ...binding, '--session', agent.id], agent.listenerEnv ?? {});
    await until(() => agent.listener.events.some(e => e.type === 'listening'), `${agent.name} listener`);
  }
  const completionCount = agent => agent.vendor === 'codex'
    ? agent.rpc.events.filter(e => e.method === 'turn/started' && e.params.threadId === agent.vendorSession).length
    : agent.vendor === 'grok' ? agent.rpc.events.filter(e => e.method === 'session/update' && e.params.update?.sessionUpdate === 'agent_message_chunk').length
    : agent.vendor === 'opencode' ? new Set(agent.events.filter(e => e.type === 'message.updated' && e.properties?.info?.role === 'assistant').map(e => e.properties.info.id)).size
    : agent.process.events.filter(e => agent.vendor === 'pi' ? e.type === 'agent_start' : e.type === 'assistant').length;
  const beforePassive = new Map(agents.filter(a => a.vendor !== 'raw').map(a => [a.id, completionCount(a)]));
  const passive = agents.map(agent => call('send_message', { to: agent.id, replyRequired: false, body: 'FYI: passive wake guard; acknowledge when other work wakes you, without replying.', key: `passive-${agent.id}`, reasoning: 'Verify informational mail never starts a model turn', wake: 'passive' }, controller.id));
  await delay(2500);
  for (const agent of agents.filter(a => a.vendor !== 'raw')) assert.equal(completionCount(agent), beforePassive.get(agent.id), `${agent.name} passive mail started work`);
  report.passiveWakeGuard = true;
  report.hostPromptsAfterStartup = 0;
  const requests = [];
  if (!agentOriginated) for (const sender of agents) for (const recipient of agents.filter(a => a.id !== sender.id)) {
    const input = { to: recipient.id, body: `QUESTION ${sender.name}: which communication tools can you use? Refer to mesh-context.md.`, key: `question-${recipient.id}`, reasoning: 'Discover collaborator capabilities before coordinating work', wake: 'action', conversationId: `mesh-${sender.name}-${recipient.name}` };
    const sent = call('send_message', input, sender.id);
    assert.deepEqual(call('send_message', input, sender.id), sent);
    requests.push({ ...sent, sender: sender.id, recipient: recipient.id, conversationId: input.conversationId });
  }
  const starts = agentOriginated ? agents.map(agent => call('send_message', {to: agent.id, replyRequired: false, body: `START: discover collaborators, publish your contribution and originate the ${destinations(agent).length} requested peer questions. When all sends succeed, call complete for this START ID without reply. Also complete every other handled FYI/answer delivered with it; leave unfinished requests pending. This message requires no reply; do not send a completion/status message.`, key: `start-${agent.id}`, wake: 'action', reasoning: `Start the authorized ${agents.length}-agent ${taskFamily} and capability exchange`}, controller.id)) : [];
  const raw = agents.find(agent => agent.vendor === 'raw');
  async function rawDrain() {
    if (!raw) return;
    for (;;) {
      const batch = call('hook', { format: 'json' }, raw.id);
      if (!batch.items.length) break;
      for (const message of hookMessages(batch)) {
        if (message.body.startsWith('QUESTION')) call('complete', { message: message.id, reply: 'ANSWER COPPER: DB-backed messages, peers, documents and advisory path leases.', reasoning: 'Answer the peer capability question and unblock its requester' }, raw.id);
        else call('complete', { message: message.id }, raw.id);
      }
    }
  }
  async function drainUntil(predicate, label) {
    await until(async () => {
      for (const child of children) {
        if (child.error || child.child.exitCode !== null || child.child.signalCode !== null) throw Error(`${child.name} stopped: ${child.error ?? child.stderr}`);
      }
      for (const agent of agents.filter(a => ['claude', 'codex'].includes(a.vendor))) {
        const failure = nativeTurnFailure(agent.vendor, agent.vendor === 'claude' ? agent.process.events : agent.rpc.events, agent.vendorSession);
        if (failure) throw Error(`${agent.name}: ${failure}`);
      }
      await rawDrain();
      const unsolicited = db.prepare('SELECT m.id,m.sender FROM messages m JOIN deliveries d ON d.message=m.id WHERE d.recipient=?').all(controller.id);
      assert.equal(unsolicited.length, 0, `Unexpected peer messages to the controller: ${JSON.stringify(unsolicited)}; handled messages must use complete without reply`);
      return predicate();
    }, label, 240000);
  }
  const began = performance.now();
  if (agentOriginated) {
    await drainUntil(() => db.prepare("SELECT count(*) n FROM messages WHERE body LIKE 'QUESTION%'").get().n >= plan.requestEdges, 'all native agents originate their own directed questions');
    requests.push(...db.prepare("SELECT id,sender,target AS recipient,conversationId,body FROM messages WHERE body LIKE 'QUESTION%' ORDER BY id").all());
    assert.equal(requests.length, plan.requestEdges);
    const edges = new Set(requests.map(r => `${r.sender}:${r.recipient}`));
    for (const sender of agents) for (const recipient of destinations(sender)) assert.ok(edges.has(`${sender.id}:${recipient.id}`), `Missing native-originated edge ${sender.name} -> ${recipient.name}`);
    report.agentOriginatedRequests = requests.length;
  }
  await drainUntil(() => requests.every(request => db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=? AND recipient=?').get(request.id, request.recipient)?.acknowledgedAt), 'all cross-vendor questions acknowledged by message-triggered turns');
  report.replyFormatDeviations = [];
  report.replyCountDeviations = [];
  // Response quality remains a failing gate, evaluated after broadcast coverage.
  for (const request of requests) {
    const replies = db.prepare('SELECT * FROM messages WHERE sender=? AND replyTo=?').all(request.recipient, request.id);
    if (replies.length !== 1) report.replyCountDeviations.push({replyTo:request.id,sender:request.recipient,count:replies.length});
    for (const reply of replies) {
      assert.equal(reply.target, request.sender);
      assert.equal(reply.conversationId, request.conversationId);
      if (!reply.body.startsWith('ANSWER') || !reply.body.includes('COPPER')) report.replyFormatDeviations.push({message:reply.id,replyTo:request.id,sender:request.recipient});
    }
  }
  report.questionRoundMs = performance.now() - began;
  // An explicit actionable broadcast closes the exchange for every recipient.
  let topicNotice;
  if (featureCheck) {
    assert.equal(db.prepare("SELECT count(*) n FROM subscriptions WHERE topic='readiness'").get().n, agents.length, 'Every agent subscribed through its native tool');
    topicNotice=call('send_message',{topic:'readiness',body:'FYI: subscription delivery verified; complete silently.',replyRequired:false,key:'topic-qualification',wake:'action',reasoning:'Verify topic delivery to all eight native subscribers'},controller.id);
    assert.equal(topicNotice.recipients,agents.length);
    report.topicRecipients=topicNotice.recipients;
  }
  const notice = call('notify_all', { body: 'FYI: mesh complete; handle pending answers and acknowledge this notice without replying.', key: 'final-broadcast', reasoning: 'Close the interoperability exchange and confirm all recipients can receive fanout', wake: 'action' }, controller.id);
  assert.equal(notice.recipients, agents.length);
  assert.deepEqual(call('notify_all', { body: 'FYI: mesh complete; handle pending answers and acknowledge this notice without replying.', key: 'final-broadcast', reasoning: 'Close the interoperability exchange and confirm all recipients can receive fanout', wake: 'action' }, controller.id), notice);
  await drainUntil(() => db.prepare('SELECT count(*) n FROM deliveries WHERE acknowledgedAt IS NULL').get().n === 0, 'all answers and broadcast acknowledged without host prompts');
  await until(() => db.prepare("SELECT count(*) n FROM dispatches WHERE state<>'submitted'").get().n === 0, 'all native submission receipts completed');
  report.handledRoundMs = performance.now() - began;
  await until(() => agents.filter(a => a.vendor === 'grok').every(a => a.listener.events.some(e => e.messages?.includes(notice.id))), 'Grok final receipt and usage audit flushed');
  await until(() => agents.filter(a => ['claude', 'codex'].includes(a.vendor)).every(a => nativeTurnFinished(a.vendor, a.vendor === 'claude' ? a.process.events : a.rpc.events, a.vendorSession)), 'native result/usage after the final handled message');
  report.nativeTurnsFinished = true;
  await delay(1000);
  report.deliveryClosurePassed = true;
  // Grade content only after native turn and message completion, so a bad answer cannot
  // strand otherwise handled mail or truncate late native tool receipts.
  const hasDocument = result => {
    for (const block of Array.isArray(result?.content) ? result.content : []) {
      try { const value = JSON.parse(block.text); if (value.document?.name === 'mesh-context.md' && value.content?.includes('COPPER')) return true; } catch {}
    }
    return false;
  };
  report.documentReaders = [];
  for (const agent of agents.filter(a => a.vendor !== 'raw')) {
    let read = false;
    if (agent.vendor === 'codex') read = agent.rpc.events.some(e => e.method === 'item/completed' && e.params.threadId === agent.vendorSession && e.params.item?.tool === 'read_document' && hasDocument(e.params.item.result));
    if (agent.vendor === 'pi') read = agent.process.events.some(e => e.type === 'tool_execution_end' && e.toolName === 'read_document' && !e.isError && hasDocument(e.result));
    if (agent.vendor === 'grok') read = agent.rpc.events.some(e => {
      const update = e.params?.update, result = update?.rawOutput;
      if (e.method !== 'session/update' || e.params.sessionId !== agent.vendorSession || update?.status !== 'completed' || result?.type !== 'MCP' || result.server_name !== 'communication' || result.tool_name !== 'read_document') return false;
      try {
        const value = JSON.parse(result.output?.OkayOutput);
        return value.document?.name === 'mesh-context.md' && value.content?.includes('COPPER');
      } catch { return false; }
    });
    if (agent.vendor === 'opencode') {
      agent.nativeMessages = await openCodeApi(agent, `/session/${agent.vendorSession}/message`);
      read = agent.nativeMessages.flatMap(m => m.parts).some(p => p.type === 'tool' && p.tool === 'communication_read_document' && p.state.status === 'completed' && JSON.stringify(p.state.output).includes('COPPER'));
    }
    if (agent.vendor === 'claude') {
      const calls = new Set(agent.process.events.flatMap(e => e.message?.content ?? []).filter(b => b.type === 'tool_use' && b.name.endsWith('__read_document')).map(b => b.id));
      read = agent.process.events.flatMap(e => e.message?.content ?? []).some(b => b.type === 'tool_result' && calls.has(b.tool_use_id) && !b.is_error && hasDocument(b));
    }
    assert.ok(read, `${agent.name} must successfully read the referenced document; copying another peer's answer is not evidence`);
    report.documentReaders.push(agent.name);
  }
  if (agentOriginated) {
    report.collaborators = [];
    const recordsByAgent = new Map(agents.map(agent => [agent.id, nativeResults(agent)]));
    report.contributionReads = verifyContributionReads(requests, recordsByAgent);
    if (featureCheck) {
      report.featureChecks = agents.map(agent => {
        const records = recordsByAgent.get(agent.id);
        const required = ['peers','set_status','subscribe','activity','lock','lock_many','locks','renew','unlock','share_document','context','read_document','send_message','complete'];
        for (const name of required) assert.ok(records.some(r=>r.name===name), `${agent.name} needs successful native ${name} evidence`);
        for (const name of ['lock','lock_many']) assert.ok(records.some(r=>r.name===name && r.value?.ok===true), `${agent.name} acquired ${name}`);
        assert.ok(records.filter(r=>r.name==='unlock' && r.value?.released===true).length>=3, `${agent.name} released all three leases`);
        assert.equal(db.prepare('SELECT count(*) n FROM leases WHERE owner=?').get(agent.id).n,0, `${agent.name} left no leases`);
        const pages=records.filter(r=>r.name==='read_document' && r.value?.document?.name==='mesh-context.md' && r.value.document.sha256===sharedContext.document.sha256).map(r=>r.value).sort((a,b)=>a.offset-b.offset);
        let covered=0;for(const page of pages){if(page.offset>covered)break;covered=Math.max(covered,page.offset+Buffer.byteLength(page.content));}
        assert.equal(covered,sharedContext.document.bytes, `${agent.name} read all context pages`);
        assert.ok(pages.length>=2 && pages.some(p=>!p.next), `${agent.name} exercised pagination to completion`);
        assert.ok(records.some(r=>r.name==='context' && JSON.stringify(r.value).includes(`${agent.name}-coordination.md`)), `${agent.name} discovered its shared context`);
        return {name:agent.name,successfulTools:required,contextBytesRead:covered,contextPages:pages.length,leasesReleased:3};
      });
    }

    if (codeReview) {
      report.reviewOutputs = [];
      for (const agent of agents) {
        const records = recordsByAgent.get(agent.id);
        const pages = records.filter(r=>r.name==='read_document' && r.value?.document?.name===agent.sourceDocument).map(r=>r.value).sort((a,b)=>a.offset-b.offset);
        let covered=0; for(const page of pages){ if(page.offset>covered)break; covered=Math.max(covered,page.offset+Buffer.byteLength(page.content)); }
        assert.equal(covered,Buffer.byteLength(reviewManifest.assignments[agent.name].content),`${agent.name} read its full assigned source`);
        assert.ok(records.some(r=>r.name==='set_status'),`${agent.name} successfully declared status through native tools`);
        const contribution=records.find(r=>r.name==='share_document' && r.value?.document?.author===agent.id)?.value.document;
        assert.ok(contribution,`${agent.name} published review`);
        report.reviewOutputs.push({agent:agent.name,document:contribution.name,sourceBytesRead:covered,peerCritiques:db.prepare('SELECT sender,body FROM messages WHERE replyTo IN (SELECT id FROM messages WHERE sender=?)').all(agent.id)});
      }
    }

    for (const agent of agents) {
      const records = recordsByAgent.get(agent.id), sentIds = new Set(records.filter(r => r.name === 'send_message').map(r => r.value?.id));
      const completedReplyIds = new Set(records.filter(r => r.name === 'complete' && r.value?.completed === true && Number.isSafeInteger(r.value?.id)).map(r => r.value.id));
      const answers = db.prepare('SELECT id FROM messages WHERE sender=? AND replyTo IS NOT NULL').all(agent.id);
      assert.ok(answers.every(answer => completedReplyIds.has(answer.id)), `${agent.name} final replies need successful native complete receipts`);
      const authored = requests.filter(r => r.sender === agent.id);
      assert.ok(authored.every(r => sentIds.has(r.id)), `${agent.name} requests need successful native tool receipts`);
      assert.equal(new Set(authored.map(r => r.recipient)).size, destinations(agent).length, `${agent.name} must address every collaborator by exact identity`);
      assert.ok(records.some(r => r.name === 'share_document'), `${agent.name} must publish its own contribution`);
      const readNames = new Set(records.filter(r => r.name === 'read_document').map(r => r.value?.document?.name));
      report.collaborators.push({name: agent.name, requestsAuthored: authored.length, nativeSendReceipts: authored.filter(r => sentIds.has(r.id)).length, nativeReplyReceipts: answers.filter(r => completedReplyIds.has(r.id)).length, peerDocumentsRead: [...readNames].filter(name => name !== 'mesh-context.md')});
    }
  }
  report.messageCounts = {actual: db.prepare('SELECT count(*) n FROM messages').get().n, expected: requests.length * 2 + passive.length + starts.length + 1 + Number(featureCheck)};
  report.messageCountPassed = report.messageCounts.actual === report.messageCounts.expected;
  report.unsolicitedReplies = db.prepare('SELECT child.id,child.sender,child.replyTo FROM messages child JOIN messages parent ON parent.id=child.replyTo WHERE parent.replyTo IS NOT NULL').all();
  assert.equal(db.prepare("SELECT count(*) n FROM deliveries d LEFT JOIN dispatches x ON x.message=d.message AND x.recipient=d.recipient WHERE x.state IS NULL OR x.state<>'submitted'").get().n, 0, 'Every handled message went through a confirmed adapter');
  const nativeTools = [];
  for (const agent of agents.filter(a => a.rpc)) for (const event of agent.rpc.events) {
    if (event.method === 'item/completed' && event.params?.item?.type === 'mcpToolCall') nativeTools.push(event.params.item.tool);
    const update = event.params?.update;
    if (update?.status === 'completed' && update.rawOutput?.type === 'MCP') nativeTools.push(update.rawOutput.tool_name);
  }
  for (const agent of agents.filter(a => a.process)) for (const event of agent.process.events) {
    if (event.type === 'tool_execution_start') nativeTools.push(event.toolName);
    for (const block of event.message?.content ?? []) if (block.type === 'tool_use') nativeTools.push(block.name);
  }
  for (const agent of agents.filter(a => a.vendor === 'opencode')) {
    agent.nativeMessages = await openCodeApi(agent, `/session/${agent.vendorSession}/message`);
    nativeTools.push(...agent.nativeMessages.flatMap(m => m.parts).filter(p => p.type === 'tool').map(p => p.tool));
  }
  assert.ok(nativeTools.length > 0, 'Observe recipient tool usage');
  assert.ok(!nativeTools.some(name => /(?:^|__)hook$|^communication_hook$/.test(name)), 'No raw hook bypass of native delivery');
  if (!completionCheck) assert.ok(!nativeTools.some(name => /(?:^|__)inbox$|^communication_inbox$/.test(name)), 'No manual inbox bypass');
  if (completionCheck) for (const agent of agents) {
    if (!['claude', 'pi'].includes(agent.vendor)) assert.ok(!nativeResults(agent).some(r => r.name === 'inbox'), 'Recovery requires the configured Stop check');
    else if (agent.vendor === 'pi') for (const event of agent.process.events.filter(e => e.type === 'tool_execution_start' && e.toolName === 'inbox')) assert.deepEqual(Object.keys(event.args), ['message'], 'Recover only a missing ID; never replay whole inbox');
    else for (const block of agent.process.events.flatMap(e => e.message?.content ?? [])) {
      if (block.type === 'tool_use' && block.name.endsWith('__inbox')) assert.deepEqual(Object.keys(block.input), ['message'], 'Recover only a missing ID; never replay whole inbox');
    }
  }
  report.recipientToolCalls = nativeTools.length;
  // The owning host can observe usage; a message dispatcher cannot infer it.
  for (const agent of agents) {
    if (agent.vendor === 'opencode') for (const message of agent.nativeMessages.filter(m => m.info.role === 'assistant')) {
      const usage = message.info.tokens;
      if (usage) call('record_usage', {key: message.info.id, scope: 'request', inputTokens: usage.input, outputTokens: usage.output, cachedInputTokens: usage.cache.read, cacheWriteTokens: usage.cache.write}, agent.id);
    }
    if (agent.vendor === 'claude') for (const [index, event] of agent.process.events.entries()) {
      if (event.type !== 'result' || !event.usage) continue;
      const usage = event.usage;
      call('record_usage', { key: `mesh-result-${index}`, scope: 'turn',
        ...(Number.isInteger(usage.input_tokens) ? { inputTokens: usage.input_tokens } : {}),
        ...(Number.isInteger(usage.output_tokens) ? { outputTokens: usage.output_tokens } : {}),
        ...(Number.isInteger(usage.cache_read_input_tokens) ? { cachedInputTokens: usage.cache_read_input_tokens } : {}),
        ...(Number.isInteger(usage.cache_creation_input_tokens) ? { cacheWriteTokens: usage.cache_creation_input_tokens } : {}),
      }, agent.id);
    }
    if (agent.vendor === 'codex') {
      const usage = agent.rpc.events.findLast(event => event.method === 'thread/tokenUsage/updated' && event.params.threadId === agent.vendorSession)?.params.tokenUsage;
      if (usage) call('record_usage', { key: 'mesh-final-observed', scope: 'cumulative',
        ...(Number.isInteger(usage.total?.inputTokens) ? { inputTokens: usage.total.inputTokens } : {}),
        ...(Number.isInteger(usage.total?.outputTokens) ? { outputTokens: usage.total.outputTokens } : {}),
        ...(Number.isInteger(usage.total?.cachedInputTokens) ? { cachedInputTokens: usage.total.cachedInputTokens } : {}),
      }, agent.id);
    }
  }
  report.observedUsage = db.prepare("SELECT s.name,s.vendor,a.data FROM audit a JOIN sessions s ON s.id=a.session WHERE a.kind='usage' ORDER BY a.id").all().map(row => ({ name: row.name, vendor: row.vendor, ...JSON.parse(row.data) }));
  report.usageScope = 'Available host observations only; Pi and OpenCode request, Claude and Grok turn, Codex final cumulative. Grok native completion counters are recorded by the dispatcher. Do not sum overlapping scopes or assume absent vendor counters are zero.';
  assert.equal(report.heartbeatError, undefined);
  assert.equal(digest(readFileSync(binary)), report.binarySha256, 'Frozen executable throughout run');
  report.protocolPassed = true;
  report.replyFormatPassed = report.replyFormatDeviations.length === 0;
  report.replyCountPassed = report.replyCountDeviations.length === 0 && report.messageCountPassed;
  report.passed = report.replyFormatPassed && report.replyCountPassed;
  if (!report.passed) { report.error = 'Delivery checks passed, but agent replies failed the single-answer/content gate; see messageCounts, unsolicitedReplies, replyCountDeviations and replyFormatDeviations.'; process.exitCode = 1; }
  report.requestEdges = requests.length; report.replyEdges = db.prepare('SELECT count(*) n FROM messages WHERE replyTo IS NOT NULL').get().n; report.broadcastRecipients = notice.recipients;
  report.messages = db.prepare('SELECT count(*) n FROM messages').get().n;
  report.identities = db.prepare('SELECT vendor,count(*) n FROM sessions GROUP BY vendor').all();
  report.dispatches = db.prepare('SELECT transport,state,count(*) n FROM dispatches GROUP BY transport,state').all();
  report.audit = db.prepare('SELECT kind,count(*) n FROM audit GROUP BY kind').all();
} catch (error) { report.error = error.stack; process.exitCode = 1; }
finally {
  cleaningUp = true;
  clearInterval(timer);
  if (db) report.pending = db.prepare('SELECT message,recipient FROM deliveries WHERE acknowledgedAt IS NULL').all();
  if (db) try {
    // Record outcomes on failures too; a timed-out completion must not erase completed exchanges.
    report.messageProgress = db.prepare(`SELECT
      (SELECT count(*) FROM messages WHERE body LIKE 'QUESTION%') AS questions,
      (SELECT count(*) FROM messages WHERE replyTo IS NOT NULL) AS replies,
      (SELECT count(*) FROM deliveries WHERE acknowledgedAt IS NOT NULL) AS acknowledged,
      (SELECT count(*) FROM deliveries WHERE acknowledgedAt IS NULL) AS pending`).get();
    report.availableUsage = db.prepare("SELECT s.name,s.vendor,a.data FROM audit a JOIN sessions s ON s.id=a.session WHERE a.kind='usage' ORDER BY a.id").all().map(row => ({name:row.name,vendor:row.vendor,...JSON.parse(row.data)}));
    report.availableUsageScope = 'Audit observations captured even on failure; host-final counters may be absent. Missing is not zero; do not sum overlapping request/turn/cumulative scopes.';
  } catch (error) { report.measurementError = error.message; report.passed = false; process.exitCode = 1; }

  if (interviewEnabled && db) {
    try { await interviewOriginalSessions(); }
    catch (error) { report.interviews = {...report.interviews, passed:false, error:error.message, separateFromWorkVerdict:true}; }
  }

  for (const agent of agents.filter(a => a.rpc)) writeFileSync(join(output, `${agent.name}-api-events.json`), JSON.stringify(agent.rpc.events));
  for (const agent of agents.filter(a => a.vendor === 'opencode')) {
    writeFileSync(join(output, `${agent.name}-api-events.json`), JSON.stringify(agent.events ?? []));
    if (agent.nativeMessages) writeFileSync(join(output, `${agent.name}-messages.json`), JSON.stringify(agent.nativeMessages));
  }
  report.opencodeSessions = agents.filter(a => a.vendor === 'opencode').map(a => ({name: a.name, vendorSession: a.vendorSession, sessionMetadata: a.sessionMetadata, readinessError: a.vendorSession ? undefined : a.readinessError, streamError: a.streamError}));
  report.grokSessions = agents.filter(a => a.vendor === 'grok').map(a => ({name: a.name, vendorSession: a.vendorSession, vendorInfo: a.vendorInfo, metadataProbe: a.metadataProbe, config: a.sessionConfig}));
  report.profiles = Object.fromEntries(agents.filter(a => a.profile).map(a => [a.name,a.profile]));
  report.toolInventories = Object.fromEntries(agents.filter(a => a.toolInventory).map(a => [a.name, a.toolInventory]));
  for (const socket of sockets) socket.close ? socket.close() : socket.destroy();
  for (const item of children.reverse()) {
    try { process.kill(-item.child.pid, 'SIGTERM'); } catch {}
    await until(() => item.child.exitCode !== null || item.child.signalCode !== null, 'child exit', 2000).catch(async () => {
      try { process.kill(-item.child.pid, 'SIGKILL'); } catch {}
      await until(() => item.child.exitCode !== null || item.child.signalCode !== null, 'child reap', 2000);
    }).catch(error => { report.cleanupError = error.message; report.passed = false; process.exitCode = 1; });
    writeFileSync(join(output, `${item.name}-events.json`), JSON.stringify(item.events));
    writeFileSync(join(output, `${item.name}-stderr.txt`), item.stderr);
    if (item.stdout) writeFileSync(join(output, `${item.name}-stdout.txt`), item.stdout);
  }
  report.childrenReaped = children.every(item => item.child.exitCode !== null || item.child.signalCode !== null);
  if (!report.childrenReaped || children.some(item => item.error)) { report.passed = false; process.exitCode = 1; }
  for (const agent of [controller, ...agents].filter(Boolean)) {
    try { call('leave', {}, agent.id); }
    catch (error) { report.cleanupError = error.message; report.passed = false; process.exitCode = 1; }
  }
  db?.close();
  if (existsSync(database)) {
    try {
      report.snapshot = JSON.parse(execFileSync(binary, ['db', 'export', JSON.stringify({path: join(output, 'audit.sqlite')}), '--database', database], {encoding: 'utf8'}));
      const documents = join(workspace, '.octocode/communication');
      if (existsSync(documents)) cpSync(documents, join(output, 'workspace-documents'), {recursive: true});
    } catch (error) { report.snapshotError = error.message; report.passed = false; process.exitCode = 1; }
  }
  report.completedAt = new Date().toISOString();
  writeFileSync(join(output, 'result.json'), JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ output, ...report }, null, 2));
}
