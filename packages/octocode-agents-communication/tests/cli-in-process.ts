// Runs one CLI call in a long-lived worker: the same `main`, arguments, environment and exit status as a process, for the
// synchronous call shape tests use. Starting Node and loading the runtime for each call was nearly all of this suite's time.
// Set COMMUNICATION_REAL_CLI=1 to spawn every call for real.
import { join } from 'node:path';
import { MessageChannel, receiveMessageOnPort, Worker } from 'node:worker_threads';
import type { MessagePort } from 'node:worker_threads';

export interface CliCall { status: number; stdout: string; stderr: string; }

/** Commands that read the process's own stdin or live on past one call, as src/entries/cli.ts lists them. */
const streaming = new Set(['mcp', 'run', 'listen', 'host-hook', 'hook', 'view']);

/** What a real process owns and a worker cannot stand in for: stdin as a stream, a working directory, a timeout kill, inherited streams. */
export function runsInProcess(args: readonly string[], options: { cwd?: unknown; timeout?: unknown; stdio?: unknown } | undefined): boolean {
  if (process.env.COMMUNICATION_REAL_CLI || streaming.has(args[0] ?? '')) return false;
  const stdio = options?.stdio;
  const inherits = stdio === 'inherit' || (Array.isArray(stdio) && stdio.some(mode => mode !== 'pipe' && mode !== 'ignore' && mode !== undefined));
  return options?.cwd === undefined && options?.timeout === undefined && !inherits;
}

let worker: { thread: Worker; port: MessagePort; flag: Int32Array } | undefined;

function started(root: string, entry: string): NonNullable<typeof worker> {
  if (worker) return worker;
  const { port1, port2 } = new MessageChannel();
  const signal = new SharedArrayBuffer(4);
  const thread = new Worker(new URL('./cli-worker.ts', import.meta.url), {
    workerData: { port: port2, signal, runtime: join(root, 'dist', 'runtime.js'), entry },
    transferList: [port2],
    stdout: false,
    stderr: false,
  });
  thread.unref();
  port1.unref();
  worker = { thread, port: port1, flag: new Int32Array(signal) };
  return worker;
}

/** One CLI call. A call past `limitMs` kills the worker, as a stuck process would be: the next call starts a fresh one. */
export function callInProcess({ root, entry, args, env, input, limitMs = 120_000 }: { root: string; entry: string; args: readonly string[]; env: NodeJS.ProcessEnv; input?: string; limitMs?: number }): CliCall {
  const { thread, port, flag } = started(root, entry);
  Atomics.store(flag, 0, 0);
  thread.postMessage({ args: [...args], env: { ...env }, input }, []);
  if (Atomics.wait(flag, 0, 0, limitMs) === 'timed-out') {
    void thread.terminate();
    worker = undefined;
    throw new Error(`In-process CLI call exceeded ${limitMs} ms: ${args.join(' ')}`);
  }
  const reply = receiveMessageOnPort(port);
  if (!reply) throw new Error(`In-process CLI call left no reply: ${args.join(' ')}`);
  return reply.message as CliCall;
}
