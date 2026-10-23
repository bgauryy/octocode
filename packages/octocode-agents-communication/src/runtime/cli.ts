/** CLI ingress: bounded JSON input, catalog discovery and command dispatch. */
import { writeSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import dbProtocol from '../../docs/DB.md';
import * as activity from './activity.js';
import * as catalog from './catalog.js';
import { databasePath } from './config.js';
import * as database from './database.js';
import { exportDatabase } from './database-export.js';
import type { DeliveryRuntime } from './delivery-types.js';
import * as dispatch from './dispatch.js';
import { ValidationError, describe } from './errors.js';
import * as hostHooks from './host-hooks.js';
import { loopActive } from './loop-control.js';
import * as mcp from './mcp.js';
import * as proxy from './proxy.js';
import * as retention from './retention.js';
import { Store } from './store.js';
import { stripNulls } from './store-types.js';
import { isBlank } from './text.js';
import { monotonic } from './timing.js';
import { wellFormed, type Json } from './validation.js';
import { VERSION } from './version.js';
import * as view from './view.js';
import { MAX_FRAME } from './wire.js';
import { compact, decodeUtf8 } from './json-text.js';

/** What a command run needs from its process: standard streams and the path of the runnable entry script. */
export interface Context {
  /** Text for standard output; throws when the consumer is gone, so a caller can record the delivery as uncertain. */
  write(text: string): void;
  /** At most `limit` bytes of standard input. */
  readInput(limit: number): Buffer;
  /** This runtime's own entry script; worker processes and host hooks start it again. */
  entry: string;
}

/** The runnable entry script: every bundle of this package puts `communication.js` next to its chunks. */
const ENTRY = fileURLToPath(new URL('./communication.js', import.meta.url));

/** Waiting for a full non-blocking pipe to drain. */
const DRAIN = new Int32Array(new SharedArrayBuffer(4));

/** Writes all of `text` to standard output before returning; a closed reader throws `EPIPE` here, where an asynchronous stream write would only report it later. */
function writeStandardOutput(text: string): void {
  const bytes = Buffer.from(text);
  for (let offset = 0; offset < bytes.length;) {
    try {
      offset += writeSync(1, bytes, offset);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'EAGAIN') throw error;
      Atomics.wait(DRAIN, 0, 0, 2);
    }
  }
}

/** The process streams as a context. */
function processContext(entry = ENTRY): Context {
  return { write: writeStandardOutput, readInput: hostHooks.readStandardInput, entry };
}

/** A command line the parser refused; the process exits with status 2. */
class UsageError extends Error {
  override name = 'UsageError';
}

interface Arguments {
  workspace: string;
  database?: string;
  session?: string;
  vendor?: string;
  model?: string;
  name?: string;
  prompt?: string;
  tools?: string;
  durationMs?: number;
  trace: boolean;
  managed: boolean;
  compact: boolean;
  help: boolean;
  args: string[];
}

type TextOption = 'workspace' | 'database' | 'session' | 'vendor' | 'model' | 'name' | 'prompt' | 'tools';
const VALUE_FLAGS: Record<string, TextOption | 'durationMs'> = {
  '--workspace': 'workspace', '--database': 'database', '--session': 'session', '--vendor': 'vendor', '--model': 'model',
  '--name': 'name', '--prompt': 'prompt', '--tools': 'tools', '--duration-ms': 'durationMs',
};
const SWITCHES: Record<string, 'trace' | 'managed' | 'compact' | 'help'> = { '--trace': 'trace', '--managed': 'managed', '--compact': 'compact', '--help': 'help', '-h': 'help' };
const USAGE = 'usage: agents-communication [--workspace WORKSPACE] [--database DATABASE] [--session SESSION] [--vendor VENDOR] [--model MODEL] [--name NAME] [--prompt PROMPT] [--tools TOOLS] [--duration-ms DURATION_MS] [--trace] [--managed] [--compact] [-h] [-V] [args ...]';
const NEGATIVE_NUMBER = /^-\d+$|^-\d*\.\d+$/;

/** Whether a token reads as an option: it starts with a dash, is not a lone dash, a negative number or text with a space. */
function optionLike(token: string): boolean {
  return token.startsWith('-') && token.length > 1 && !NEGATIVE_NUMBER.test(token) && !token.includes(' ');
}

/** Parses the command line with positionals and options in any order; `-V` is handled by the caller. */
function parseArguments(argv: readonly string[]): Arguments | 'version' {
  const result: Arguments = { workspace: '.', trace: false, managed: false, compact: false, help: false, args: [] };
  const extras: string[] = [];
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index]!;
    if (token === '--') {
      result.args.push(...argv.slice(index + 1));
      break;
    }
    if (token === '-V' || token === '--version') return 'version';
    const equals = token.startsWith('--') ? token.indexOf('=') : -1;
    const flag = equals < 0 ? token : token.slice(0, equals);
    const inline = equals < 0 ? undefined : token.slice(equals + 1);
    if (Object.hasOwn(VALUE_FLAGS, flag)) {
      const property = VALUE_FLAGS[flag]!;
      let value = inline;
      if (value === undefined) {
        const following = argv[index + 1];
        if (following === undefined || following === '--' || optionLike(following)) throw new UsageError(`argument ${flag}: expected one argument`);
        value = following;
        index += 1;
      }
      if (property === 'durationMs') {
        if (!/^\s*[+-]?\d+(_\d+)*\s*$/.test(value)) throw new UsageError(`argument --duration-ms: invalid int value: '${value}'`);
        result.durationMs = Number(value.replaceAll('_', ''));
      } else result[property] = value;
    } else if (Object.hasOwn(SWITCHES, flag)) {
      if (inline !== undefined) throw new UsageError(`argument ${flag}: ignored explicit argument '${inline}'`);
      result[SWITCHES[flag]!] = true;
    } else if (optionLike(token)) extras.push(token);
    else result.args.push(token);
  }
  if (extras.length) throw new UsageError('unrecognized arguments: ' + extras.join(' '));
  if (result.durationMs !== undefined && result.durationMs < 0) throw new UsageError('--duration-ms must be nonnegative');
  return result;
}

function emit(context: Context, value: unknown): void {
  context.write(compact(value) + '\n');
}

function output(context: Context, value: unknown): void {
  emit(context, stripNulls(value));
}

/** Parses bounded JSON text (`-` reads standard input); strings must be well-formed because JSON and SQLite need valid UTF-8. */
function parseJson(context: Context, argument = '{}'): Json {
  if (argument === '-') {
    const raw = context.readInput(MAX_FRAME + 1);
    if (raw.length > MAX_FRAME) throw new ValidationError('JSON input exceeds 8 MiB');
    try {
      argument = decodeUtf8(raw);
    } catch {
      throw new ValidationError('Invalid JSON input: invalid UTF-8');
    }
  } else if (Buffer.byteLength(argument) > MAX_FRAME) throw new ValidationError('JSON input exceeds 8 MiB');
  let result: Json;
  try {
    result = JSON.parse(argument);
  } catch {
    throw new ValidationError('Invalid JSON input: not valid JSON');
  }
  if (!wellFormed(result)) throw new ValidationError('Invalid JSON input: surrogates not allowed');
  return result;
}

/** The command input in `text`, validated against the catalog entry `name`. */
function commandInput<N extends catalog.CommandName>(context: Context, name: N, text: string | undefined): catalog.CommandCall<N> {
  return catalog.command(name, parseJson(context, text ?? '{}'));
}

function arity(values: readonly unknown[], minimum: number, maximum: number): void {
  if (values.length < minimum || values.length > maximum) throw new ValidationError('Unexpected positional arguments; use --help or schema');
}

function delivery(context: Context): DeliveryRuntime {
  return {
    output: value => output(context, value),
    print: text => context.write(text + '\n'),
    open: (path, workspace, options) => new Store(path, workspace, options?.readOnly ?? false, options?.create ?? false),
  };
}

const READ_ONLY = ['binding', 'fetch', 'peers', 'inbox', 'read_document', 'context', 'check_paths', 'locks', 'check_write', 'health', 'completion-check'];

/** Runs one command line. Interrupts and failures are the caller's: see `main`. */
async function run(argv: readonly string[], context: Context): Promise<void> {
  const parsed = parseArguments(argv);
  if (parsed === 'version') return context.write(`agents-communication ${VERSION} (TypeScript)\n`);
  const args = parsed;
  if (args.help || !args.args.length) return output(context, args.args.length ? catalog.definition(args.args.join(' ')) : catalog.help());
  const [command = '', ...rest] = args.args;
  if (args.compact && !(command === 'schema' && rest.length === 1 && rest[0] === 'types')) throw new ValidationError('--compact is supported only for schema types');
  if (args.managed && command !== 'mcp') throw new ValidationError('--managed is supported only for mcp');
  if (args.tools !== undefined) {
    if (!['mcp', 'run'].includes(command) && !(command === 'schema' && rest.length === 1 && rest[0] === 'tools')) throw new ValidationError('--tools is supported only for mcp, run, and schema tools');
    catalog.selectedTools(args.tools);
  }
  // Reject flags the selected operation cannot consume before opening storage.
  const name = command === 'db' || (command === 'schema' && rest.length === 1 && rest[0] === 'tools') || (command === 'inbox' && rest[0] === 'wait') ? args.args.slice(0, 2).join(' ') : command;
  const allowed = new Set<string>([...(catalog.definition(name).cliFlags ?? []), 'help', 'version']);
  for (const token of argv) {
    if (token.startsWith('--') && !allowed.has(token.slice(2).split('=', 1)[0]!)) throw new ValidationError(`${token.split('=', 1)[0]} is not supported for ${name}`);
  }
  const runtime = delivery(context);
  const packageRoot = dirname(dirname(context.entry));
  if (command === 'skill') {
    arity(rest, 0, 0);
    return output(context, { instructions: catalog.skillInstructions(args.vendor), packageRoot, referenceRoot: join(packageRoot, 'docs') });
  }
  if (command === 'schema') {
    let value: unknown;
    if (!rest.length) value = catalog.catalog();
    else if (rest.length === 1 && rest[0] === 'tools') value = catalog.selectedTools(args.tools);
    else if (rest.length === 1 && rest[0] === 'types') value = args.compact ? catalog.typeSummary() : catalog.catalog().recordTypes;
    else if (rest.length === 2 && rest[0] === 'type') value = catalog.recordType(rest[1]!);
    else value = catalog.definition(rest.join(' '));
    return emit(context, value);
  }
  if (['mcp', 'run', 'listen', 'host-hook', 'host-config'].includes(command)) {
    arity(rest, 0, 0);
    const launcher = [process.execPath, context.entry];
    if (command === 'mcp') return mcp.run(args, value => emit(context, value));
    if (command === 'run') {
      return proxy.run(args, { ...runtime, launcher: { mcp: flags => ({ command: process.execPath, args: [context.entry, ...flags] }), binary: context.entry, piExtension: join(packageRoot, 'dist', 'hosts', 'pi-extension.js') } });
    }
    if (command === 'listen') return dispatch.listen(args, runtime);
    if (command === 'host-hook') return hostHooks.run(args, { output: runtime.output, open: runtime.open, readInput: context.readInput });
    return hostHooks.config(args, { output: runtime.output, launcher });
  }
  if (command === 'view') {
    arity(rest, 0, 1);
    return view.run(args, commandInput(context, 'view', rest[0]).input, { output: runtime.output, open: runtime.open, openBrowser: view.launchBrowser });
  }
  if (command === 'db') {
    arity(rest, 1, 2);
    const action = rest[0]!;
    if (action === 'protocol') {
      arity(rest, 1, 1);
      return output(context, { protocol: dbProtocol, database: catalog.catalog().database });
    }
    if (action === 'info') {
      arity(rest, 1, 1);
      return output(context, database.inspect(databasePath(args.database), args.workspace));
    }
    if (!['retention', 'compact', 'export'].includes(action)) throw new ValidationError('Use db info, db protocol, db export, db retention, or db compact');
    if (action === 'export') arity(rest, 2, 2);
    if (action === 'export') {
      const { input } = commandInput(context, 'db export', rest[1]);
      return output(context, await exportDatabase(databasePath(args.database), catalog.text(input, 'path')));
    }
    if (action === 'retention') {
      const { input } = commandInput(context, 'db retention', rest[1]);
      return output(context, retention.report(databasePath(args.database), input));
    }
    commandInput(context, 'db compact', rest[1]);
    return output(context, retention.compact(databasePath(args.database)));
  }
  const wait = command === 'inbox' && rest[0] === 'wait';
  arity(rest, Number(wait), wait ? 2 : 1);
  const call = commandInput(context, catalog.commandName(wait ? 'inbox wait' : command), rest[Number(wait)]);
  if (call.name === 'activity') return output(context, await activity.read(args.workspace, call.input));
  const session = args.session ?? '';
  if (!['join', 'peers', 'prune', 'health'].includes(command) && isBlank(session)) throw new ValidationError('--session required');
  const store = new Store(databasePath(args.database), args.workspace, READ_ONLY.includes(command), command === 'join');
  try {
    if (call.name === 'inbox wait') {
      const deadline = monotonic() + (call.input.timeoutMs ?? 30000);
      for (;;) {
        const result = store.inbox(session, call.input.after ?? 0);
        if (result.items?.length || monotonic() >= deadline) return output(context, result);
        await delay(Math.min(250, Math.max(0, deadline - monotonic())));
      }
    }
    switch (call.name) {
      case 'attach': return output(context, store.attach(session, call.input));
      case 'retry_delivery': return output(context, store.retryDelivery(session, call.input));
      case 'record_usage': return output(context, store.recordUsage(session, call.input));
      case 'completion-check': return output(context, store.completionCheck(session, call.input));
      case 'confirm_delivery':
        store.finishDispatch(session, call.input.items, null);
        return output(context, { submitted: true });
      case 'hook': return await dispatch.hook(store, session, call.input, runtime);
      case 'dispatch': return output(context, await dispatch.dispatch(store, session));
      default: return output(context, await store.call(session, call.name, call.input));
    }
  } finally {
    store.close();
  }
}

/**
 * Ends the process with 130 (SIGINT) or 143 (SIGTERM) unless a loop registered through `stopEvent` (loop-control.ts) has taken over
 * the signal and will stop cleanly. Returns the function that removes the handlers.
 */
function exitOnSignal(): () => void {
  const handlers = new Map<NodeJS.Signals, () => void>();
  for (const [signal, code] of [['SIGINT', 130], ['SIGTERM', 143]] as const) {
    const handler = (): void => {
      if (!loopActive()) process.exit(code);
    };
    handlers.set(signal, handler);
    process.on(signal, handler);
  }
  return () => {
    for (const [signal, handler] of handlers) process.off(signal, handler);
  };
}

/** The exit status and stderr text a failure maps to: expected errors report their message, defects name their type. */
function failure(error: unknown): { code: number; message: string } {
  if (error instanceof UsageError) return { code: 2, message: `${USAGE}\nagents-communication: error: ${error.message}` };
  return { code: 1, message: describe(error) };
}

/** Runs a command line as a process would and returns its exit status; failures go to standard error. */
export async function main(argv: readonly string[], context: Context = processContext()): Promise<number> {
  const release = exitOnSignal();
  try {
    await run(argv, context);
    return 0;
  } catch (error) {
    // A consumer closing the pipe is normal: any dispatch it left staged was already recorded `uncertain`.
    if ((error as NodeJS.ErrnoException | null)?.code === 'EPIPE') return 0;
    const { code, message } = failure(error);
    process.stderr.write(message + '\n');
    return code;
  } finally {
    release();
  }
}

/** Runs a command that prints one JSON value (its input, when given, replaces standard input) and returns that value. */
export async function capture(argv: readonly string[], input?: string): Promise<unknown> {
  let text = '';
  const release = exitOnSignal();
  try {
    await run(argv, { write: chunk => void (text += chunk), readInput: () => Buffer.from(input ?? ''), entry: ENTRY });
  } finally {
    release();
  }
  return JSON.parse(text);
}

/** Runs a long-lived or streaming command on the process streams (`input`, when given, replaces standard input); failures are thrown to the caller. */
export async function stream(argv: readonly string[], input?: string): Promise<void> {
  const release = exitOnSignal();
  try {
    await run(argv, input === undefined ? processContext() : { ...processContext(), readInput: () => Buffer.from(input) });
  } finally {
    release();
  }
}
