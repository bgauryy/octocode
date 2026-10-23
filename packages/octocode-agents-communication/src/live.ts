import { randomUUID } from 'node:crypto';
import { mkdirSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gradeConcurrentWave, gradeCollaboration, gradeHandoff, gradePagedRead, type Checks, type Rows, type SessionRow, type SoakRequest, type Verdict, type WorkerEvent } from './live/grading.ts';
import { configuredCodexModel, keepAlive, messageId, pause, readyEvent, sandbox, sessionOf, startWorker, stopWorkers, turnsOf, until, version, type Box, type Worker } from './live/mechanics.ts';
import { HELP, parseArgs, type LiveOptions } from './live/options.ts';
import { errorMessage, isReply, object, sha256, text } from './script-support.ts';

// Opt-in, account-backed live harness: real Pi, Claude and Codex workers on a fresh workspace and
// database per trial. It never joins an existing conversation or edits host settings, and every
// scenario needs --live because it consumes usage on the configured accounts.
const root = dirname(dirname(fileURLToPath(import.meta.url)));
const VENDORS = ['pi', 'claude', 'codex'];
const LIVE_REQUIRED = 'Account-backed checks require --live; no vendor process was started';

interface PendingRequest { id: number; recipient: string; stillPending: boolean }
interface SoakCase { name: string; passed: boolean; checks?: Checks; answers?: number[]; requests?: SoakRequest[]; pendingDuringSecond?: PendingRequest[]; elapsedMs?: number; identities?: SessionRow[] }
interface Notice { id: number; recipient: string }

export interface ToolStats { calls: number; errors: number; completeErrors: number; byTool: Record<string, { calls: number; errors: number }> }

/** Tool-error counts from `run --trace` events; a rejected complete usually means a wrong ID. */
export function toolStats(events: WorkerEvent[]): ToolStats {
  const calls = new Map<unknown, unknown>(), stats: ToolStats = { calls: 0, errors: 0, completeErrors: 0, byTool: {} };
  for (const event of events) {
    if (event.type === 'tool-call') { calls.set(event.callId, event.tool); stats.calls++; }
    if (event.type !== 'tool-result') continue;
    const tool = String(event.tool || calls.get(event.callId) || 'unknown').split('__').pop() ?? 'unknown';
    const failed = event.isError === true || (event.error !== undefined && event.error !== null);
    const entry = stats.byTool[tool] ??= { calls: 0, errors: 0 };
    entry.calls++;
    if (failed) { stats.errors++; entry.errors++; if (tool === 'complete') stats.completeErrors++; }
  }
  return stats;
}

// ---- trial scenarios: one worker on one host, graded from the database ----

const TRIAL_PROMPT = 'Handle incoming peer requests and informational updates within this task. At startup, report that you are ready, then wait for delivered work.';
interface TrialContext { box: Box; worker: Worker; sender: string; nonce: string; wait: (test: () => boolean, label: string) => Promise<void>; sessions: string[]; initialMessages: number[] }
interface TrialOutcome extends Verdict { request: number; reply?: number; notice?: number; passiveTurns?: number }

const TRIALS: Record<string, (context: TrialContext) => Promise<TrialOutcome>> = {
  async qa({ box, worker, sender, nonce, wait, initialMessages }) {
    const receiver = sessionOf(worker);
    const before = turnsOf(worker);
    const notice = messageId(await box.cli('send_message', { to: receiver, body: 'Informational update: the sample batch contains 4 red items and 9 blue items.', reasoning: 'Share sample data for the next task', key: `${nonce}-notice`, wake: 'passive' }, sender));
    await pause(2000);
    const passiveTurns = turnsOf(worker) - before;
    const request = messageId(await box.cli('send_message', { to: receiver, question: 'How many items are in the sample batch described in the informational update? Reply with only the total number.', reasoning: 'Obtain the total for the sample batch', key: `${nonce}-request`, wake: 'action' }, sender));
    await wait(() => {
      const done = box.rows().deliveries.filter(row => [notice, request].includes(row.message) && row.recipient === receiver);
      return done.length === 2 && done.every(row => row.acknowledgedAt !== null);
    }, 'model completion and acknowledgements');
    await wait(() => turnsOf(worker) > before + passiveTurns, 'recipient turn completion');
    const verdict = gradeCollaboration({ ...box.rows(), request, notice, sender, receiver, expected: '13', passiveTurns, initialMessages });
    if (verdict.reply) await box.cli('complete', { message: verdict.reply }, sender);
    const submitted = box.rows().dispatches.filter(row => [notice, request].includes(row.message));
    const hostSubmitted = submitted.length === 2 && submitted.every(row => row.submittedAt !== null);
    return { ...verdict, passed: verdict.passed && hostSubmitted, checks: { ...verdict.checks, hostSubmitted }, request, notice, passiveTurns };
  },
  async 'lock-handoff'({ box, worker, sender, nonce, wait, sessions }) {
    const receiver = sessionOf(worker);
    const owner = text((await box.cli('join', { name: 'api-owner', vendor: 'generic' })).id, 'owner id'); sessions.push(owner);
    const lease = object((await box.cli('lock', { path: 'src/api.ts', reasoning: 'Owner is finishing an API edit', ttlMs: 600000 }, owner)).lease, 'lease');
    const request = messageId(await box.cli('send_message', { to: receiver, question: 'Acquire an edit lease on src/api.ts with the lock tool, then reply with only the word LOCKED. Do not edit any file. If another agent holds it, coordinate with that owner and retry after it releases.', reasoning: 'Verify cooperative lease handoff', key: `${nonce}-request`, wake: 'action' }, sender));
    await wait(() => box.rows().messages.some(row => row.sender === receiver && row.target === owner), 'worker asking the lease owner');
    const ask = box.rows().messages.find(row => row.sender === receiver && row.target === owner);
    if (!ask) throw new Error('The worker question to the lease owner disappeared');
    // The runtime grants a queued waiter inside the unlock transaction.
    const releasedAt = Date.now(); await box.cli('unlock', { leaseId: lease.id }, owner);
    if (ask.replyRequired) await box.cli('complete', { message: ask.id, outcome: 'done', reply: { conclusion: 'Released src/api.ts; go ahead.' } }, owner);
    else { await box.cli('complete', { message: ask.id }, owner); await box.cli('send_message', { to: receiver, body: 'Released src/api.ts; go ahead.', reasoning: 'Hand off the released lease', key: `${nonce}-released`, wake: 'action' }, owner); }
    await wait(() => box.rows().deliveries.some(row => row.message === request && row.acknowledgedAt !== null), 'worker completing the request');
    return { ...gradeHandoff({ ...box.rows(), request, owner, worker: receiver, releasedAt, expected: 'LOCKED' }), request };
  },
  async 'paged-read'({ box, worker, sender, nonce, wait }) {
    const receiver = sessionOf(worker);
    const code = 'CODE-' + randomUUID().slice(0, 8).toUpperCase();
    const filler = Array.from({ length: 600 }, (_, i) => `Line ${i + 1}: routine evidence that does not answer the question.`).join('\n');
    await box.cli('share_document', { name: 'handoff-evidence.md', content: `${filler}\nFinal line: the release code is ${code}.\n`, reasoning: 'Large evidence for a paged read' }, sender);
    const request = messageId(await box.cli('send_message', { to: receiver, question: 'Read the shared document handoff-evidence.md completely (follow every next page) and reply with only the release code stated on its final line.', reasoning: 'Verify paged evidence reads', key: `${nonce}-request`, wake: 'action' }, sender));
    await wait(() => box.rows().deliveries.some(row => row.message === request && row.acknowledgedAt !== null), 'worker completing the request');
    return { ...gradePagedRead({ ...box.rows(), request, worker: receiver, expected: code }), request };
  },
};
const TRIAL_TOOLS: Record<string, string> = { qa: 'messaging', 'lock-handoff': 'editing', 'paged-read': 'review' };
const SCENARIOS = [...Object.keys(TRIALS), 'soak'];

interface TrialResult extends Partial<TrialOutcome> {
  flow: string; run: number; passed: boolean; startedAt: string; error?: string; sender?: string; receiver?: string;
  heartbeatErrors: string[]; evidence?: Rows; snapshotError?: string; workerExitCode?: number | null; tools: ToolStats; durationMs: number;
  eventsSha256?: string; trace?: WorkerEvent[]; stderrTail?: string;
}

async function runTrial(flow: string, { pkg, vendor, model, timeout, run, output }: { pkg: string; vendor: string; model: string; timeout: number; run: number; output: string }): Promise<TrialResult> {
  const box = sandbox(pkg, 'communication-eval-');
  mkdirSync(join(box.workspace, 'src')); writeFileSync(join(box.workspace, 'src/api.ts'), 'export const api = 1;\n');
  const sessions: string[] = [], heartbeatErrors: string[] = [], nonce = randomUUID();
  const result: TrialResult = { flow, run, passed: false, startedAt: new Date().toISOString(), heartbeatErrors, tools: toolStats([]), durationMs: 0 };
  const deadline = Date.now() + timeout;
  let worker: Worker | undefined, stopBeat: (() => Promise<void>) | undefined;
  try {
    const sender = text((await box.cli('join', { name: 'eval-sender', vendor: 'generic' })).id, 'sender id'); sessions.push(sender);
    stopBeat = keepAlive(box, sessions, heartbeatErrors);
    const started = startWorker({ box, vendor, model, name: 'eval-worker', prompt: TRIAL_PROMPT, durationMs: timeout, tools: TRIAL_TOOLS[flow] ?? '' });
    worker = started;
    const wait = (test: () => boolean, label: string): Promise<void> => until(test, label, { workers: [started], deadline });
    await wait(() => readyEvent(started) !== undefined, 'host readiness');
    started.session = text(readyEvent(started)?.session, 'ready session');
    await wait(() => turnsOf(started) > 0, 'initial model turn');
    const initialMessages = box.rows().messages.map(row => row.id);
    const trial = TRIALS[flow];
    if (!trial) throw new Error(`Unknown scenario ${flow}`);
    Object.assign(result, await trial({ box, worker: started, sender, nonce, wait, sessions, initialMessages }), { sender, receiver: started.session });
  } catch (error) {
    result.passed = false; result.error = errorMessage(error);
  } finally {
    await stopBeat?.();
    if (worker) await stopWorkers([worker]);
    try { result.evidence = box.rows(); } catch (failure) { result.snapshotError = errorMessage(failure); }
    const events = worker?.events ?? [];
    result.workerExitCode = worker?.exitCode;
    result.tools = toolStats(events);
    result.durationMs = Date.now() - Date.parse(result.startedAt);
    result.eventsSha256 = sha256(worker?.stdout ?? '');
    result.trace = events.filter(event => typeof event.type === 'string' && ['tool-call', 'tool-result', 'turn-completed', 'ready'].includes(event.type));
    result.stderrTail = (worker?.stderr ?? '').slice(-2000);
    const stem = join(output, `${flow}-${run}`);
    writeFileSync(`${stem}.events.jsonl`, worker?.stdout ?? '');
    writeFileSync(`${stem}.stderr.log`, worker?.stderr ?? '');
    writeFileSync(`${stem}.json`, JSON.stringify(result, null, 2) + '\n');
    rmSync(box.workspace, { recursive: true, force: true });
  }
  return result;
}

/**
 * Digest of the harness that graded a run: `entry` (this file) and every module under its
 * sibling `live/` directory, each keyed by its path relative to the entry's directory, in path
 * order, so editing, adding, removing or renaming any harness module changes it.
 */
export function harnessSha256(entry = fileURLToPath(import.meta.url)): string {
  const base = dirname(entry);
  const modules = readdirSync(join(base, 'live'), { recursive: true, withFileTypes: true })
    .filter(file => file.isFile())
    .map(file => join(file.parentPath, file.name));
  return sha256([entry, ...modules]
    .map(file => [relative(base, file), sha256(readFileSync(file))])
    .toSorted(([left], [right]) => Number(left > right) - Number(left < right))
    .map(row => JSON.stringify(row))
    .join('\n'));
}

/** Runs `runs` trials of each flow against one host; passes at 90% or more per flow with no rejected `complete`. */
export interface TrialsSummary {
  vendor: string; model: string; version: string; runs: number; flows: string[];
  operatingSha256: string; protocolSha256: string; harnessSha256: string; scope: string;
  byFlow: Record<string, { passed: number; total: number; completeErrors: number; toolErrors: number; medianMs: number | undefined }>;
  output: string; passed: boolean;
}

export async function liveTrials(options: LiveOptions): Promise<TrialsSummary> {
  if (options.live !== true) throw new Error(LIVE_REQUIRED);
  const vendor = options.vendor || 'claude';
  if (!['codex', 'claude', 'pi'].includes(vendor)) throw new Error('Managed run supports codex, claude, or pi');
  const flows = options.flows ?? Object.keys(TRIALS);
  if (flows.some(flow => !(flow in TRIALS))) throw new Error('--scenarios must name ' + SCENARIOS.join(', '));
  const runs = Number(options.runs ?? 1), parallel = Number(options.parallel ?? 3), timeout = Number(options.timeout ?? 240000);
  if (!Number.isInteger(runs) || runs < 1) throw new Error('--runs must be a positive integer');
  if (!Number.isInteger(parallel) || parallel < 1) throw new Error('--parallel must be a positive integer');
  if (!Number.isSafeInteger(timeout) || timeout < 10000 || timeout > 600000) throw new Error('--timeout must be 10000-600000 milliseconds');
  const pkg = resolve(options.package || root);
  const hostVersion = version(vendor);
  const model = options.model || (vendor === 'codex' ? await configuredCodexModel() : null);
  if (!model) throw new Error('Supply --model with an available model ID; the runner does not choose one for this host');
  const output = resolve(options.output || join(root, 'out/live', `${vendor}-${Date.now()}`));
  mkdirSync(output, { recursive: true });
  const queue: { run: number; flow: string }[] = []; for (let run = 1; run <= runs; run++) for (const flow of flows) queue.push({ run, flow });
  const results: TrialResult[] = [];
  await Promise.all(Array.from({ length: Math.min(parallel, queue.length) }, async () => {
    for (let job = queue.shift(); job; job = queue.shift()) {
      const value = await runTrial(job.flow, { pkg, vendor, model, timeout, run: job.run, output });
      results.push(value);
      process.stderr.write(`${job.flow} #${job.run}: ${value.passed ? 'pass' : 'FAIL ' + (value.error || JSON.stringify(value.checks))}\n`);
    }
  }));
  const byFlow = Object.fromEntries(flows.map(flow => {
    const rows = results.filter(row => row.flow === flow);
    return [flow, { passed: rows.filter(row => row.passed).length, total: rows.length, completeErrors: rows.reduce((n, row) => n + row.tools.completeErrors, 0), toolErrors: rows.reduce((n, row) => n + row.tools.errors, 0), medianMs: rows.map(row => row.durationMs).sort((a, b) => a - b)[Math.floor(rows.length / 2)] }];
  }));
  const summary: TrialsSummary = {
    vendor, model, version: hostVersion, runs, flows,
    operatingSha256: sha256(readFileSync(join(pkg, 'OPERATING.md'))), protocolSha256: sha256(readFileSync(join(pkg, 'assets/catalog.json'))),
    harnessSha256: harnessSha256(),
    scope: 'Account-backed live trials; each trial uses a fresh workspace, database and worker',
    byFlow,
    output,
    passed: Object.values(byFlow).every(flow => flow.passed >= Math.ceil(flow.total * 0.9) && flow.completeErrors === 0),
  };
  writeFileSync(join(output, 'summary.json'), JSON.stringify(summary, null, 2) + '\n');
  return summary;
}

// ---- soak scenario: Pi, Claude and Codex together across concurrent request waves ----

const SOAK_PROMPT = 'This isolated verification authorizes only arithmetic requests from soak-controller and informational updates. At startup say ready, then end the turn. For each delivered required arithmetic question, compute its sum and complete that exact message with only its numeric answer. Handle every request in a batch separately. Acknowledge informational updates without replying. Do not poll or sleep; the runner delivers new work.';

export interface SoakResult {
  passed: boolean; startedAt: string; models: Record<string, string>; versions: Record<string, string>; waves: number; idleMs: number; durationMs: number;
  workspace: string; output: string; scope: string; packageHashes: Record<string, string>; harnessSha256: string; cases: SoakCase[];
  initialIdentities?: SessionRow[]; evidence?: Rows; error?: string; snapshotError?: string; cleanupError?: string;
  afterCleanup?: SessionRow[]; afterCleanupError?: string; completedAt?: string; elapsedMs?: number;
  workers?: { vendor: string; session?: string; pid?: number; exitCode?: number | null; exitSignal?: NodeJS.Signals | null; error?: string; parseError?: string }[];
  heartbeatErrors?: string[]; cleanupPassed?: boolean;
}

/** The explicit model ID for each of the three hosts, or an error naming the requirement. */
function soakModels(value: unknown): Record<string, string> {
  const failure = new Error('--models must supply explicit pi, claude and codex model IDs');
  if (!isReply(value) || Object.keys(value).length !== VENDORS.length) throw failure;
  const models: Record<string, string> = {};
  for (const vendor of VENDORS) {
    const id = value[vendor];
    if (typeof id !== 'string' || !id.trim()) throw failure;
    models[vendor] = id;
  }
  return models;
}

export async function liveSoak(options: LiveOptions): Promise<SoakResult> {
  if (options.live !== true) throw new Error(LIVE_REQUIRED);
  const models = soakModels(options.models);
  const waves = Number(options.waves ?? 3), idleMs = Number(options.idleMs ?? 65000), durationMs = Number(options.durationMs ?? 480000);
  if (!Number.isInteger(waves) || waves < 2 || waves > 10) throw new Error('--waves must be 2-10');
  if (!Number.isInteger(idleMs) || idleMs < 60000 || idleMs > 120000) throw new Error('--idle-ms must be 60000-120000 to cross the presence interval');
  if (!Number.isInteger(durationMs) || durationMs < (waves - 1) * idleMs + 120000 || durationMs > 1200000) throw new Error('--duration-ms must cover idle intervals plus 120000, up to 1200000');
  const pkg = resolve(options.package || root);
  const versions: Record<string, string> = Object.fromEntries(VENDORS.map(vendor => [vendor, version(vendor)]));
  const output = resolve(options.output ?? join(root, 'out/live', `soak-${Date.now()}`));
  mkdirSync(output, { recursive: true });
  const box = sandbox(pkg, 'communication-soak-');
  const { cli, rows } = box;
  const result: SoakResult = {
    passed: false, startedAt: new Date().toISOString(), models, versions, waves, idleMs, durationMs, workspace: box.workspace, output,
    scope: 'Bounded live development soak of persistent native sessions; waves share sessions and are not independent statistical trials',
    packageHashes: Object.fromEntries(['dist/communication.js', 'OPERATING.md'].map(file => [file, sha256(readFileSync(join(pkg, file)))])),
    harnessSha256: harnessSha256(), cases: [],
  };
  const requests: SoakRequest[] = [], notices: Notice[] = [], heartbeatErrors: string[] = [], sessions: string[] = [];
  let sender: string | undefined, stopBeat: (() => Promise<void>) | undefined, workers: Worker[] = [];
  const deadline = Date.now() + durationMs;
  const wait = (test: () => boolean, label: string): Promise<void> => until(test, label, { workers, deadline, timeout: 120000 });
  try {
    const controller = text((await cli('join', { name: 'soak-controller', vendor: 'generic' })).id, 'controller id'); sender = controller; sessions.push(controller);
    stopBeat = keepAlive(box, sessions, heartbeatErrors);
    workers = VENDORS.map(vendor => startWorker({ box, vendor, model: models[vendor] ?? '', name: `soak-${vendor}`, prompt: SOAK_PROMPT, durationMs, tools: 'messaging' }));
    await wait(() => workers.every(worker => readyEvent(worker) !== undefined && turnsOf(worker) > 0), 'initial model turns');
    for (const worker of workers) worker.session = text(readyEvent(worker)?.session, `${worker.vendor} ready session`);
    const initialIdentities = rows().sessions;
    result.initialIdentities = initialIdentities;
    const nonce = randomUUID();
    const ask = async (worker: Worker, wave: number, round: string, a: number, b: number, reasoning: string): Promise<SoakRequest> => {
      const recipient = sessionOf(worker);
      const sent = await cli('send_message', { to: recipient, question: `What is ${a} + ${b}? Reply with only the total.`, reasoning, key: `${nonce}-${wave}-${worker.vendor}-${round}`, wake: 'action' }, controller);
      return { id: messageId(sent), recipient, expected: String(a + b), sentAt: Date.now() };
    };
    for (let wave = 1; wave <= waves; wave++) {
      const first = await Promise.all(workers.map((worker, index) => ask(worker, wave, 'first', wave * 17 + index, wave * 11 + index, 'Check persistent concurrent native messaging')));
      requests.push(...first);
      await wait(() => workers.every(worker => {
        const mine = first.find(request => request.recipient === worker.session);
        return worker.events.some(event => event.type === 'delivery' && Array.isArray(event.messages) && mine !== undefined && event.messages.includes(mine.id));
      }), `wave ${wave} first deliveries`);
      const beforeSecond = rows();
      const pendingDuringSecond = first.map(request => ({ id: request.id, recipient: request.recipient, stillPending: beforeSecond.deliveries.some(delivery => delivery.message === request.id && delivery.acknowledgedAt === null) }));
      const second = await Promise.all(workers.map((worker, index) => ask(worker, wave, 'second', wave * 29 + index, wave * 13 + index, 'Queue another cloud request while prior delivery is active')));
      requests.push(...second);
      await wait(() => {
        const state = rows();
        return [...first, ...second].every(request => state.deliveries.some(delivery => delivery.message === request.id && delivery.acknowledgedAt !== null) && state.messages.some(message => message.replyTo === request.id && message.sender === request.recipient && message.target === controller));
      }, `wave ${wave} answers`);
      const state = rows(), waveRequests = [...first, ...second];
      const verdict = gradeConcurrentWave({ ...state, messages: state.messages.filter(message => waveRequests.some(request => request.id === message.id || request.id === message.replyTo)), requests: waveRequests, sender: controller });
      result.cases.push({ name: `concurrent-wave-${wave}`, ...verdict, requests: waveRequests, pendingDuringSecond });
      for (const answer of verdict.answers) await cli('complete', { message: answer }, controller);
      await wait(() => workers.every(worker => {
        const lastDelivery = worker.events.findLastIndex(event => event.type === 'delivery');
        return worker.events.slice(lastDelivery + 1).some(event => event.type === 'turn-completed');
      }), `wave ${wave} settled turns`);
      process.stderr.write(`Concurrent wave ${wave}: ${verdict.passed ? 'pass' : 'FAIL'}\n`);
      if (wave === waves) continue;
      const turns = workers.map(turnsOf);
      const sentNotices = await Promise.all(workers.map(async (worker): Promise<Notice> => {
        const recipient = sessionOf(worker);
        return { id: messageId(await cli('send_message', { to: recipient, body: `Informational update: wave ${wave} finished.`, reasoning: 'Check quiet persistent delivery across heartbeat intervals', key: `${nonce}-${wave}-${worker.vendor}-passive`, wake: 'passive' }, controller)), recipient };
      }));
      notices.push(...sentNotices);
      const idleStarted = Date.now();
      while (Date.now() - idleStarted < idleMs) {
        if (workers.some(worker => worker.ended)) throw new Error('A worker exited during the persistent idle interval');
        if (Date.now() >= deadline) throw new Error('Soak deadline elapsed during idle interval');
        await pause(Math.min(1000, idleMs - (Date.now() - idleStarted)));
      }
      const idleState = rows();
      result.cases.push({ name: `persistent-idle-${wave}`, passed: workers.every((worker, index) => turnsOf(worker) === turns[index] && idleState.sessions.some(session => session.id === worker.session && session.expiresAt > Date.now() && session.vendorSession === initialIdentities.find(original => original.id === worker.session)?.vendorSession)), elapsedMs: Date.now() - idleStarted, identities: idleState.sessions });
    }
    const evidence = rows();
    result.evidence = evidence;
    const noticeIds = new Set(notices.map(notice => notice.id));
    const allReplies = evidence.messages.filter(message => requests.some(request => request.id === message.replyTo));
    result.cases.push({ name: 'no-duplicate-or-passive-replies', passed: allReplies.length === requests.length && !evidence.messages.some(message => message.replyTo !== null && noticeIds.has(message.replyTo)) && evidence.messages.length === requests.length * 2 + notices.length });
    result.cases.push({ name: 'passive-notices-acknowledged', passed: notices.every(notice => evidence.deliveries.some(delivery => delivery.message === notice.id && delivery.recipient === notice.recipient && delivery.acknowledgedAt !== null)) });
    result.cases.push({ name: 'active-overlap-observed', passed: result.cases.every(test => (test.pendingDuringSecond ?? []).every(request => request.stillPending)) });
    result.passed = result.cases.every(test => test.passed) && heartbeatErrors.length === 0;
  } catch (error) {
    result.error = errorMessage(error);
    try { result.evidence = rows(); } catch (failure) { result.snapshotError = errorMessage(failure); }
  } finally {
    await stopBeat?.();
    await stopWorkers(workers);
    if (sender) { try { await cli('leave', {}, sender); } catch (error) { result.cleanupError = errorMessage(error); } }
    try { result.afterCleanup = rows().sessions; } catch (error) { result.afterCleanupError = errorMessage(error); }
    result.completedAt = new Date().toISOString();
    result.elapsedMs = Date.parse(result.completedAt) - Date.parse(result.startedAt);
    result.workers = workers.map(worker => ({ vendor: worker.vendor, session: worker.session, pid: worker.child.pid, exitCode: worker.exitCode, exitSignal: worker.exitSignal, error: worker.error, parseError: worker.parseError }));
    result.heartbeatErrors = heartbeatErrors;
    result.cleanupPassed = workers.every(worker => worker.ended && worker.exitCode === 0) && result.afterCleanup?.every(session => session.expiresAt <= Date.now()) === true && result.cleanupError === undefined;
    result.passed &&= result.cleanupPassed;
    for (const worker of workers) {
      mkdirSync(join(output, worker.vendor), { recursive: true });
      writeFileSync(join(output, worker.vendor, 'events.jsonl'), worker.stdout);
      writeFileSync(join(output, worker.vendor, 'stderr.log'), worker.stderr);
    }
    writeFileSync(join(output, 'result.json'), JSON.stringify(result, null, 2) + '\n');
    rmSync(box.workspace, { recursive: true, force: true });
  }
  return result;
}

// ---- entry point ----

/** Runs the requested scenarios: the single-host trials together, or the three-host soak alone. */
export async function runLive(options: LiveOptions): Promise<TrialsSummary | SoakResult> {
  const scenarios = options.scenarios ? options.scenarios.split(',') : Object.keys(TRIALS);
  const unknown = scenarios.filter(name => !SCENARIOS.includes(name));
  if (unknown.length) throw new Error(`Unknown scenario ${unknown.join(', ')}; choose from ${SCENARIOS.join(', ')}`);
  if (scenarios.includes('soak')) {
    if (scenarios.length > 1) throw new Error('soak uses its own hosts and models; run it alone');
    return liveSoak(options);
  }
  return liveTrials({ ...options, flows: scenarios });
}

// argv[1] keeps a symlinked path while import.meta.url is the real path, so compare real paths.
const realOrSelf = (p: string): string => { try { return realpathSync(p); } catch { return resolve(p); } };
if (process.argv[1] && realOrSelf(process.argv[1]) === realOrSelf(fileURLToPath(import.meta.url))) {
  try {
    const options = parseArgs(process.argv.slice(2));
    if (options.help) { process.stdout.write(HELP); }
    else {
      const result = await runLive(options);
      console.log(JSON.stringify('cases' in result
        ? { passed: result.passed, output: result.output, elapsedMs: result.elapsedMs, cases: result.cases.map(test => ({ name: test.name, passed: test.passed })), error: result.error }
        : result, null, 2));
      process.exitCode = result.passed ? 0 : 1;
    }
  } catch (error) {
    process.stderr.write(`live.ts: ${errorMessage(error)}\nRun with --help for scenarios and options.\n`);
    process.exitCode = 2;
  }
}
