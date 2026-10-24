import * as childProcess from 'node:child_process';
import { closeSync, existsSync, mkdtempSync, openSync, readdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Readable, Writable } from 'node:stream';
import type { PiContext, PiEvents, PiHost, PiMessage, PiSendOptions, PiTool } from '../src/hosts/shared/command.ts';
import type { PiBinding, PiInboxController } from '../src/hosts/pi-inbox.ts';
import type { TestContext } from 'node:test';
import type { JsonRecord } from './json-record.ts';
import { promisify } from 'node:util';
import assert from 'node:assert/strict';

// Every runtime these tests start inherits a private Octocode home: a session on a test database registers itself under
// the home (docs/DB.md#active-database-registry), and a real home must not see temporary databases.
export const octocodeHome = mkdtempSync(join(tmpdir(), 'communication-home-'));
process.env.OCTOCODE_HOME = octocodeHome;
process.on('exit', () => rmSync(octocodeHome, { recursive: true, force: true }));
/** The active-database registry under the private home (docs/DB.md#active-database-registry). */
export const registryDirectory = join(octocodeHome, 'agents-communication', 'active-databases');
export const registryEntries = () => existsSync(registryDirectory) ? readdirSync(registryDirectory).filter(name => name.endsWith('.json')).map(name => JSON.parse(readFileSync(join(registryDirectory, name), 'utf8'))) : [];
export const resetRegistry = () => rmSync(registryDirectory, { recursive: true, force: true });

/** The one row a statement must return: `get()` yields undefined when nothing matches. */
export function only<Value extends object>(row: Value | undefined): Value {
  assert.ok(row, 'The query returned no row');
  return row;
}

/** What the synchronous calls (execFileSync, spawnSync) throw for a failed child: it has `status`, never `killed`. Output is a string under `encoding`, else a Buffer. */
export type ChildFailure = Error & { status: number | null; signal: NodeJS.Signals | null; stdout: string | Buffer; stderr: string | Buffer };

export function isChildFailure(error: unknown): error is ChildFailure {
  return error instanceof Error && 'status' in error && 'stderr' in error;
}

/** What the asynchronous call (promisified execFile) rejects with for a failed or killed child: it has `killed` (a timeout kill versus a self-exit) and `code`, never `status`. */
export type ExecFailure = Error & { code: number | string | null; killed: boolean; signal: NodeJS.Signals | null; stdout: string | Buffer; stderr: string | Buffer };

export function isExecFailure(error: unknown): error is ExecFailure {
  return error instanceof Error && 'killed' in error && typeof error.killed === 'boolean' && 'stderr' in error;
}

/** Script entries run under this Node; every other executable (vendor fixtures, host binaries) runs as is. */
function invocationFor(executable: string, args: readonly string[]): { command: string; args: string[] } {
  if (existsSync(executable) && realpathSync(executable).endsWith('/tests/vendor-fixture.sh')) return { command: '/bin/sh', args: [executable, ...args] };
  if (/\.[cm]?js$/.test(executable)) return { command: process.execPath, args: [executable, ...args] };
  return { command: executable, args: [...args] };
}

/**
 * Runs a synchronous spawn with `options.input` delivered from a private file instead of a stdin pipe. On macOS,
 * Node's synchronous spawn writes `input` into a socketpair and can miss the write-ready wakeup: the child then waits in
 * read() and the call stalls until its timeout (about 1 in 100 one-MiB writes to an interpreter; asynchronous stdin and file
 * stdin never stalled in 1500 runs each). A file gives the child the same bytes followed by EOF.
 */
export function withFileInput<Options extends { input?: string | NodeJS.ArrayBufferView; stdio?: childProcess.StdioOptions }, Result>(options: Options | undefined, run: (options: Options | undefined) => Result): Result {
  const input = options?.input;
  if (options === undefined || input === undefined) return run(options);
  const directory = mkdtempSync(join(tmpdir(), 'communication-stdin-'));
  const path = join(directory, 'stdin');
  writeFileSync(path, input, { mode: 0o600 });
  const descriptor = openSync(path, 'r');
  try {
    const stdio: Exclude<childProcess.StdioOptions, childProcess.IOType> = typeof options.stdio === 'string' ? [options.stdio, options.stdio, options.stdio] : [...(options.stdio ?? ['pipe', 'pipe', 'pipe'])];
    stdio[0] = descriptor;
    return run({ ...options, input: undefined, stdio });
  } finally {
    closeSync(descriptor);
    rmSync(directory, { recursive: true, force: true });
  }
}

export function execFileSync(executable: string, args?: readonly string[]): NonSharedBuffer;
export function execFileSync(executable: string, args: readonly string[] | undefined, options: childProcess.ExecFileSyncOptionsWithStringEncoding): string;
export function execFileSync(executable: string, args: readonly string[] | undefined, options: childProcess.ExecFileSyncOptionsWithBufferEncoding): NonSharedBuffer;
export function execFileSync(executable: string, args: readonly string[] | undefined, options?: childProcess.ExecFileSyncOptions): string | NonSharedBuffer;
export function execFileSync(executable: string, args: readonly string[] = [], options?: childProcess.ExecFileSyncOptions): string | NonSharedBuffer {
  const invocation = invocationFor(executable, args);
  return withFileInput(options, resolved => childProcess.execFileSync(invocation.command, invocation.args, resolved));
}
export function spawnSync(executable: string, args?: readonly string[]): childProcess.SpawnSyncReturns<NonSharedBuffer>;
export function spawnSync(executable: string, args: readonly string[] | undefined, options: childProcess.SpawnSyncOptionsWithStringEncoding): childProcess.SpawnSyncReturns<string>;
export function spawnSync(executable: string, args: readonly string[] | undefined, options: childProcess.SpawnSyncOptionsWithBufferEncoding): childProcess.SpawnSyncReturns<NonSharedBuffer>;
export function spawnSync(executable: string, args: readonly string[] | undefined, options?: childProcess.SpawnSyncOptions): childProcess.SpawnSyncReturns<string | NonSharedBuffer>;
export function spawnSync(executable: string, args: readonly string[] = [], options?: childProcess.SpawnSyncOptions): childProcess.SpawnSyncReturns<string | NonSharedBuffer> {
  const invocation = invocationFor(executable, args);
  return withFileInput(options, resolved => childProcess.spawnSync(invocation.command, invocation.args, resolved));
}
/** The asynchronous form is only used through `promisify(execFile)`, which resolves to the promise-returning function below. */
export function execFile(executable: string, args: readonly string[] = [], callback?: (error: childProcess.ExecFileException | null, stdout: string, stderr: string) => void): childProcess.ChildProcess {
  const invocation = invocationFor(executable, args);
  return childProcess.execFile(invocation.command, invocation.args, callback);
}
type ExecFileOutput<Output> = childProcess.PromiseWithChild<{ stdout: Output; stderr: Output }>;
function execFileAsync(executable: string, args?: readonly string[]): ExecFileOutput<string>;
function execFileAsync(executable: string, args: readonly string[] | undefined, options: childProcess.ExecFileOptionsWithBufferEncoding): ExecFileOutput<NonSharedBuffer>;
function execFileAsync(executable: string, args: readonly string[] | undefined, options: childProcess.ExecFileOptionsWithStringEncoding): ExecFileOutput<string>;
function execFileAsync(executable: string, args: readonly string[] | undefined, options: childProcess.ExecFileOptions | undefined): ExecFileOutput<string | NonSharedBuffer>;
function execFileAsync(executable: string, args: readonly string[] = [], options?: childProcess.ExecFileOptions): ExecFileOutput<string | NonSharedBuffer> {
  const invocation = invocationFor(executable, args);
  return promisify(childProcess.execFile)(invocation.command, invocation.args, options);
}
execFile[promisify.custom] = execFileAsync;
type PipeOrNull = undefined | null | 'pipe' | 'overlapped';
type StdioStream<Mode, Stream> = Mode extends PipeOrNull ? Stream : null;
export function spawn(executable: string, args?: readonly string[], options?: childProcess.SpawnOptionsWithoutStdio): childProcess.ChildProcessWithoutNullStreams;
export function spawn<In extends childProcess.StdioNull | PipeOrNull, Out extends childProcess.StdioNull | PipeOrNull, Err extends childProcess.StdioNull | PipeOrNull>(
  executable: string, args: readonly string[] | undefined, options: childProcess.SpawnOptionsWithStdioTuple<In, Out, Err>,
): childProcess.ChildProcessByStdio<StdioStream<In, Writable>, StdioStream<Out, Readable>, StdioStream<Err, Readable>>;
export function spawn(executable: string, args: readonly string[] = [], options?: childProcess.SpawnOptions): childProcess.ChildProcess {
  const invocation = invocationFor(executable, args);
  return childProcess.spawn(invocation.command, invocation.args, options ?? {});
}

export const root = fileURLToPath(new URL('../', import.meta.url));
/** COMMUNICATION_BINARY override, else the package's built TypeScript runtime. */
export const binary = process.env.COMMUNICATION_BINARY || join(root, 'dist', 'communication.js');

/** Child environment with a short heartbeat period so presence and lease-renewal tests skip real 15 s waits. */
export const fastHeartbeatEnv = (env: NodeJS.ProcessEnv = process.env, ms: number | string = 100) => ({...env, OCTOCODE_COMMUNICATION_HEARTBEAT_MS: String(ms)});

/** Whether a value is an object a test may read fields from: untyped command payload JSON. */
export const isJsonRecord = (value: unknown): value is JsonRecord => typeof value === 'object' && value !== null;

/** A tool as Pi hands it to a test host: the name and the call that runs the bound runtime, its result details read untyped. */
export interface BoundTool {
  name: string;
  execute(id: string, input: Record<string, unknown>, signal?: AbortSignal): Promise<{ content: { type: 'text'; text: string }[]; details: JsonRecord }>;
}

/** The test view of a Pi tool: the same call, with the parsed CLI result readable field by field. */
export const boundTool = (tool: PiTool): BoundTool => ({
  name: tool.name,
  execute: async (id, input, signal) => {
    const result = await tool.execute(id, input, signal);
    return { content: result.content, details: result.details as JsonRecord };
  },
});

/** A Pi host that records what the adapters register: event handlers by name and tools, and passes messages to `onMessage`. */
export function fakePi(onMessage: (message: PiMessage, options: PiSendOptions) => void = () => {}) {
  const handlers = new Map<string, (event: JsonRecord, context: PiContext) => unknown>();
  const tools: PiTool[] = [];
  const host: PiHost = {
    // A test fires any event payload by name; the adapter's handler reads only the fields its event type declares.
    on: (name, handler) => { handlers.set(name, (event, context) => handler(event as PiEvents[typeof name], context)); },
    registerTool: tool => { tools.push(tool); },
    sendMessage: onMessage,
  };
  return { host, handlers, tools };
}

/** The binding of a controller that must be bound. */
export function boundBinding(controller: PiInboxController): PiBinding {
  const binding = controller.getBinding();
  assert.ok(binding, 'the controller is bound');
  return binding;
}

/** The session a bound controller names. */
export function boundSession(controller: PiInboxController): string {
  const { session } = boundBinding(controller);
  assert.ok(session, 'the binding names its session');
  return session;
}

/** True where the tests can use POSIX process groups, signals and Unix sockets. */
export const posix = process.platform !== 'win32';

/**
 * Polls `check` every `interval` ms until it returns truthy; fails with `message` after `timeout` ms. The bound is
 * monotonic, so a host that sleeps mid-test does not turn into a spurious timeout.
 */
export async function waitFor<Value>(check: () => Value | Promise<Value>, {timeout = 10000, interval = 20, message = 'Condition not reached'}: { timeout?: number; interval?: number; message?: string | (() => string) } = {}): Promise<Awaited<Value>> {
  const end = performance.now() + timeout;
  for (;;) {
    const value = await check();
    if (value) return value;
    if (performance.now() >= end) throw new Error(`${typeof message === 'function' ? message() : message} within ${timeout} ms`);
    await new Promise(resolve => setTimeout(resolve, interval));
  }
}

/** Fresh temporary directory; `real` resolves symlinked temp roots such as /var → /private/var. */
export function tempDir(prefix: string, { real = false, parent = tmpdir() }: { real?: boolean; parent?: string } = {}): string {
  const directory = mkdtempSync(join(parent, prefix));
  return real ? realpathSync(directory) : directory;
}

/** tempDir removed after test `t`. */
export function tempWorkspace(t: TestContext, prefix: string, options?: Parameters<typeof tempDir>[1]): string {
  const directory = tempDir(prefix, options);
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  return directory;
}

/** `command '<json input>' --workspace W --database D [--session S]` argv builder. */
export const commandArgs = (workspace: string, database: string) => (command: string, input: unknown = {}, session?: string): string[] =>
  [command, JSON.stringify(input), '--workspace', workspace, '--database', database, ...(session ? ['--session', session] : [])];

/** Synchronous JSON CLI call bound to one workspace and database. */
export function jsonCall(file: string, workspace: string, database: string, options: Omit<childProcess.ExecFileSyncOptions, 'encoding'> = {}) {
  const args = commandArgs(workspace, database);
  return (command: string, input?: unknown, session?: string): JsonRecord => JSON.parse(execFileSync(file, args(command, input, session), { encoding: 'utf8', ...options }));
}

export const reasoningCommands = ['send_message', 'notify_all', 'lock', 'lock_many', 'share_document'];
/** Adds a default reasoning to commands that require one; explicit input wins. */
export const withReasoning = (command: string, input: Record<string, unknown>): Record<string, unknown> => reasoningCommands.includes(command)
  ? { reasoning: `Verify ${command} behavior in this isolated regression fixture`, ...input }
  : input;

/** Valid slot payloads for the message kinds, shared by the store and CLI protocol tests. */
export const task = (extra = {}) => ({ objective: 'Review storage', output: 'Findings', done: 'Given v7, when migrated, then tests pass', ...extra });
export const decision = (extra = {}) => ({ question: 'Which store?', options: ['sqlite', 'postgres'], recommendation: 'sqlite', default: 'sqlite', ...extra });
export const restate = (request: number) => ({ request, objective: 'Review storage', done: 'tests pass' });
export const sbar = { situation: 'CI red', background: 'main', assessment: 'flaky', recommendation: 'rerun' };
export const issue = { what: 'crash', where: 'a.ts:3', when: 'on save', who: 'unknown', why: 'unknown', how: 'save twice', howMuch: 'every save' };
export const answer = (message: number, conclusion = 'Done', extra: Record<string, unknown> = {}) => ({ message, outcome: 'done', reply: { conclusion }, ...extra });
