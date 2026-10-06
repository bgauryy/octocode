import { spawn } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { gradeCollaboration, invoke } from './live-host-smoke.mjs';

// Account-backed reliability eval: N independent runs of three collaboration flows
// against a real managed host worker. Each run gets a fresh workspace, database and
// worker; nothing joins an existing conversation or edits host settings.
// Run: node src/live-reliability.mjs --vendor claude --model MODEL --runs 10 --output DIR
const root = dirname(dirname(fileURLToPath(import.meta.url)));
const pause = ms => new Promise(done => setTimeout(done, ms));
const hash = value => createHash('sha256').update(value).digest('hex');
export const FLOWS = ['qa', 'lock-handoff', 'paged-read'];

/** Tool-error counts from `run --trace` events; a rejected complete usually means a wrong ID. */
export function toolStats(events) {
  const calls = new Map(), stats = { calls: 0, errors: 0, completeErrors: 0, byTool: {} };
  for (const event of events) {
    if (event.type === 'tool-call') { calls.set(event.callId, event.tool); stats.calls++; }
    if (event.type !== 'tool-result') continue;
    const tool = String(event.tool || calls.get(event.callId) || 'unknown').split('__').pop();
    const failed = event.isError === true || (event.error !== undefined && event.error !== null);
    stats.byTool[tool] = stats.byTool[tool] || { calls: 0, errors: 0 };
    stats.byTool[tool].calls++;
    if (failed) { stats.errors++; stats.byTool[tool].errors++; if (tool === 'complete') stats.completeErrors++; }
  }
  return stats;
}

/** Lock handoff passes when the worker asked the owner, never overlapped the owner's lease,
 * acquired the path after release, and answered the request exactly. */
export function gradeHandoff({ messages, deliveries, leases, request, owner, worker, releasedAt, expected }) {
  const asked = messages.filter(row => row.sender === worker && row.target === owner);
  const acquired = leases.filter(row => row.owner === worker && row.path.endsWith('src/api.ts'));
  const replies = messages.filter(row => row.replyTo === request && row.sender === worker);
  const checks = {
    askedOwner: asked.length >= 1,
    noOverlap: acquired.every(row => releasedAt !== null && row.acquiredAt >= releasedAt),
    acquiredAfterRelease: acquired.length >= 1,
    correlatedAnswer: replies.length === 1 && replies[0].body.trim() === expected,
    requestAcknowledged: deliveries.some(row => row.message === request && row.recipient === worker && row.acknowledgedAt !== null),
  };
  return { passed: Object.values(checks).every(Boolean), checks };
}

/** Paged read passes when the worker answers with the code that only the last page holds. */
export function gradePagedRead({ messages, deliveries, request, worker, expected }) {
  const replies = messages.filter(row => row.replyTo === request && row.sender === worker);
  const checks = {
    correlatedAnswer: replies.length === 1 && replies[0].body.trim() === expected,
    requestAcknowledged: deliveries.some(row => row.message === request && row.recipient === worker && row.acknowledgedAt !== null),
  };
  return { passed: Object.values(checks).every(Boolean), checks };
}

async function runFlow(flow, { vendor, model, python, binary, timeout }) {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'communication-eval-')));
  const database = join(workspace, 'communication.sqlite');
  mkdirSync(join(workspace, 'src')); writeFileSync(join(workspace, 'src/api.ts'), 'export const api = 1;\n');
  const cli = (name, input, session) => JSON.parse(invoke(python, ['-B', binary, name, JSON.stringify(input), '--workspace', workspace, '--database', database, ...(session ? ['--session', session] : [])]));
  const rows = () => {
    // Rollback-journal stores block readers during a commit; wait like the runtime does.
    const db = new DatabaseSync(database, { readOnly: true, timeout: 5000 });
    try { return {
      messages: db.prepare('SELECT id,sender,target,body,replyTo,replyRequired,wake FROM messages ORDER BY id').all(),
      deliveries: db.prepare('SELECT message,recipient,acknowledgedAt FROM deliveries ORDER BY message,recipient').all(),
      leases: db.prepare("SELECT json_extract(data,'$.leaseId') AS id,json_extract(data,'$.path') AS path,\"from\" AS owner,timestamp AS acquiredAt FROM records WHERE type='lease.acquired' ORDER BY id").all(),
    }; } finally { db.close(); }
  };
  const result = { flow, startedAt: new Date().toISOString() }, events = [];
  let child, ended = false, stdout = '', stderr = '', incomplete = '', heartbeat;
  const deadline = Date.now() + timeout;
  const wait = async (test, description) => {
    while (Date.now() < deadline) { if (test()) return; if (ended) throw new Error(`Host ended before ${description}`); await pause(200); }
    throw new Error(`Timed out waiting for ${description}`);
  };
  try {
    const sender = cli('join', { name: 'eval-sender', vendor: 'generic' }).id;
    const keepAlive = [sender];
    heartbeat = setInterval(() => { for (const id of keepAlive) { try { cli('heartbeat', { renewLeases: true }, id); } catch {} } }, 10000);
    const tools = flow === 'lock-handoff' ? 'editing' : flow === 'paged-read' ? 'review' : 'messaging';
    child = spawn(python, ['-B', binary, 'run', '--vendor', vendor, '--model', model, '--name', 'eval-worker',
      '--prompt', 'Handle incoming peer requests within this task. At startup, report that you are ready, then wait for delivered work.',
      '--duration-ms', String(timeout), '--trace', '--tools', tools, '--workspace', workspace, '--database', database], { stdio: ['ignore', 'pipe', 'pipe'] });
    child.stdout.on('data', chunk => {
      const text = chunk.toString(); stdout += text; incomplete += text;
      const lines = incomplete.split('\n'); incomplete = lines.pop();
      for (const line of lines) if (line) { try { events.push(JSON.parse(line)); } catch {} }
    });
    child.stderr.on('data', chunk => { stderr += chunk.toString(); });
    child.on('close', code => { ended = true; result.workerExitCode = code; });
    await wait(() => events.some(event => event.type === 'ready'), 'host readiness');
    const worker = events.find(event => event.type === 'ready').session;
    await wait(() => events.some(event => event.type === 'turn-completed'), 'initial model turn');
    const initialMessages = rows().messages.map(row => row.id);
    if (flow === 'qa') {
      const before = events.filter(event => event.type === 'turn-completed').length;
      const notice = cli('send_message', { to: 'eval-worker', body: 'Informational update: the sample batch contains 4 red items and 9 blue items.', reasoning: 'Share sample data for the next task', replyRequired: false, wake: 'passive' }, sender).id;
      await pause(2000);
      const passiveTurns = events.filter(event => event.type === 'turn-completed').length - before;
      const request = cli('send_message', { to: 'eval-worker', body: 'How many items are in the sample batch described in the informational update? Reply with only the total number.', reasoning: 'Obtain the total for the sample batch', replyRequired: true, wake: 'action' }, sender).id;
      await wait(() => rows().deliveries.filter(row => [notice, request].includes(row.message) && row.acknowledgedAt !== null).length === 2, 'completion and acknowledgements');
      Object.assign(result, gradeCollaboration({ ...rows(), request, notice, sender, receiver: worker, expected: '13', passiveTurns, initialMessages }));
    } else if (flow === 'lock-handoff') {
      const owner = cli('join', { name: 'api-owner', vendor: 'generic' }).id; keepAlive.push(owner);
      const lease = cli('lock', { path: 'src/api.ts', reasoning: 'Owner is finishing an API edit', ttlMs: 600000 }, owner).lease;
      const request = cli('send_message', { to: 'eval-worker', body: 'Acquire an edit lease on src/api.ts with the lock tool, then reply with only the word LOCKED. Do not edit any file. If another agent holds it, coordinate with that owner and retry after it releases.', reasoning: 'Verify cooperative lease handoff', replyRequired: true, wake: 'action' }, sender).id;
      let releasedAt = null;
      await wait(() => rows().messages.some(row => row.sender === worker && row.target === owner), 'worker asking the lease owner');
      const ask = rows().messages.find(row => row.sender === worker && row.target === owner);
      // The runtime grants a queued waiter inside the unlock transaction.
      releasedAt = Date.now(); cli('unlock', { leaseId: lease.id }, owner);
      if (ask.replyRequired) cli('complete', { message: ask.id, reply: 'Released src/api.ts; go ahead.' }, owner);
      else { cli('complete', { message: ask.id }, owner); cli('send_message', { to: worker, body: 'Released src/api.ts; go ahead.', reasoning: 'Hand off the released lease', replyRequired: false, wake: 'action' }, owner); }
      await wait(() => rows().deliveries.some(row => row.message === request && row.acknowledgedAt !== null), 'worker completing the request');
      Object.assign(result, gradeHandoff({ ...rows(), request, owner, worker, releasedAt, expected: 'LOCKED' }));
    } else {
      const code = 'CODE-' + randomUUID().slice(0, 8).toUpperCase();
      const filler = Array.from({ length: 600 }, (_, i) => `Line ${i + 1}: routine evidence that does not answer the question.`).join('\n');
      cli('share_document', { name: 'handoff-evidence.md', content: `${filler}\nFinal line: the release code is ${code}.\n`, reasoning: 'Large evidence for a paged read' }, sender);
      const request = cli('send_message', { to: 'eval-worker', body: 'Read the shared document handoff-evidence.md completely (follow every next page) and reply with only the release code stated on its final line.', reasoning: 'Verify paged evidence reads', replyRequired: true, wake: 'action' }, sender).id;
      await wait(() => rows().deliveries.some(row => row.message === request && row.acknowledgedAt !== null), 'worker completing the request');
      Object.assign(result, gradePagedRead({ ...rows(), request, worker, expected: code }));
    }
  } catch (error) {
    result.passed = false; result.error = error.message;
  } finally {
    clearInterval(heartbeat);
    if (child && !ended) { child.kill('SIGTERM'); for (let n = 0; n < 50 && !ended; n++) await pause(100); if (!ended) child.kill('SIGKILL'); }
    result.tools = toolStats(events);
    result.durationMs = Date.now() - Date.parse(result.startedAt);
    result.eventsSha256 = hash(stdout);
    result.trace = events.filter(event => ['tool-call', 'tool-result', 'turn-completed', 'ready'].includes(event.type));
    result.stderrTail = stderr.slice(-2000);
    rmSync(workspace, { recursive: true, force: true });
  }
  return result;
}

export async function liveReliability(options) {
  const vendor = options.vendor || 'claude';
  if (!['codex', 'claude', 'pi'].includes(vendor)) throw new Error('Managed run supports codex, claude, or pi');
  if (!options.model) throw new Error('Supply --model with an available model ID; the runner does not choose one');
  const runs = Number(options.runs || 10), parallel = Number(options.parallel || 3), timeout = Number(options.timeout || 240000);
  const flows = options.flows ? options.flows.split(',') : FLOWS;
  if (flows.some(flow => !FLOWS.includes(flow))) throw new Error('--flows must name ' + FLOWS.join(', '));
  const python = process.env.OCTOCODE_PYTHON || (process.platform === 'win32' ? 'python' : 'python3');
  const binary = join(resolve(options.skill || root), 'scripts', 'communication.py');
  const output = resolve(options.output || join(root, 'out/live-reliability', `${vendor}-${Date.now()}`));
  mkdirSync(output, { recursive: true });
  const queue = []; for (let run = 1; run <= runs; run++) for (const flow of flows) queue.push({ run, flow });
  const results = [];
  await Promise.all(Array.from({ length: Math.min(parallel, queue.length) }, async () => {
    for (let job = queue.shift(); job; job = queue.shift()) {
      const value = { run: job.run, ...(await runFlow(job.flow, { vendor, model: options.model, python, binary, timeout })) };
      results.push(value);
      writeFileSync(join(output, `${job.flow}-${job.run}.json`), JSON.stringify(value, null, 2) + '\n');
      process.stderr.write(`${job.flow} #${job.run}: ${value.passed ? 'pass' : 'FAIL ' + (value.error || JSON.stringify(value.checks))}\n`);
    }
  }));
  const summary = {
    vendor, model: options.model, version: invoke(vendor, ['--version']).trim(), runs, flows,
    skillSha256: hash(readFileSync(join(root, 'OPERATING.md'))), harnessSha256: hash(readFileSync(fileURLToPath(import.meta.url))),
    byFlow: Object.fromEntries(flows.map(flow => {
      const rows = results.filter(row => row.flow === flow);
      return [flow, { passed: rows.filter(row => row.passed).length, total: rows.length, completeErrors: rows.reduce((n, row) => n + row.tools.completeErrors, 0), toolErrors: rows.reduce((n, row) => n + row.tools.errors, 0), medianMs: rows.map(row => row.durationMs).sort((a, b) => a - b)[Math.floor(rows.length / 2)] }];
    })),
    output,
  };
  summary.passed = Object.values(summary.byFlow).every(flow => flow.passed >= Math.ceil(flow.total * 0.9)) && Object.values(summary.byFlow).every(flow => flow.completeErrors === 0);
  writeFileSync(join(output, 'summary.json'), JSON.stringify(summary, null, 2) + '\n');
  return summary;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const options = {};
  for (let i = 2; i < process.argv.length; i += 2) {
    const key = process.argv[i].replace(/^--/, '');
    if (!['vendor', 'model', 'skill', 'output', 'runs', 'parallel', 'timeout', 'flows'].includes(key) || !process.argv[i + 1]) throw new Error(`Unknown or incomplete option: ${process.argv[i]}`);
    options[key] = process.argv[i + 1];
  }
  const summary = await liveReliability(options);
  console.log(JSON.stringify(summary, null, 2));
  process.exitCode = summary.passed ? 0 : 1;
}
