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
import { packageTmpDir } from './engine/engine-io.mjs';
import type { ChromeInvocation } from '@octocodeai/config/chrome';
const scripts = join(dirname(fileURLToPath(import.meta.url)), 'engine');
const flags = (input?: Record<string, string | number | boolean | undefined>) =>
  Object.entries(input ?? {})
    .filter(([, value]) => value !== undefined)
    .flatMap(([key, value]) =>
      value === false
        ? []
        : [
            `--${key.replace(/[A-Z]/g, letter => `-${letter.toLowerCase()}`)}`,
            ...(value === true ? [] : [String(value)]),
          ]
    );
const typedFlagCommands = [
  'artifact',
  'query',
  'open',
  'cleanup',
  'session',
  'dev-check',
];
/** Engine argv of a typed `cookies` call; the schema already requires the acknowledgement. */
export function cookieArgs(input: ChromeInvocation): string[] {
  const from: Record<string, string | number | undefined> = input.from ?? {};
  return [
    ...(input.acknowledgeSecrets ? ['--i-understand-secrets'] : []),
    ...flags({
      fromPort: from.port,
      fromProfile: from.profile,
      sourcePort: from.sourcePort,
      fromStorageState: from.storageState,
      toPort: input.to?.port,
      confirmDomains: input.confirmDomains?.join(','),
      urls: input.urls?.join(','),
      exportStorageState: input.exportStorageState,
      dryRun: input.dryRun,
    }),
    ...(from.chromePath === undefined
      ? []
      : ['--chromePath', String(from.chromePath)]),
  ];
}
/** The whole engine deadline, covering a dev-check wait ceiling. */
const engineDeadline = (input: ChromeInvocation) =>
  (input.connection?.scriptTimeout ??
    Math.max(300000, (input.timeoutMs ?? 0) + 60000)) + 10000;
const CAUSE_CHARS = 300;
const oneLine = (text: string) =>
  text.replace(/\s+/g, ' ').trim().slice(0, CAUSE_CHARS);
/** First actionable cause plus the exact next call that reads the rest. */
type FailedResult = {
  error?: unknown;
  flow?: {
    failure?: { step?: number; error?: unknown };
    failedOptionalSteps?: { step?: number; error?: unknown }[];
  };
  timedOut?: boolean;
  cancelled?: boolean;
  exitCode?: number | null;
  next?: Record<string, { tool?: string; query?: unknown } | undefined>;
};
// Engine stderr opens with the sandbox banner and Node warnings; the cause is a later line.
const NOISE =
  /sandboxed \(node |ExperimentalWarning|--trace-warnings|^\[CDP_SANDBOX\]/;
const CAUSE_MARK =
  /FATAL|Refusing|blocked|Ambiguous|not found|No Chrome|Error:/i;
/** The line of multi-line stderr that names the failure: a marked line, else the first non-noise one. */
function causeLine(text: string) {
  const lines = text
    .split('\n')
    .map(line => line.trim())
    .filter(line => line && !NOISE.test(line));
  return lines.find(line => CAUSE_MARK.test(line)) ?? lines[0] ?? text;
}
export function failureSummary(result: FailedResult) {
  const failed = result.flow?.failure;
  const cause =
    typeof result.error === 'string' && result.error.trim()
      ? oneLine(causeLine(result.error))
      : typeof failed?.error === 'string' && failed.error.trim()
        ? `step ${failed.step ?? '?'}: ${oneLine(failed.error)}`
        : result.timedOut
          ? 'timed out'
          : result.cancelled
            ? 'cancelled'
            : `exit ${result.exitCode ?? 'unknown'}`;
  const call = result.next?.stderr ?? result.next?.capture;
  const skipped = Array.isArray(result.flow?.failedOptionalSteps)
    ? result.flow.failedOptionalSteps
        .map(
          step =>
            `step ${step.step ?? '?'}: ${oneLine(typeof step.error === 'string' ? step.error : '')}`
        )
        .join('; ')
    : '';
  return `Failed: ${cause}${skipped ? `; optional steps failed earlier: ${skipped}` : ''}${
    call?.tool ? `; next: ${call.tool} ${JSON.stringify(call.query)}` : ''
  }`;
}
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

const isStringList = (value: unknown): value is string[] =>
  Array.isArray(value) &&
  value.length > 0 &&
  value.every(item => typeof item === 'string');
/** Typed value flags that name their value list singularly; arrays travel as comma lists. */
const listFlagNames: Record<string, string> = { allowOrigins: 'allowOrigin' };
/** Typed `open` launch fields whose engine flag is not the kebab-case of the field name. */
const launchFlagNames: Record<string, string> = {
  enableFeatures: '--enableFeatures',
  disableFeatures: '--disableFeatures',
  blinkFeatures: '--blinkFeatures',
  windowSize: '--windowSize',
  userAgent: '--userAgent',
  chromePath: '--chromePath',
  chromeFlags: '--chrome-flags',
};
/** `proxy` object keys and the engine flags they travel as. */
const proxyFlagNames: Record<string, string> = {
  server: '--proxyServer',
  bypassList: '--proxyBypassList',
  pacUrl: '--proxyPacUrl',
};
/** Typed value flags of one `open` entry; the proxy object expands to its three engine flags. */
const openEntryFlags = (key: string, value: unknown): string[] => {
  if (key === 'proxy')
    return Object.entries(value as Record<string, string>).flatMap(
      ([name, text]) => [proxyFlagNames[name], text]
    );
  const flag =
    launchFlagNames[key] ??
    `--${(listFlagNames[key] ?? key).replace(/[A-Z]/g, c => `-${c.toLowerCase()}`)}`;
  return [
    flag,
    ...(value === true
      ? []
      : [
          isStringList(value)
            ? value.join(',')
            : typeof value === 'string'
              ? value
              : JSON.stringify(value),
        ]),
  ];
};
/** Engine argv of a typed `proxy` call: action, then the browser and listen ports. */
export function proxyArgs(input: ChromeInvocation): string[] {
  return [
    input.action!,
    ...flags({
      port: input.port,
      listenPort: input.listenPort,
      log: input.log,
    }),
  ];
}
/** Engine argv of a typed `flow` call; a saved plan travels on stdin as --plan -. */
export function flowArgs(input: ChromeInvocation): string[] {
  return [
    input.action!,
    ...(input.name === undefined ? [] : [input.name]),
    // Params ride on stdin (a secret must not sit in the process argument list); a saved plan owns stdin.
    ...(input.params === undefined
      ? []
      : input.plan === undefined
        ? ['--params', '-']
        : ['--params', JSON.stringify(input.params)]),
    ...(input.plan === undefined ? [] : ['--plan', '-']),
  ];
}
/** Engine command, argv and stdin of one typed call. */
export function engineArgs(command: string, input: ChromeInvocation) {
  let args = flags(input.connection),
    stdin: string | undefined;
  if (command === 'run') {
    stdin = JSON.stringify(input.plan);
    args.push('--plan', '-');
  } else if (command === 'skill')
    args = [
      input.topic,
      ...flags({ offset: input.offset, length: input.length }),
    ].filter(value => value !== undefined);
  else if (command === 'cdp') {
    args.unshift(input.method!, '--params', '-');
    if (input.acknowledge) args.push('--acknowledge');
    stdin = JSON.stringify(input.params ?? {});
  } else if (command === 'protocol') {
    if (input.member) args.unshift(input.member);
    if (input.search) args.push('--search', input.search);
  } else if (command === 'check') args.unshift(input.recipe!);
  else if (command === 'cookies') args = cookieArgs(input);
  else if (command === 'proxy') args = proxyArgs(input);
  else if (command === 'flow') {
    args = [...flowArgs(input), ...args];
    if (input.plan !== undefined) stdin = JSON.stringify(input.plan);
    else if (input.params !== undefined) stdin = JSON.stringify(input.params);
  } else if (command === 'script' && input.file) args.push(input.file);
  else if (command === 'schema')
    args = [input.command, input.recipe ?? input.operation].filter(
      value => value !== undefined
    );
  if (typedFlagCommands.includes(command)) {
    const { args: _args, ...typed } = input;
    const entries = Object.entries(typed).filter(
      ([, value]) => value !== undefined && value !== false
    );
    args.push(...entries.flatMap(([key, value]) => openEntryFlags(key, value)));
  }
  if (input.options !== undefined)
    args.push('--options', JSON.stringify(input.options));
  if (input.args) args.push(...input.args);
  return { command, args, stdin };
}

async function invokeEngine(
  command: string,
  input: ChromeInvocation,
  signal?: AbortSignal
) {
  signal?.throwIfAborted();
  if (shuttingDown) throw Error('Chrome adapter is shutting down');
  const prepared = engineArgs(command, input);
  command = prepared.command;
  const { args, stdin } = prepared;
  const directory = join(packageTmpDir(), 'adapter', randomUUID());
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  const outputs = Object.fromEntries(
    ['stdout', 'stderr'].map(name => [
      name,
      {
        file: join(directory, `${name}.txt`),
        fd: openSync(join(directory, `${name}.txt`), 'wx', 0o600),
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
      timer = setTimeout(() => {
        timedOut = true;
        terminate();
      }, engineDeadline(input));
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
      throw Object.assign(
        Error(
          JSON.stringify({
            summary: failureSummary(result),
            ...Object.fromEntries(
              Object.entries(result).filter(([key]) => key !== 'summary')
            ),
          })
        ),
        {
          exitCode: status.exitCode,
        }
      );
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
  input: ChromeInvocation,
  signal?: AbortSignal
) => serialized(() => invokeEngine(command, input, signal));
export function stopEngine() {
  shuttingDown = true;
  for (const terminate of children.values()) terminate();
}
