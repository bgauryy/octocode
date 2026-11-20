#!/usr/bin/env node
// `proxy start|status|stop --port N [--listen-port N] [--log]`: a loopback-only, token-guarded CDP proxy
// that lets several clients share one browser. `start` detaches a daemon (like `open` keeps Chrome alive);
// the token lives only in the 0600 state file and is printed once, by `start`.
import { spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import {
  mkdirSync,
  readFileSync,
  realpathSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { getProcessCommand, waitForExit } from './browser-process.mjs';
import { chromeCommands } from './chrome-contract.mjs';
import { startProxy } from './cdp-proxy-server.mjs';
import { packageTmpDir } from './engine-io.mjs';
import { errorMessage } from './error-message.mjs';
import { fail, optionValue, parse } from './cli-options.mjs';
import { checkRunning, stateDirFor } from './session-registry.mjs';

const actions = ['start', 'status', 'stop'];
const portOf = (text: string | undefined, flag: string) => {
  if (text === undefined) return undefined;
  const port = Number(text);
  if (!/^\d+$/.test(text) || port < 1 || port > 65535)
    fail(`${flag} needs a port from 1 to 65535`);
  return port;
};
/** Validated `proxy` argv: the action, the browser port and the start-only options. */
export function parseProxyArgs(args: string[]) {
  const { opts, positional } = parse(
    args,
    ['--port', '--listen-port'],
    ['--log']
  );
  const action = positional[0];
  if (positional.length !== 1 || !actions.includes(action))
    fail(`proxy needs one action: ${actions.join(', ')}`);
  const port =
    portOf(optionValue(opts, '--port'), '--port') ??
    fail('proxy needs --port, the browser CDP port');
  if (action !== 'start' && (opts['--listen-port'] || opts['--log']))
    fail('--listen-port and --log apply only to start');
  return {
    action,
    port,
    listenPort: portOf(optionValue(opts, '--listen-port'), '--listen-port'),
    log: opts['--log'] === true,
  };
}

/** The proxy state of one browser port: pid, ports, counters and the secret token (file mode 0600). */
export const proxyStateFile = (stateDir: string, port: number | string) =>
  join(stateDir, `cdp-proxy-${port}.json`);
type ProxyState = {
  pid: number;
  browserPort: number;
  listenPort?: number;
  addresses?: string[];
  token?: string;
  startedAt: number;
  logFile?: string;
  stats?: Record<string, number>;
  ready?: boolean;
  error?: string;
};
const readState = (file: string): ProxyState | null => {
  try {
    return JSON.parse(readFileSync(file, 'utf8'));
  } catch {
    return null;
  }
};
const alive = (state: ProxyState) => {
  try {
    process.kill(state.pid, 0);
  } catch {
    return false;
  }
  // Never trusts a recycled pid: the process must be this proxy daemon.
  return getProcessCommand(state.pid).includes('cdp-proxy');
};
const say = (payload: Record<string, unknown>) => {
  process.stdout.write(`${JSON.stringify(payload)}\n`);
};
const SECRET_NOTE =
  'SECRET: this URL carries the access token and is shown only now. Anyone who has it can drive the browser, including its cookies and logged-in pages. Do not log it or paste it into shared places.';

/** Stops the proxy of a browser port (used by `proxy stop` and by `cleanup`); true when one was running. */
export async function stopProxyFor(
  port: number | string,
  stateDir = stateDirFor(process.cwd())
) {
  const file = proxyStateFile(stateDir, port);
  const state = readState(file);
  if (!state) return false;
  const running = alive(state);
  if (running) {
    try {
      process.kill(state.pid, 'SIGTERM');
    } catch {
      // Already exited; the wait below confirms.
    }
    if (!(await waitForExit(state.pid, 3000))) {
      try {
        process.kill(state.pid, 'SIGKILL');
      } catch {
        // Already exited between checks.
      }
      await waitForExit(state.pid, 2000);
    }
  }
  rmSync(file, { force: true });
  return running;
}

async function start(input: ReturnType<typeof parseProxyArgs>) {
  const stateDir = stateDirFor(process.cwd());
  const file = proxyStateFile(stateDir, input.port);
  const existing = readState(file);
  if (existing && alive(existing))
    return say({
      status: 'ERROR',
      message: `A proxy already runs for browser port ${input.port} (pid ${existing.pid}, listen port ${existing.listenPort}); run proxy stop --port ${input.port} first`,
    });
  rmSync(file, { force: true });
  if (!(await checkRunning(String(input.port))))
    return say({
      status: 'ERROR',
      message: `No browser answers on port ${input.port}; run open --port ${input.port} first`,
    });
  const token = randomBytes(16).toString('hex');
  let logFile: string | undefined;
  if (input.log) {
    const dir = join(packageTmpDir(), 'proxy-captures');
    mkdirSync(dir, { recursive: true, mode: 0o700 });
    logFile = join(dir, `cdp-proxy-${input.port}-${Date.now()}.jsonl`);
    writeFileSync(logFile, '', { mode: 0o600 });
  }
  mkdirSync(stateDir, { recursive: true, mode: 0o700 });
  const child = spawn(
    process.execPath,
    [
      fileURLToPath(import.meta.url),
      '--serve',
      '--port',
      String(input.port),
      '--listen-port',
      String(input.listenPort ?? 0),
      ...(logFile ? ['--log-file', logFile] : []),
    ],
    {
      detached: true,
      stdio: 'ignore',
      env: { ...process.env, OCTOCODE_PROXY_TOKEN: token },
    }
  );
  child.unref();
  let exited = false;
  child.once('exit', () => (exited = true));
  for (let i = 0; i < 100; i++) {
    await new Promise(r => setTimeout(r, 100));
    const state = readState(file);
    if (state?.error) {
      rmSync(file, { force: true });
      return say({ status: 'ERROR', message: state.error });
    }
    if (state?.ready) {
      return say({
        status: 'PROXY_STARTED',
        url: `ws://127.0.0.1:${state.listenPort}/${token}`,
        secret: SECRET_NOTE,
        pid: state.pid,
        port: input.port,
        listenPort: state.listenPort,
        addresses: state.addresses,
        logFile: state.logFile,
      });
    }
    if (exited)
      return say({
        status: 'ERROR',
        message: 'The proxy process exited before it was ready',
      });
  }
  if (child.pid !== undefined)
    try {
      process.kill(child.pid, 'SIGKILL');
    } catch {
      // Child already exited; nothing to kill.
    }
  say({ status: 'ERROR', message: 'The proxy did not become ready in 10s' });
}

async function status(port: number) {
  const file = proxyStateFile(stateDirFor(process.cwd()), port);
  const state = readState(file);
  if (!state?.ready) return say({ status: 'PROXY_NOT_RUNNING', port });
  if (!alive(state)) {
    rmSync(file, { force: true });
    return say({ status: 'PROXY_NOT_RUNNING', port, stale: true });
  }
  // The token is never part of status output.
  say({
    status: 'PROXY_RUNNING',
    pid: state.pid,
    port,
    listenPort: state.listenPort,
    addresses: state.addresses,
    startedAt: state.startedAt,
    ...state.stats,
    logFile: state.logFile,
  });
}

/** The detached daemon: serves until stopped or until the browser connection drops. */
async function serve(args: string[]) {
  const { opts } = parse(
    args,
    ['--port', '--listen-port', '--log-file'],
    ['--serve']
  );
  const port = Number(optionValue(opts, '--port'));
  const token = process.env.OCTOCODE_PROXY_TOKEN ?? '';
  const file = proxyStateFile(stateDirFor(process.cwd()), port);
  const base = { pid: process.pid, browserPort: port, startedAt: Date.now() };
  const write = (state: ProxyState) => {
    writeFileSync(`${file}.tmp`, JSON.stringify(state), { mode: 0o600 });
    renameSync(`${file}.tmp`, file);
  };
  try {
    if (token.length < 32) throw new Error('Missing proxy token');
    const info = await checkRunning(String(port));
    if (!info) throw new Error(`No browser answers on port ${port}`);
    let dirty = false;
    const logFile = optionValue(opts, '--log-file');
    const snapshot = (): ProxyState => ({
      ...base,
      listenPort: proxy.listenPort,
      addresses: proxy.addresses,
      token,
      logFile,
      stats: { ...proxy.stats() },
      ready: true,
    });
    const stop = async () => {
      await proxy.close();
      rmSync(file, { force: true });
      process.exit(0);
    };
    const proxy = await startProxy({
      browserUrl: info.webSocketDebuggerUrl,
      token,
      listenPort: Number(optionValue(opts, '--listen-port') ?? 0),
      logFile,
      onBrowserClose: () => void stop(),
      onStats: () => (dirty = true),
    });
    write(snapshot());
    setInterval(() => {
      if (!dirty) return;
      dirty = false;
      write(snapshot());
    }, 500);
    process.on('SIGTERM', () => void stop());
    process.on('SIGINT', () => void stop());
  } catch (error) {
    write({ ...base, error: errorMessage(error) });
    process.exit(1);
  }
}

const isMain = () => {
  try {
    return (
      realpathSync(process.argv[1]) ===
      realpathSync(fileURLToPath(import.meta.url))
    );
  } catch {
    return false;
  }
};
if (isMain()) {
  const args = process.argv.slice(2);
  if (args[0] === '--serve') await serve(args);
  else if (args.includes('--help') || args.includes('-h')) {
    process.stdout.write(
      `proxy: ${chromeCommands.proxy}\nUsage: proxy start --port <browser port> [--listen-port <port>] [--log] | proxy status|stop --port <browser port>\n`
    );
  } else {
    let input: ReturnType<typeof parseProxyArgs> | undefined;
    try {
      input = parseProxyArgs(args);
    } catch (error) {
      process.stderr.write(`[PROXY] ${errorMessage(error)}\n`);
      process.exitCode = 2;
    }
    if (input) {
      try {
        if (input.action === 'start') await start(input);
        else if (input.action === 'status') await status(input.port);
        else {
          const stopped = await stopProxyFor(input.port);
          say({
            status: stopped ? 'PROXY_STOPPED' : 'PROXY_NOT_RUNNING',
            port: input.port,
          });
        }
      } catch (error) {
        say({ status: 'ERROR', message: errorMessage(error) });
        process.exitCode = 1;
      }
    }
  }
}
