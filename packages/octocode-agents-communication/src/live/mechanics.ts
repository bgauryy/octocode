// Live-harness mechanics: one CLI caller, one row reader, one worker, one waiter.
import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createInterface } from 'node:readline';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { bindingFlags, errorMessage, integer, isReply, parseReply, text, type Reply } from '../script-support.ts';
import type { DeliveryRow, DispatchRow, LeaseRow, MessageRow, Rows, SessionRow, WorkerEvent } from './grading.ts';

export const pause = (ms: number): Promise<void> => new Promise(done => setTimeout(done, ms));

/** A managed `run --trace` worker process and what it printed. */
export interface Worker {
  vendor: string; child: ChildProcess; events: WorkerEvent[]; stdout: string; stderr: string; buffer: string; ended: boolean;
  session?: string; parseError?: string; error?: string; exitCode?: number | null; exitSignal?: NodeJS.Signals | null;
}

const optional = <T>(read: (value: unknown, label: string) => T) => (value: unknown, label: string): T | null => value === null ? null : read(value, label);

/** Row readers: each column is checked once, so a schema drift fails loudly instead of grading `undefined`. */
const messageRow = (row: Reply): MessageRow => ({ id: integer(row.id, 'messages.id'), sender: text(row.sender, 'messages.sender'), target: text(row.target, 'messages.target'), body: text(row.body, 'messages.body'), replyTo: optional(integer)(row.replyTo, 'messages.replyTo'), replyRequired: integer(row.replyRequired, 'messages.replyRequired'), wake: text(row.wake, 'messages.wake') });
const deliveryRow = (row: Reply): DeliveryRow => ({ message: integer(row.message, 'deliveries.message'), recipient: text(row.recipient, 'deliveries.recipient'), acknowledgedAt: optional(integer)(row.acknowledgedAt, 'deliveries.acknowledgedAt') });
const dispatchRow = (row: Reply): DispatchRow => ({ message: integer(row.message, 'dispatches.message'), recipient: text(row.recipient, 'dispatches.recipient'), transport: text(row.transport, 'dispatches.transport'), state: text(row.state, 'dispatches.state'), submittedAt: optional(integer)(row.submittedAt, 'dispatches.submittedAt'), error: optional(text)(row.error, 'dispatches.error') });
const sessionRow = (row: Reply): SessionRow => ({ id: text(row.id, 'sessions.id'), name: text(row.name, 'sessions.name'), vendor: text(row.vendor, 'sessions.vendor'), vendorSession: optional(text)(row.vendorSession, 'sessions.vendorSession'), expiresAt: integer(row.expiresAt, 'sessions.expiresAt') });
const leaseId = (value: unknown): number | string | null => typeof value === 'number' || typeof value === 'string' || value === null ? value : text(value, 'lease id');
const leaseRow = (row: Reply): LeaseRow => ({ id: leaseId(row.id), path: text(row.path, 'lease path'), owner: text(row.owner, 'lease owner'), acquiredAt: integer(row.acquiredAt, 'lease acquiredAt') });

export function version(vendor: string): string {
  const run = spawnSync(vendor, ['--version'], { encoding: 'utf8', timeout: 15000 });
  if (run.error || run.status !== 0) throw new Error(`${vendor} is not usable (\`${vendor} --version\`: ${run.error?.message ?? `exit ${run.status}: ${run.stderr}`}); install it or pick another --vendor`);
  return run.stdout.trim();
}

/** The model Codex is configured with, read over its app-server JSON-RPC protocol. */
export async function configuredCodexModel(): Promise<string> {
  const cwd = mkdtempSync(join(tmpdir(), 'communication-codex-model-'));
  const child = spawn('codex', ['app-server'], { cwd, stdio: ['pipe', 'pipe', 'ignore'] });
  const lines = createInterface({ input: child.stdout });
  const waiting = new Map<unknown, (frame: Reply) => void>();
  lines.on('line', line => {
    let frame: Reply; try { frame = parseReply(line); } catch { return; }
    waiting.get(frame.id)?.(frame);
  });
  const request = (id: number, method: string, params: Reply): Promise<unknown> => new Promise((resolveCall, reject) => {
    const timer = setTimeout(() => reject(new Error(`Codex ${method} timed out`)), 15000);
    waiting.set(id, frame => {
      clearTimeout(timer);
      if (frame.error) reject(new Error(JSON.stringify(frame.error)));
      else resolveCall(frame.result);
    });
    child.stdin.write(JSON.stringify({ id, method, params }) + '\n');
  });
  try {
    await request(1, 'initialize', { clientInfo: { name: 'communication-live', version: '1' } });
    child.stdin.write(JSON.stringify({ method: 'initialized', params: {} }) + '\n');
    const read = await request(2, 'config/read', { includeLayers: false });
    const value = isReply(read) && isReply(read.config) ? read.config.model : undefined;
    if (typeof value !== 'string' || !value) throw new Error('Codex has no configured model; supply --model with an available model ID');
    return value;
  } finally {
    child.kill();
    rmSync(cwd, { recursive: true, force: true });
  }
}

/** A fresh workspace and database plus an async caller for the package CLI (15-second deadline per call). */
export interface Box { workspace: string; database: string; binary: string; cli: (name: string, input: Reply, session?: string) => Promise<Reply>; rows: () => Rows }

export function sandbox(pkg: string, prefix: string): Box {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), prefix)));
  const database = join(workspace, 'communication.sqlite'), binary = join(pkg, 'dist/communication.js');
  const cli = (name: string, input: Reply, session?: string): Promise<Reply> => new Promise((resolveCall, reject) => {
    const child = spawn(process.execPath, [binary, name, JSON.stringify(input), ...bindingFlags(workspace, database, session)], { stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '', stderr = '', settled = false;
    const finish = (error?: Error): void => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (error) reject(error);
      else { try { resolveCall(parseReply(stdout)); } catch (failure) { reject(failure); } }
    };
    const timer = setTimeout(() => { child.kill('SIGKILL'); finish(new Error(`${name} exceeded its 15-second deadline`)); }, 15000);
    child.stdout.setEncoding('utf8'); child.stderr.setEncoding('utf8');
    child.stdout.on('data', (chunk: string) => { stdout += chunk; });
    child.stderr.on('data', (chunk: string) => { stderr += chunk; });
    child.on('error', finish);
    child.on('close', (code: number | null) => finish(code === 0 ? undefined : new Error(`${name} exited ${code}: ${stderr}`)));
  });
  const rows = (): Rows => {
    const db = new DatabaseSync(database, { readOnly: true, timeout: 5000 });
    try { return {
      messages: db.prepare('SELECT id,sender,target,body,replyTo,replyRequired,wake FROM messages ORDER BY id').all().map(messageRow),
      deliveries: db.prepare('SELECT message,recipient,acknowledgedAt FROM deliveries ORDER BY message,recipient').all().map(deliveryRow),
      dispatches: db.prepare('SELECT message,recipient,transport,state,submittedAt,error FROM dispatches ORDER BY message,recipient').all().map(dispatchRow),
      sessions: db.prepare('SELECT id,name,vendor,vendorSession,expiresAt FROM sessions ORDER BY id').all().map(sessionRow),
      leases: db.prepare("SELECT json_extract(data,'$.leaseId') AS id,json_extract(data,'$.path') AS path,\"from\" AS owner,timestamp AS acquiredAt FROM records WHERE type='lease.acquired' ORDER BY id").all().map(leaseRow),
    }; } finally { db.close(); }
  };
  return { workspace, database, binary, cli, rows };
}

/** A managed `run --trace` worker whose JSON-lines events are collected as they arrive. */
export function startWorker({ box, vendor, model, name, prompt, durationMs, tools }: { box: Box; vendor: string; model: string; name: string; prompt: string; durationMs: number; tools: string }): Worker {
  const child = spawn(process.execPath, [box.binary, 'run', '--vendor', vendor, '--model', model, '--name', name, '--prompt', prompt, '--duration-ms', String(durationMs), '--trace', '--tools', tools, ...bindingFlags(box.workspace, box.database)], { stdio: ['ignore', 'pipe', 'pipe'] });
  const worker: Worker = { vendor, child, events: [], stdout: '', stderr: '', buffer: '', ended: false };
  child.stdout.setEncoding('utf8'); child.stderr.setEncoding('utf8');
  child.stdout.on('data', (chunk: string) => {
    worker.stdout += chunk; worker.buffer += chunk;
    const lines = worker.buffer.split('\n'); worker.buffer = lines.pop() ?? '';
    for (const line of lines) if (line) { try { worker.events.push(parseReply(line)); } catch { worker.parseError = line; } }
  });
  child.stderr.on('data', (chunk: string) => { worker.stderr += chunk; });
  child.on('error', error => { worker.error = error.message; worker.ended = true; });
  child.on('close', (code, signal) => { worker.ended = true; worker.exitCode = code; worker.exitSignal = signal; });
  return worker;
}

export async function stopWorkers(workers: Worker[]): Promise<void> {
  for (const worker of workers) if (!worker.ended) worker.child.kill('SIGTERM');
  for (let index = 0; index < 50 && workers.some(worker => !worker.ended); index++) await pause(100);
  for (const worker of workers) if (!worker.ended) worker.child.kill('SIGKILL');
  for (let index = 0; index < 20 && workers.some(worker => !worker.ended); index++) await pause(100);
}

/** Renews the given sessions (and their leases) every 10 seconds; returns the stop function. */
export function keepAlive(box: Box, sessions: string[], errors: string[]): () => Promise<void> {
  let renewing: Promise<unknown> | undefined;
  const timer = setInterval(() => {
    if (renewing) return;
    renewing = Promise.all(sessions.map(id => box.cli('heartbeat', { renewLeases: true }, id))).catch(error => { errors.push(errorMessage(error)); }).finally(() => { renewing = undefined; });
  }, 10000);
  return async () => { clearInterval(timer); await renewing; };
}

/** Polls `test` until it holds; a worker that ended or emitted non-JSON fails the wait at once. */
export async function until(test: () => boolean, label: string, { workers, deadline, timeout = Infinity }: { workers: Worker[]; deadline: number; timeout?: number }): Promise<void> {
  const limit = Math.min(deadline, Date.now() + timeout);
  while (Date.now() < limit) {
    if (test()) return;
    const failed = workers.find(worker => worker.ended || worker.parseError);
    if (failed) throw new Error(`${failed.vendor} failed before ${label}: ${failed.error ?? failed.parseError ?? failed.stderr}`);
    await pause(100);
  }
  throw new Error(`Timed out waiting for ${label}`);
}

export const turnsOf = (worker: Worker): number => worker.events.filter(event => event.type === 'turn-completed').length;
export const readyEvent = (worker: Worker): WorkerEvent | undefined => worker.events.find(event => event.type === 'ready');
/** The session a worker announced in its `ready` event. */
export const sessionOf = (worker: Worker): string => text(worker.session, `${worker.vendor} session`);
export const messageId = (reply: Reply): number => integer(reply.id, 'message id');
