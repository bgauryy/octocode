import fs from 'node:fs';
import path from 'node:path';
import net from 'node:net';
import { spawnSync } from 'node:child_process';
import { processesUnder } from '../grammar-live-cleanup.mjs';

/** Configure the protocol fixture, never an installed semantic language server. */
export function configureMockLsp(home) {
  const directory = path.join(home, '.octocode');
  fs.mkdirSync(directory, { recursive: true });
  const configPath = path.join(directory, 'lsp-servers.json');
  const trace = path.join(directory, 'mock-lsp.jsonl');
  const languages = { '.ts': 'typescript', '.tsx': 'typescriptreact', '.js': 'javascript', '.jsx': 'javascriptreact', '.mjs': 'javascript', '.cjs': 'javascript', '.rs': 'rust' };
  const languageServers = Object.fromEntries(Object.entries(languages).map(([extension, languageId]) => [extension, {
    command: process.execPath,
    args: [path.join(import.meta.dirname, 'mock-lsp.mjs')],
    languageId,
    env: { OCTOCODE_TEST_LSP_TRACE: trace },
  }]));
  fs.writeFileSync(configPath, JSON.stringify({ languageServers }));
  return { OCTOCODE_LSP_AUTO_INSTALL: 'off', OCTOCODE_LSP_PREWARM: 'off', OCTOCODE_TRUST_PROJECT_LSP_CONFIG: 'false' };
}

/** Stop only brokers inside this test's private temp directory. */
export async function stopTestBrokers(tmp, { filesystem = fs, connect = socketPath => net.createConnection(socketPath), deadlineMs = 1000 } = {}) {
  const directories = filesystem.readdirSync(tmp, { withFileTypes: true }).filter(entry => entry.isDirectory() && entry.name.startsWith('octocode-'));
  const sockets = directories.flatMap(entry => {
    const directory = path.join(tmp, entry.name);
    return filesystem.readdirSync(directory).map(name => path.join(directory, name)).filter(file => filesystem.statSync(file, { throwIfNoEntry: false })?.isSocket());
  });
  await Promise.all(sockets.map(socketPath => new Promise(resolve => {
    const connection = connect(socketPath);
    const deadline = setTimeout(() => { connection.destroy(); resolve(); }, deadlineMs);
    const finish = () => { clearTimeout(deadline); resolve(); };
    connection.on('connect', () => connection.end('{"stop":true}\n'));
    connection.on('error', finish);
    connection.on('close', finish);
  })));
}

/** Private temp directories whose `octocode serve` brokers this process still owns. */
const owned = new Set();

/**
 * Own the brokers the CLI starts with `TMPDIR=tmp` (their sockets live in `tmp/octocode-<uid>`):
 * returns the release that stops them; the caller removes `tmp`. A run that ends before its
 * release (an exception past the caller, `process.exit`, SIGINT or SIGTERM) stops every broker
 * it still owns and removes those directories before it goes, so no run leaves a server behind.
 */
export function ownTestBrokers(tmp) {
  armExit();
  owned.add(tmp);
  return async () => {
    await stopTestBrokers(tmp);
    await brokersGone(tmp);
    owned.delete(tmp);
  };
}

/**
 * Wait until no broker answers on a socket under `tmp`: a stopped server drains the call it
 * took, and writes under `tmp` until it exits. Gives up after `deadlineMs`.
 */
export async function brokersGone(tmp, { deadlineMs = 10_000 } = {}) {
  const until = Date.now() + deadlineMs;
  const answers = socketPath => new Promise(resolve => {
    const connection = net.createConnection(socketPath);
    connection.on('connect', () => { connection.destroy(); resolve(true); });
    connection.on('error', () => resolve(false));
  });
  while (Date.now() < until) {
    const live = await Promise.all(brokerSockets(tmp).map(answers));
    if (!live.includes(true)) return;
    await new Promise(resolve => setTimeout(resolve, 50));
  }
}

let armed = false;
function armExit() {
  if (armed) return;
  armed = true;
  process.once('exit', releaseOwnedSync);
  for (const signal of ['SIGINT', 'SIGTERM']) {
    process.once(signal, () => {
      releaseOwnedSync();
      // The default action, now that the brokers are stopped, unless another handler owns it.
      if (!process.listenerCount(signal)) process.kill(process.pid, signal);
    });
  }
}

/**
 * Stop and remove every directory still owned, synchronously (an exit handler runs no I/O
 * loop). A stopped server drains the call it took, but that call may wait on a mock this
 * blocked process serves, and a client whose server goes falls back to running the call
 * itself under the directory. So every process under a directory (`processesUnder`: naming
 * it, or holding a file or working directory in it, all of this run's) is stopped by its PID
 * before the directory goes.
 */
function releaseOwnedSync() {
  if (!owned.size) return;
  const directories = [...owned];
  owned.clear();
  const module = JSON.stringify(import.meta.url);
  spawnSync(process.execPath, ['--input-type=module', '-e',
    `import { stopTestBrokers } from ${module}; for (const dir of ${JSON.stringify(directories)}) await stopTestBrokers(dir);`,
  ], { stdio: 'ignore', timeout: 5_000 });
  for (const dir of directories) {
    for (const signal of ['SIGTERM', 'SIGKILL']) {
      for (const { pid } of processesUnder(dir)) {
        try { process.kill(pid, signal); } catch { /* already gone */ }
      }
      for (let wait = 0; wait < 20 && processesUnder(dir).length; wait += 1) spawnSync('sleep', ['0.1']);
    }
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

/** The broker sockets under `tmp` (`tmp/octocode-<uid>/serve-*.sock`). */
function brokerSockets(tmp) {
  try {
    return fs.readdirSync(tmp, { withFileTypes: true })
      .filter(entry => entry.isDirectory() && entry.name.startsWith('octocode-'))
      .flatMap(entry => fs.readdirSync(path.join(tmp, entry.name)).map(name => path.join(tmp, entry.name, name)))
      .filter(file => fs.statSync(file, { throwIfNoEntry: false })?.isSocket());
  } catch { return []; }
}
