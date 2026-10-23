/** Explicitly launched, isolated vendor workers using the bound communication tools: launch, supervision and the shared loop. */
import { spawn, type ChildProcess } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { mkdtempSync, rmSync } from 'node:fs';
import { Socket } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import * as catalog from './catalog.js';
import { databasePath } from './config.js';
import type { RecordUsageInput } from './catalog-types.generated.js';
import type { DeliveryStore } from './delivery-types.js';
import { SESSION_EXPIRED, withPeers } from './dispatch.js';
import { ValidationError } from './errors.js';
import { encode } from './json-text.js';
import { Presence, stopEvent } from './loop-control.js';
import { claimDeliveryOwner } from './owner-lock.js';
import { truncateCodePoints } from './text.js';
import { deadlineAfter, monotonic, now, running, type Stop } from './timing.js';
import type { UsageCounter } from './transport.js';
import { isObject, objectOrEmpty, type JsonObject } from './validation.js';
import { WORKERS, type LoopWorker, type ProxyArgs, type ProxyRuntime } from './vendor-workers.js';
import { TERMINATE_GRACE_MS, Wire, refusal } from './wire.js';

const USAGE_FIELDS: readonly (readonly [UsageCounter, readonly string[]])[] = [
  ['inputTokens', ['inputTokens', 'input_tokens', 'input']],
  ['outputTokens', ['outputTokens', 'output_tokens', 'output']],
  ['cachedInputTokens', ['cachedInputTokens', 'cache_read_input_tokens', 'cacheRead']],
  ['cacheWriteTokens', ['cacheWriteTokens', 'cache_creation_input_tokens', 'cacheWrite']],
];
const PI_CONTEXT_FIELDS = ['input', 'cacheRead', 'cacheWrite'];
const POLL_MS = 100;

/** The first line, or '' for no lines. */
function firstLine(text: string): string {
  return text.split(/\r\n|[\n\r\v\f\x1c-\x1e\x85\u2028\u2029]/)[0] ?? '';
}

export async function run(args: ProxyArgs, runtime: ProxyRuntime): Promise<void> {
  if (!args.vendor) throw new ValidationError('--vendor required');
  const input = { vendor: args.vendor, model: args.model, prompt: args.prompt, ...(args.durationMs === undefined ? {} : { durationMs: args.durationMs }), ...(args.name ? { name: args.name } : {}) };
  catalog.command('run', input);
  const model = catalog.text(input, 'model');
  const prompt = catalog.text(input, 'prompt');
  const store = runtime.open(databasePath(args.database), args.workspace, { create: true });
  try {
    const session = args.session ? await store.call(args.session, 'resume', { vendor: args.vendor }) : await store.call('', 'join', { vendor: args.vendor, name: args.name || args.vendor });
    const identity: string = session.id;
    await store.call(identity, 'heartbeat', { renewLeases: true, task: truncateCodePoints(firstLine(prompt), 256), status: 'busy' });
    try {
      await worker(args, store, identity, args.vendor, model, prompt, runtime);
    } finally {
      await store.call(identity, 'leave', {});
    }
  } finally {
    store.db.close();
  }
}

async function worker(args: ProxyArgs, store: DeliveryStore, identity: string, vendor: string, model: string, prompt: string, runtime: ProxyRuntime): Promise<void> {
  const Handler = WORKERS[vendor];
  if (!Handler) throw new ValidationError('Unknown worker vendor: ' + vendor);
  const lock = await claimDeliveryOwner(store.database, identity);
  const directory = mkdtempSync(join(tmpdir(), 'octocode-worker-'));
  const stop = stopEvent();
  try {
    store.attach(identity, { transport: 'raw' });
    const deadline = deadlineAfter(args.durationMs);
    const parent = process.ppid;
    const handler = new Handler({ args, store, identity, model, prompt, directory, runtime });
    const loop = new Loop(handler, stop.signal, deadline, parent);
    let guard: ChildProcess | null = null;
    const host = new Wire(vendor, handler.argv(), handler.cwd, Object.entries(handler.environment), deadline, stop.signal);
    try {
      handler.host = host;
      guard = (runtime.watchdog ?? watchdog)(host.pid);
      await loop.launch();
      await loop.run();
    } finally {
      await host.close();
      guard?.kill();
    }
  } finally {
    stop.dispose();
    try {
      rmSync(directory, { recursive: true, force: true });
    } finally {
      await lock.release();
    }
  }
}

/**
 * Whether the process this worker must outlive is gone. POSIX re-parents an orphan, so `ppid` changes; Windows does not,
 * so the original parent's liveness is probed instead (signal 0 only tests existence; EPERM still means it exists).
 */
export function parentGone(parent: number): boolean {
  if (process.platform !== 'win32') return process.ppid !== parent;
  try {
    process.kill(parent, 0);
    return false;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code !== 'EPERM';
  }
}

/**
 * Runs in a detached `node -e` process. Its stdin is a pipe whose only write end is the worker, so EOF means the worker died
 * (SIGKILL included, and immune to pid reuse or an unreaped zombie). Then the vendor group gets SIGTERM, SIGKILL after the
 * grace. It also exits as soon as the group is gone, so a clean worker shutdown leaves nothing behind.
 */
const WATCHDOG = `const [group, grace] = process.argv.slice(1).map(Number);
const alive = () => { try { process.kill(-group, 0); return true; } catch (error) { return error.code === 'EPERM'; } };
const signal = name => { try { process.kill(-group, name); } catch {} };
setInterval(() => { if (!alive()) process.exit(0); }, 100);
process.stdin.resume();
process.stdin.on('close', () => {
  if (!alive()) process.exit(0);
  signal('SIGTERM');
  const end = Date.now() + grace;
  setInterval(() => { if (!alive()) process.exit(0); if (Date.now() >= end) { signal('SIGKILL'); process.exit(0); } }, 25);
});`;

/**
 * Replaces Linux `prctl(PR_SET_PDEATHSIG)`, which Node cannot set: a worker killed without cleanup (SIGKILL) cannot run
 * `Wire.close`, so this detached process kills the vendor's process group for it. POSIX only: on Windows the vendor is
 * not a process group and `Wire.close` ends its tree with `taskkill /T /F`.
 * Null when there is nothing to guard or the watchdog could not start (the `parentGone` poll still ends a live worker).
 */
export function watchdog(group: number | undefined): ChildProcess | null {
  if (group === undefined || process.platform === 'win32') return null;
  const child = spawn(process.execPath, ['-e', WATCHDOG, String(group), String(TERMINATE_GRACE_MS)], { detached: true, stdio: ['pipe', 'ignore', 'ignore'], env: { ...process.env, NODE_OPTIONS: '' } });
  child.on('error', () => undefined);
  child.stdin.on('error', () => undefined);
  child.unref();
  if (child.stdin instanceof Socket) child.stdin.unref();
  return child;
}

/** Shared worker lifecycle: first turn, heartbeats, vendor events and staged delivery. */
export class Loop {
  private readonly presence: Presence;
  private readonly calls = new Map<string, string>();
  private poll = 0;

  /** `parent` is the process ID the worker must outlive; it is `process.ppid` at launch. */
  constructor(private readonly worker: LoopWorker, private readonly stop: Stop, private readonly deadline: number | null, private readonly parent: number) {
    this.presence = new Presence(() => worker.heartbeat());
  }

  async launch(): Promise<void> {
    const worker = this.worker;
    await worker.start();
    worker.emit({ type: 'ready', session: worker.identity, vendor: worker.name, pid: worker.host.pid ?? null, leases: 'managed' });
    const peers = await worker.prime(worker.store.peerContext(worker.identity, 'worker', 'session'));
    await worker.begin(this.guidance(peers));
  }

  guidance(peers: string): string {
    const { worker } = this;
    const base = catalog.workerSkill(worker.args.tools);
    const binding = { session: worker.identity, workspace: worker.store.workspace, coordinationScope: worker.store.coordinationScope, branch: worker.store.known(worker.identity, true).branch, tools: worker.toolNames, presence: 'managed', leaseRenewal: 'managed', delivery: 'managed', instructionsVersion: 1, instructionsSha256: createHash('sha256').update(base, 'utf8').digest('hex') };
    return `Available communication tools: ${encode(worker.toolNames).toString('utf8')}\n\n${base}\n\nCommunication binding: ${encode(binding).toString('utf8')}\n${peers}\nUser task:\n${worker.prompt}`;
  }

  async run(): Promise<void> {
    this.presence.reset();
    this.poll = monotonic();
    while (running(this.stop, this.deadline)) {
      // The parent died and this worker was re-parented: stop instead of running unowned.
      if (parentGone(this.parent)) break;
      await this.presence.tick();
      const event = await this.worker.host.event(POLL_MS);
      if (event !== null) await this.handle(event);
      await this.deliver();
    }
  }

  async handle(frame: Record<string, unknown>): Promise<void> {
    const event = objectOrEmpty(frame);
    const { worker } = this;
    const wasBusy = worker.busy;
    persistUsage(worker.store, worker.identity, worker.name, worker.session, event);
    const params = objectOrEmpty(event.params);
    const message = objectOrEmpty(event.message);
    const item = objectOrEmpty(params.item);
    const turn = objectOrEmpty(params.turn);
    if (worker.args.trace) this.trace(event, item, turn);
    await worker.onEvent(event, params, message, item, turn);
    if ('method' in event && 'id' in event) await worker.host.send(refusal(event.id));
    if (wasBusy && !worker.busy) {
      await worker.heartbeat({ status: 'available' });
      this.presence.reset();
    }
  }

  private trace(event: JsonObject, item: JsonObject, turn: JsonObject): void {
    const { worker } = this;
    traceUsage(worker, event);
    for (const record of traceTools(worker.identity, event, this.calls)) worker.emit(record);
    worker.emit({ type: 'protocol', method: event.method ?? event.type, itemType: item.type, status: turn.status });
  }

  async deliver(): Promise<void> {
    const { worker } = this;
    if (!worker.session || (worker.busy && !worker.deliverWhileBusy) || monotonic() - this.poll < POLL_MS) return;
    this.poll = monotonic();
    const items = worker.store.stage(worker.identity, 'managed:' + worker.name, worker.stageActionOnly());
    if (!items.length) {
      // The row expired while this process slept: renew now instead of waiting out the heartbeat period.
      if (items.reason === SESSION_EXPIRED) await this.presence.tick(true);
      return;
    }
    worker.busy = true;
    await worker.heartbeat({ status: 'busy' });
    this.presence.reset();
    worker.emit({ type: 'delivery', messages: items.map(item => item.id) });
    try {
      await worker.offer(items, withPeers(items, worker.store.peerContext(worker.identity, 'worker', 'session')));
    } catch (error) {
      worker.store.finishDispatch(worker.identity, items, (error as Error).message);
      throw error;
    }
    worker.store.finishDispatch(worker.identity, items);
  }
}

function traceUsage(worker: LoopWorker, event: JsonObject): void {
  const message = objectOrEmpty(event.message);
  const params = objectOrEmpty(event.params);
  let scope: string;
  let usage: unknown;
  if (event.method === 'thread/tokenUsage/updated') [scope, usage] = ['thread', params.tokenUsage];
  else if (event.type === 'result') [scope, usage] = ['result', event.usage];
  else if (event.type === 'assistant' || (event.type === 'message_end' && message.role === 'assistant')) [scope, usage] = ['message', message.usage];
  else return;
  if (usage !== undefined && usage !== null) worker.emit({ type: 'usage', vendor: worker.name, scope, messageId: message.id, usage });
}

const counted = (value: unknown): value is number => typeof value === 'number' && Number.isInteger(value) && value >= 0;

function usageSource(vendor: string, event: JsonObject, message: JsonObject, params: JsonObject): [unknown, RecordUsageInput['scope'] | null] {
  if (vendor === 'codex' && event.method === 'thread/tokenUsage/updated') return [objectOrEmpty(params.tokenUsage).total, 'cumulative'];
  if (vendor === 'claude' && event.type === 'result') return [event.usage, 'turn'];
  if (vendor === 'pi' && event.type === 'message_end' && message.role === 'assistant') return [message.usage, 'request'];
  return [null, null];
}

function usageKey(vendorSession: string, usage: JsonObject, scope: RecordUsageInput['scope'], event: JsonObject, message: JsonObject): string {
  const keydata = scope === 'cumulative' ? [vendorSession, usage] : (event.uuid ?? null) !== null || (message.timestamp ?? null) !== null ? [vendorSession, event.uuid ?? null, message.timestamp ?? null] : [vendorSession, randomUUID()];
  return createHash('sha256').update(encode(keydata)).digest('hex');
}

export function persistUsage(store: Pick<DeliveryStore, 'recordUsage'>, identity: string, vendor: string, vendorSession: string, event: JsonObject): void {
  const message = objectOrEmpty(event.message);
  const params = objectOrEmpty(event.params);
  const [usage, scope] = usageSource(vendor, event, message, params);
  if (typeof usage !== 'object' || usage === null || Array.isArray(usage) || scope === null) return;
  const source = usage as JsonObject;
  const record: RecordUsageInput = { key: usageKey(vendorSession, source, scope, event, message), scope };
  for (const [field, names] of USAGE_FIELDS) {
    const value = names.map(name => source[name]).find(counted);
    if (value !== undefined) record[field] = value;
  }
  const value = objectOrEmpty(objectOrEmpty(params.tokenUsage).last).inputTokens;
  if (counted(value)) record.contextTokens = value;
  if (vendor === 'pi' && PI_CONTEXT_FIELDS.every(key => counted(source[key]))) record.contextTokens = PI_CONTEXT_FIELDS.reduce((sum, key) => sum + (source[key] as number), 0);
  store.recordUsage(identity, record);
}

function rpcToolRecords(event: JsonObject): JsonObject[] {
  const item = objectOrEmpty(objectOrEmpty(event.params).item);
  if (item.type !== 'mcpToolCall') return [];
  if (event.method === 'item/started') return [{ type: 'tool-call', callId: item.id, server: item.server, tool: item.tool, input: item.arguments }];
  if (event.method === 'item/completed') return [{ type: 'tool-result', callId: item.id, server: item.server, tool: item.tool, result: item.result, error: item.error }];
  return [];
}

function executionToolRecords(event: JsonObject): JsonObject[] {
  if (event.type === 'tool_execution_start') return [{ type: 'tool-call', callId: event.toolCallId, tool: event.toolName, input: event.args }];
  if (event.type === 'tool_execution_end') return [{ type: 'tool-result', callId: event.toolCallId, tool: event.toolName, result: event.result, isError: event.isError }];
  return [];
}

function messageToolRecords(event: JsonObject, calls: Map<string, string>): JsonObject[] {
  const records: JsonObject[] = [];
  const content = objectOrEmpty(event.message).content;
  for (const item of Array.isArray(content) ? content : []) {
    if (!isObject(item)) continue;
    if (item.type === 'tool_use' && typeof item.id === 'string' && typeof item.name === 'string') {
      calls.set(item.id, item.name);
      records.push({ type: 'tool-call', callId: item.id, tool: item.name, input: item.input });
    } else if (item.type === 'tool_result') {
      const id = item.tool_use_id;
      const tool = typeof id === 'string' ? calls.get(id) ?? null : null;
      if (typeof id === 'string') calls.delete(id);
      records.push({ type: 'tool-result', callId: item.tool_use_id, tool, result: item.content, isError: item.is_error });
    }
  }
  return records;
}

export function traceTools(session: string, event: JsonObject, calls: Map<string, string>): JsonObject[] {
  const records = [...rpcToolRecords(event), ...executionToolRecords(event), ...messageToolRecords(event, calls)];
  for (const record of records) Object.assign(record, { session, at: now() });
  return records;
}
