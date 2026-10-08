import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const hash = value => createHash('sha256').update(value).digest('hex');
const pause = ms => new Promise(done => setTimeout(done, ms));

// Optional account-backed check. It never joins an existing conversation or edits host settings.
// Run: node src/live-host-smoke.mjs --vendor codex --output /absolute/results
// Other managed hosts require --model with an available model ID; there are no guessed defaults.
export function gradeCollaboration({ messages, deliveries, request, notice, sender, receiver, expected, passiveTurns, initialMessages = [] }) {
  const replies = messages.filter(row => row.replyTo === request && row.sender === receiver && row.target === sender);
  const acknowledged = id => deliveries.some(row => row.message === id && row.recipient === receiver && row.acknowledgedAt !== null);
  const checks = {
    requestAcknowledged: acknowledged(request),
    noticeAcknowledged: acknowledged(notice),
    correlatedAnswer: replies.length === 1 && replies[0].body.trim() === expected,
    noticeHasNoReply: !messages.some(row => row.replyTo === notice),
    noReplyLoop: messages.every(row => initialMessages.includes(row.id) || [request, notice].includes(row.id) || replies.some(reply => reply.id === row.id)),
    passiveDoesNotStartTurn: passiveTurns === 0,
  };
  return { passed: Object.values(checks).every(Boolean), checks, reply: replies[0]?.id };
}

export function invoke(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: 'utf8', timeout: 15000, maxBuffer: 8 * 1024 * 1024, ...options });
  if (result.error || result.status !== 0) throw new Error(result.error?.message || `${command} exited ${result.status}: ${result.stderr}`);
  return result.stdout;
}

async function configuredCodexModel(python, scripts) {
  const source = `import json,sys,tempfile\nsys.path.insert(0,sys.argv[1])\nfrom communication.wire import Wire\nwith tempfile.TemporaryDirectory() as cwd, Wire('codex',['app-server'],cwd) as host:\n host.request('initialize',{'clientInfo':{'name':'communication-live-smoke','version':'1'}})\n host.send({'method':'initialized','params':{}})\n value=host.request('config/read',{'includeLayers':False}).get('config',{}).get('model')\n print(json.dumps({'model':value}))\n`;
  const value = JSON.parse(invoke(python, ['-B', '-c', source, scripts])).model;
  if (!value) throw new Error('Codex has no configured model; supply --model with an available model ID');
  return value;
}

export async function liveHostSmoke(options) {
  const vendor = options.vendor || 'codex';
  if (!['codex', 'claude', 'pi'].includes(vendor)) throw new Error('Managed run supports codex, claude, or pi');
  const skill = resolve(options.skill || root), scripts = join(skill, 'scripts');
  const python = process.env.OCTOCODE_PYTHON || (process.platform === 'win32' ? 'python' : 'python3');
  const model = options.model || (vendor === 'codex' ? await configuredCodexModel(python, scripts) : null);
  if (!model) throw new Error('Supply --model for this host; the runner does not choose a model');
  const duration = Number(options.duration || 120000);
  if (!Number.isSafeInteger(duration) || duration < 10000 || duration > 600000) throw new Error('--duration must be 10000–600000 milliseconds');
  const output = resolve(options.output || join(root, 'out/live-host-smoke', `${vendor}-${Date.now()}`));
  mkdirSync(output, { recursive: true });
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'communication-live-')));
  const database = join(workspace, 'communication.sqlite'), binary = join(scripts, 'communication.py');
  const events = [], result = {
    passed: false, startedAt: new Date().toISOString(), vendor, model,
    version: invoke(vendor, ['--version']).trim(), skillSha256: hash(readFileSync(join(skill, 'OPERATING.md'))),
    protocolSha256: hash(readFileSync(join(scripts, 'catalog.json'))),
    harnessVersion: 2, harnessSha256: hash(readFileSync(fileURLToPath(import.meta.url))),
    scenario: 'passive FYI, then an arithmetic request with exact reply and both acknowledgements',
    scope: 'Exploratory live-host smoke; one trial, no generalization or held-out reliability claim',
    workspace, output, durationMs: duration,
  };
  const cli = (name, input, session) => JSON.parse(invoke(python, ['-B', binary, name, JSON.stringify(input), '--workspace', workspace, '--database', database, ...(session ? ['--session', session] : [])]));
  const rows = () => {
    const db = new DatabaseSync(database, { readOnly: true });
    db.exec('PRAGMA busy_timeout=5000');
    try { return {
      messages: db.prepare('SELECT id,sender,target,body,replyTo,replyRequired,wake FROM messages ORDER BY id').all(),
      deliveries: db.prepare('SELECT message,recipient,acknowledgedAt FROM deliveries ORDER BY message,recipient').all(),
      dispatches: db.prepare('SELECT message,recipient,transport,state,submittedAt,error FROM dispatches ORDER BY message,recipient').all(),
      sessions: db.prepare('SELECT id,vendor,vendorSession,expiresAt FROM sessions ORDER BY id').all(),
    }; } finally { db.close(); }
  };
  let child, heartbeat, ended = false, workerError, sender, receiver;
  let stderr = '', stdout = '', incomplete = '';
  const deadline = Date.now() + duration;
  const wait = async (test, description) => {
    while (Date.now() < deadline) {
      if (test()) return;
      if (ended) throw new Error(`Host ended before ${description}: ${workerError || stderr}`);
      await pause(100);
    }
    throw new Error(`Timed out waiting for ${description}`);
  };
  try {
    sender = cli('join', { name: 'isolated-smoke-sender', vendor: 'generic' }).id;
    heartbeat = setInterval(() => { try { cli('heartbeat', {}, sender); } catch (error) { workerError = error.message; } }, 15000);
    child = spawn(python, ['-B', binary, 'run', '--vendor', vendor, '--model', model, '--name', 'isolated-smoke-recipient', '--prompt', 'Handle incoming peer questions and informational updates within this task. At startup, report that you are ready, then wait for delivered work.', '--duration-ms', String(duration), '--trace', '--tools', 'messaging', '--workspace', workspace, '--database', database], { stdio: ['ignore', 'pipe', 'pipe'] });
    child.stdout.on('data', chunk => {
      const text = chunk.toString(); stdout += text; incomplete += text;
      const lines = incomplete.split('\n'); incomplete = lines.pop();
      for (const line of lines) if (line) {
        try { events.push(JSON.parse(line)); } catch { workerError = `Invalid runtime JSON: ${line}`; }
      }
    });
    child.stderr.on('data', chunk => { stderr += chunk.toString(); });
    child.on('error', error => { workerError = error.message; ended = true; });
    child.on('close', code => { ended = true; result.workerExitCode = code; });
    await wait(() => events.some(event => event.type === 'ready'), 'host readiness');
    receiver = events.find(event => event.type === 'ready').session;
    result.sender = sender; result.receiver = receiver;
    await wait(() => events.some(event => event.type === 'turn-completed'), 'initial model turn');
    const initialMessages = rows().messages.map(row => row.id);
    result.initialMessages = initialMessages;
    const beforePassive = events.filter(event => event.type === 'turn-completed').length;
    const notice = cli('send_message', { to: receiver, body: 'Informational update: the sample batch contains 4 red items and 9 blue items.', reasoning: 'Share sample data for the next task', key: 'live-smoke-fyi', replyRequired: false, wake: 'passive' }, sender).id;
    await pause(2000);
    const passiveTurns = events.filter(event => event.type === 'turn-completed').length - beforePassive;
    const request = cli('send_message', { to: receiver, body: 'How many items are in the sample batch described in the informational update? Reply with only the total number.', reasoning: 'Obtain the total for the sample batch', key: 'live-smoke-request', replyRequired: true, wake: 'action' }, sender).id;
    result.request = request; result.notice = notice; result.passiveTurns = passiveTurns;
    await wait(() => {
      const state = rows();
      const expected = state.deliveries.filter(row => [notice, request].includes(row.message) && row.recipient === receiver);
      return expected.length === 2 && expected.every(row => row.acknowledgedAt !== null);
    }, 'model completion and acknowledgements');
    await wait(() => events.filter(event => event.type === 'turn-completed').length > beforePassive + passiveTurns, 'recipient turn completion');
    result.evidence = rows();
    const verdict = gradeCollaboration({ ...result.evidence, request, notice, sender, receiver, expected: '13', passiveTurns, initialMessages });
    Object.assign(result, verdict);
    if (verdict.reply) cli('complete', { message: verdict.reply }, sender);
    result.evidence = rows();
    const submitted = result.evidence.dispatches.filter(row => [notice, request].includes(row.message));
    result.hostSubmitted = submitted.length === 2 && submitted.every(row => row.submittedAt !== null);
    result.passed &&= result.hostSubmitted;
  } catch (error) {
    result.error = error.message;
    if (existsSync(database)) { try { result.evidence = rows(); } catch (snapshotError) { result.snapshotError = snapshotError.message; } }
  } finally {
    clearInterval(heartbeat);
    if (child && !ended) {
      child.kill('SIGTERM');
      for (let n = 0; n < 50 && !ended; n++) await pause(100);
      if (!ended) { child.kill('SIGKILL'); await pause(200); }
    }
    if (sender && existsSync(database)) { try { cli('leave', {}, sender); } catch (error) { result.cleanupError = error.message; } }
    result.completedAt = new Date().toISOString();
    result.eventsSha256 = hash(stdout);
    writeFileSync(join(output, 'events.jsonl'), stdout);
    writeFileSync(join(output, 'stderr.log'), stderr);
    writeFileSync(join(output, 'result.json'), JSON.stringify(result, null, 2) + '\n');
    rmSync(workspace, { recursive: true, force: true });
  }
  return result;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const options = {};
  for (let i = 2; i < process.argv.length; i += 2) {
    const key = process.argv[i].replace(/^--/, '');
    if (!['vendor', 'model', 'skill', 'output', 'duration'].includes(key) || !process.argv[i + 1]) throw new Error(`Unknown or incomplete option: ${process.argv[i]}`);
    options[key] = process.argv[i + 1];
  }
  const result = await liveHostSmoke(options);
  console.log(JSON.stringify({ passed: result.passed, vendor: result.vendor, model: result.model, output: result.output, checks: result.checks, error: result.error }));
  process.exitCode = result.passed ? 0 : 1;
}
