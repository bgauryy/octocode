import { spawn, execFileSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { mkdtempSync, writeFileSync, mkdirSync, symlinkSync, cpSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { randomUUID, createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import assert from 'node:assert/strict';


const probeInput=(command,input)=>['send_message','notify_all','lock','lock_many'].includes(command)?{...( ['send_message','notify_all'].includes(command)?{wake:'action'}:{}),reasoning:`Validate ${command} interoperability in this isolated communication exercise`,...input}:input;

const withPi = process.argv.includes('--pi');
const piModel = process.env.COMMUNICATION_PI_MODEL;
if (withPi && !piModel) throw new Error('Set COMMUNICATION_PI_MODEL to an exact provider/model from pi --list-models');
const directory = mkdtempSync(join(tmpdir(), 'octocode-communication-poc-'));
const database = join(directory, 'communication.sqlite');
mkdirSync(join(directory, 'real/sub'), { recursive: true });
if (process.platform !== 'win32') symlinkSync('real/sub', join(directory, 'alias'));
const claudePath = process.platform === 'win32' ? 'real/shared.txt' : 'alias/../shared.txt';
const skill = join(directory, 'installed-skill');
cpSync(fileURLToPath(new URL('../skills/octocode-agents-communication', import.meta.url)), skill, { recursive: true });
const cli = join(skill, 'scripts/agents-communication');
const instructions = readFileSync(join(skill, 'SKILL.md'), 'utf8');
assert.equal(JSON.parse(execFileSync(cli, ['skill'], { encoding: 'utf8' })).instructions, instructions);
const skillEvidence = { path: skill, words: instructions.trim().split(/\s+/).length, sha256: createHash('sha256').update(instructions).digest('hex') };
const invoke = (command, input = {}, session) => JSON.parse(execFileSync(cli, [command, JSON.stringify(probeInput(command,input)), '--workspace', directory, '--database', database, ...(session ? ['--session', session] : [])], { encoding: 'utf8' }));
const store = {
  join: input => invoke('join', input),
  heartbeat: session => invoke('heartbeat', {}, session),
  send: (session, input) => invoke('send_message', input, session),
  inbox: session => invoke('inbox', {}, session),
  leave: session => invoke('leave', {}, session),
};
const controller = store.join({ name: 'poc-controller', vendor: 'test' });
const nonce = randomUUID();
const workers = [];
const events = [];
let report;
const heartbeat = setInterval(() => store.heartbeat(controller.id), 10_000);
const deadline = Date.now() + 150_000;

async function until(predicate, label) {
  while (!predicate()) {
    if (Date.now() > deadline) throw new Error(`POC timed out: ${label}`);
    const failed = workers.find(w => w.exited);
    if (failed) throw new Error(`${failed.vendor} exited: ${failed.code}\n${failed.stderr.slice(-2000)}`);
    await new Promise(resolve => setTimeout(resolve, 250));
  }
}
function start(vendor, model, prompt) {
  const child = spawn(cli, ['run', '--vendor', vendor, '--model', model,
    '--name', `poc-${vendor}`, '--workspace', directory, '--database', database,
    '--duration-ms', '150000', '--trace', '--prompt', prompt], { stdio: ['ignore', 'pipe', 'pipe'] });
  const worker = { vendor, child, stderr: '', ready: false, idle: false, exited: false };
  workers.push(worker);
  child.stderr.on('data', data => { worker.stderr += data; });
  child.on('error', error => { worker.exited = true; worker.stderr += error.message; });
  child.on('exit', code => { worker.exited = true; worker.code = code; });
  createInterface({ input: child.stdout }).on('line', line => {
    const event = JSON.parse(line);
    events.push({ vendor, ...event });
    if (event.type === 'ready') { worker.ready = true; worker.session = event.session; worker.vendorPid = event.pid; }
    if (event.type === 'turn-completed') worker.idle = true;
    if (!['protocol', 'tool-result', 'tool-call'].includes(event.type)) process.stdout.write(`${JSON.stringify({ vendor, ...event })}\n`);
  });
  return worker;
}

try {
  const common = `This is a communication-only POC. Use only the communication tools (peers, send_message, notify_all, inbox, ack, subscribe, lock, renew, unlock). Do not use shell, edit files, or spawn agents. Initially call peers once, respond READY in your final answer (do not send a READY peer message), and finish your turn. Later inbox messages initiate the following authorized test. Preserve nonce strings exactly. `;
  const claude = start('claude', 'haiku', common + `On PING <nonce>, try lock with path ${claudePath}. It MUST conflict. Send PONG <nonce> CONFLICT to the sender only if the lock was denied; otherwise send FAIL. Acknowledge the incoming message. Finish the turn.`);
  const pi = withPi ? start('pi', piModel, common + `On PROBE <nonce>, try lock REAL/SHARED.TXT. It MUST conflict. Send CHECKED <nonce> CONFLICT to the sender only if denied; otherwise send FAIL. Acknowledge PROBE and finish the turn.`) : null;
  const completion = `release your lease, send DONE <nonce> to controller ${controller.id}, acknowledge the incoming message and finish the turn.`;
  const afterPong = withPi ? `find poc-pi via peers, send PROBE <nonce> to it, acknowledge PONG and finish the turn. On CHECKED <nonce> CONFLICT, ${completion}` : completion;
  const codex = start('codex', 'gpt-6-luna', common + `On START <nonce>, acquire lock real/shared.txt with ttlMs 120000, remember the lease ID, find poc-claude via peers and send PING <nonce> to it. Acknowledge START. Finish the turn. On PONG <nonce> CONFLICT, ${afterPong}`);
  await until(() => workers.every(w => w.ready && w.idle), 'workers ready');
  store.send(controller.id, { to: codex.session, body: `START ${nonce}`, key: nonce });
  await until(() => store.inbox(controller.id).items.some(m => m.body === `DONE ${nonce}`), 'cross-vendor round trip');
  const db = new DatabaseSync(database);
  try {
    await until(() => Number(db.prepare('SELECT count(*) AS n FROM deliveries WHERE recipient != ? AND acknowledgedAt IS NULL').get(controller.id).n) === 0, 'worker acknowledgements');
    assert.equal(db.prepare('SELECT count(*) AS n FROM leases').get().n, 0);
    const messages = db.prepare('SELECT id,sender,target,body FROM messages ORDER BY id').all();
    const exchange = messages.filter(m => m.body.includes(nonce));
    assert.deepEqual(exchange.map(m => m.body), [`START ${nonce}`, `PING ${nonce}`, `PONG ${nonce} CONFLICT`, ...(withPi ? [`PROBE ${nonce}`, `CHECKED ${nonce} CONFLICT`] : []), `DONE ${nonce}`]);
    assert.equal(exchange[1].sender, codex.session);
    assert.equal(exchange[1].target, claude.session);
    assert.equal(exchange[2].sender, claude.session);
    assert.equal(exchange[2].target, codex.session);
    function containsConflict(value) {
      if (typeof value === 'string') { try { return containsConflict(JSON.parse(value)); } catch { return false; } }
      if (!value || typeof value !== 'object') return false;
      if (value.ok === false && value.conflict?.owner === codex.session && value.conflict?.id > 0) return true;
      return Object.values(value).some(containsConflict);
    }
    for (const vendor of ['claude', ...(withPi ? ['pi'] : [])]) {
      const expectedPath = vendor === 'pi' ? 'REAL/SHARED.TXT' : claudePath;
      assert.ok(events.some(e => e.vendor === vendor && e.type === 'tool-call' && e.tool?.endsWith('lock') && e.input?.path === expectedPath), `${vendor}: actual alias lock input missing`);
      assert.ok(events.some(e => e.vendor === vendor && e.type === 'tool-result' && e.tool?.endsWith('lock') && containsConflict(e.result)), `${vendor}: actual lock conflict tool result missing`);
    }
    if (pi) {
      assert.equal(exchange[3].sender, codex.session); assert.equal(exchange[3].target, pi.session);
      assert.equal(exchange[4].sender, pi.session); assert.equal(exchange[4].target, codex.session);
    }
    report = { passed: true, skill: skillEvidence, models: { codex: 'gpt-6-luna', claude: 'haiku', ...(withPi ? { pi: piModel } : {}) }, nonce, messages,
      checks: ['copied standalone skill launcher and embedded instructions match', `${workers.length} live vendor processes`, 'Codex to Claude delivery', 'Claude to Codex reply', withPi ? 'actual lock tool results confirm symlink-parent and case alias conflicts' : 'actual lock tool result confirms symlink-parent alias conflict', 'lease release', 'explicit acknowledgements'], events };

  } finally { db.close(); }
} catch (error) {
  writeFileSync(join(directory, 'failure.json'), JSON.stringify({ error: error.message, events, stderr: workers.map(w => ({ vendor: w.vendor, stderr: w.stderr })) }, null, 2), { mode: 0o600 });
  console.error(`${error.message}\nEvidence: ${directory}`);
  process.exitCode = 1;
} finally {
  clearInterval(heartbeat);
  await Promise.all(workers.map(w => new Promise(resolve => {
    if (w.exited) return resolve();
    const timer = setTimeout(() => w.child.kill('SIGKILL'), 5000);
    w.child.once('exit', () => { clearTimeout(timer); resolve(); });
    w.child.kill('SIGTERM');
  })));
  store.leave(controller.id);
  if (report) {
    for (const worker of workers) {
      assert.throws(() => process.kill(worker.vendorPid, 0), error => error.code === 'ESRCH', `${worker.vendor} process still running`);
    }
    const db = new DatabaseSync(database);
    try { assert.equal(db.prepare('SELECT count(*) AS n FROM sessions WHERE expiresAt > ?').get(Date.now()).n, 0); }
    finally { db.close(); }
    report.checks.push('owned vendor processes terminated', 'all test sessions expired');
    writeFileSync(join(directory, 'result.json'), JSON.stringify(report, null, 2), { mode: 0o600 });
    console.log(JSON.stringify({ passed: true, result: join(directory, 'result.json') }));
  }
}
