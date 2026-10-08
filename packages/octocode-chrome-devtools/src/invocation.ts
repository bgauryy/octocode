import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { mkdirSync, openSync, writeSync, closeSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';
import {
  captureResult,
  type ExitStatus,
  type CapturedOutput,
} from './engine/capture-result.mjs';
import type { Invocation } from './command-inputs.js';
const scripts = join(dirname(fileURLToPath(import.meta.url)), 'engine');
const flags = (input?: Record<string, string | number | boolean | undefined>) =>
  Object.entries(input ?? {})
    .filter(([, value]) => value !== undefined)
    .flatMap(([key, value]) =>
      value === false
        ? []
        : [
            '--' + key.replace(/[A-Z]/g, letter => '-' + letter.toLowerCase()),
            ...(value === true ? [] : [String(value)]),
          ]
    );
const children = new Map<ChildProcessWithoutNullStreams, () => void>();
let shuttingDown = false;

// Serialize engine calls so concurrent MCP requests cannot race the same tab.
let queue = Promise.resolve();
function serialized<T>(task: () => Promise<T>): Promise<T> {
  const next = queue.then(task);
  queue = next.then(
    () => {},
    () => {}
  );
  return next;
}

async function invokeEngine(
  command: string,
  input: Invocation,
  signal?: AbortSignal
) {
  signal?.throwIfAborted();
  if (shuttingDown) throw Error('Chrome adapter is shutting down');
  let args = flags(input.connection),
    stdin: string | undefined;
  if (command === 'run' || command === 'step') {
    stdin = JSON.stringify(
      command === 'run' ? input.plan : { steps: [input.step] }
    );
    command = 'run';
    args.push('--plan', '-');
  } else if (command === 'skill')
    args = [
      input.topic,
      ...flags({ offset: input.offset, length: input.length }),
    ].filter(value => value !== undefined);
  else if (command === 'cdp')
    args.unshift(input.method!, '--params', JSON.stringify(input.params ?? {}));
  else if (command === 'protocol' && input.member) args.unshift(input.member);
  else if (command === 'check') args.unshift(input.recipe!);
  else if (command === 'schema')
    args = [input.command, input.recipe ?? input.operation].filter(
      value => value !== undefined
    );
  if (['artifact', 'query', 'open', 'cleanup'].includes(command)) {
    const { args: _legacy, ...typed } = input;
    const entries = Object.entries(typed).filter(
      ([, value]) => value !== undefined && value !== false
    );
    args.push(
      ...entries.flatMap(([key, value]) => [
        '--' + key.replace(/[A-Z]/g, c => '-' + c.toLowerCase()),
        ...(value === true
          ? []
          : [
              typeof value === 'object' ? JSON.stringify(value) : String(value),
            ]),
      ])
    );
  }
  if (input.options !== undefined)
    args.push('--options', JSON.stringify(input.options));
  if (input.args) args.push(...input.args);
  const directory = join(
    process.cwd(),
    '.octocode/tmp/chrome-devtools/adapter',
    randomUUID()
  );
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  const outputs = Object.fromEntries(
    ['stdout', 'stderr'].map(name => [
      name,
      {
        file: join(directory, name + '.txt'),
        fd: openSync(join(directory, name + '.txt'), 'wx', 0o600),
        bytes: 0,
      },
    ])
  ) as Record<'stdout' | 'stderr', CapturedOutput & { fd: number }>;
  const write = (name: 'stdout' | 'stderr', chunk: Buffer) => {
    const out = outputs[name];
    for (let at = 0; at < chunk.length;)
      at += writeSync(out.fd, chunk, at, chunk.length - at);
    out.bytes += chunk.length;
  };
  let child: ChildProcessWithoutNullStreams | undefined,
    timer: NodeJS.Timeout | undefined,
    force: NodeJS.Timeout | undefined,
    timedOut = false,
    aborted = false,
    abort: (() => void) | undefined;
  try {
    const status = await new Promise<ExitStatus>(resolve => {
      const active = spawn(
        process.execPath,
        [join(scripts, 'cli.mjs'), command, ...args],
        { stdio: ['pipe', 'pipe', 'pipe'] }
      );
      child = active;
      const terminate = () => {
        if (force) return;
        active.kill('SIGTERM');
        force = setTimeout(() => active.kill('SIGKILL'), 5000);
      };
      children.set(active, terminate);
      abort = () => {
        aborted = true;
        terminate();
      };
      signal?.addEventListener('abort', abort, { once: true });
      if (signal?.aborted) abort();
      timer = setTimeout(
        () => {
          timedOut = true;
          terminate();
        },
        (input.connection?.scriptTimeout ?? 300000) + 10000
      );
      child.stdout.on('data', chunk => write('stdout', chunk));
      child.stderr.on('data', chunk => write('stderr', chunk));
      child.stdin.on('error', () => {});
      child.stdin.end(stdin);
      child.once('error', error => {
        write('stderr', Buffer.from(error.message));
        resolve({ exitCode: 1 });
      });
      child.once('close', (code, signal) =>
        resolve({ exitCode: code ?? (signal === 'SIGINT' ? 130 : 143), signal })
      );
    });
    const result = await captureResult(
      {
        ...status,
        ...(timedOut ? { timedOut: true, outcomeUncertain: true } : {}),
        ...(aborted ? { cancelled: true, outcomeUncertain: true } : {}),
      },
      outputs,
      directory,
      command,
      scripts
    );
    if (!result.ok)
      throw Object.assign(Error(JSON.stringify(result)), {
        exitCode: status.exitCode,
      });
    return result;
  } finally {
    if (abort) signal?.removeEventListener('abort', abort);
    clearTimeout(timer);
    clearTimeout(force);
    if (child) children.delete(child);
    for (const out of Object.values(outputs)) closeSync(out.fd);
  }
}

export const invoke = (
  command: string,
  input: Invocation,
  signal?: AbortSignal
) => serialized(() => invokeEngine(command, input, signal));
export function stopEngine() {
  shuttingDown = true;
  for (const terminate of children.values()) terminate();
}
