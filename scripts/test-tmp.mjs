#!/usr/bin/env node
// Runs one test command with every temp file under the repo, never the OS temp dir.
// Usage, from a package directory: node ../../scripts/test-tmp.mjs <command> [args...]
//
// The run gets a private root, <repo>/.octocode/tmp/t/<package>-<pid> (gitignored), as
// TMPDIR, TMP and TEMP (and MAC_CHROMIUM_TMPDIR on macOS: Chrome reads that, not TMPDIR, for
// its SingletonSocket and temp files there). Node's os.tmpdir() and Rust's temp_dir() read
// these on every call, and children inherit them, so every mkdtemp(join(tmpdir(), ...))
// follows without touching its call site. When the command ends (pass, fail or signal) the
// processes still working inside the root are stopped and the root is removed; the wrapper
// exits with the command's code, or dies of the command's signal. Only SIGKILL of the
// wrapper itself leaves a root behind.
//
// Inside the repo, the root must still look like the OS temp dir to what runs in it: an empty
// package.json keeps the repo's package scope ("type": "module") from turning a CommonJS
// script written there into ESM, and GIT_CEILING_DIRECTORIES keeps git from finding the repo.
// Code that walks up to `.git` itself still finds this repo, so a suite whose code does that
// (agents-communication's coordinationScope) cannot run under this wrapper.
//
// A Unix socket path must fit sockaddr_un: 104 bytes with its NUL on macOS (Linux allows
// 108). The sockets tests make under the root (socketSuffixes) are checked before anything
// is created, and a checkout too deep for them is refused, never sent back to the OS temp dir.
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import { constants } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
/** The variables that name the temp dir, for Node, Rust, POSIX tools, Windows and Chrome on macOS. */
export const ENV_NAMES = [
  'TMPDIR',
  'TMP',
  'TEMP',
  ...(process.platform === 'darwin' ? ['MAC_CHROMIUM_TMPDIR'] : []),
];
/** sockaddr_un's path size on macOS, NUL included; the smallest of the platforms tests run on. */
export const SOCKET_PATH_MAX = 104;
/** The package folder's name in the root is cut to this many characters: each one comes off the socket budget. */
const NAME_LENGTH = 8;
/** How long a process left in the root gets to exit on SIGTERM before SIGKILL. */
const GRACE_MS = 2000;
const FORWARDED = ['SIGINT', 'SIGTERM', 'SIGHUP'];

/** The sockets tests put under the temp root, each at its longest. */
export function socketSuffixes(uid) {
  return [
    // packages/octocode-native/crates/cli/src/cli/serve/unix/sockets.rs: <tmp>/octocode-<uid>/serve-<16 hex>.sock
    {
      owner: 'octocode serve',
      suffix: `/octocode-${uid}/serve-${'0'.repeat(16)}.sock`,
    },
    // Chrome's process singleton, as measured on macOS: <MAC_CHROMIUM_TMPDIR>/com.google.Chrome.XXXXXX/SingletonSocket
    {
      owner: 'Chrome',
      suffix: '/com.google.Chrome.XXXXXX/SingletonSocket',
    },
  ];
}

/** Why `root` cannot hold the test sockets, or null when every one fits. */
export function socketProblem(root, uid) {
  const worst = socketSuffixes(uid)
    .map(({ owner, suffix }) => ({ owner, path: root + suffix }))
    .reduce((a, b) =>
      Buffer.byteLength(b.path) > Buffer.byteLength(a.path) ? b : a
    );
  const bytes = Buffer.byteLength(worst.path) + 1;
  if (bytes <= SOCKET_PATH_MAX) return null;
  const room = SOCKET_PATH_MAX - (bytes - Buffer.byteLength(root));
  return (
    `test temp root ${root} is too deep: the ${worst.owner} socket under it, ${worst.path}, ` +
    `takes ${bytes} bytes with its NUL, over the ${SOCKET_PATH_MAX}-byte sockaddr_un limit (macOS). ` +
    `Use a checkout whose test root is at most ${room} bytes long.`
  );
}

/** The run's root: <repo>/.octocode/tmp/t/<package folder without octocode-, cut short>-<pid>. */
export function rootFor(repo, cwd, pid) {
  const name =
    path
      .basename(cwd)
      .replace(/^octocode-/, '')
      .slice(0, NAME_LENGTH)
      .replace(/-+$/, '') || 'run';
  return path.join(repo, '.octocode', 'tmp', 't', `${name}-${pid}`);
}

function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

/** Pids of the other processes whose working directory, or a Unix socket, is inside `root` (from lsof). */
function processesInside(root) {
  const roots = [root];
  try {
    roots.push(fs.realpathSync(root));
  } catch {
    return [];
  }
  const inside = name =>
    roots.some(dir => name === dir || name.startsWith(dir + path.sep));
  // Without -a, lsof lists the union: every cwd and every Unix socket.
  const listed = spawnSync('lsof', ['-U', '-d', 'cwd', '-F', 'pn'], {
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });
  const found = new Set();
  let pid = null;
  for (const line of (listed.stdout ?? '').split('\n')) {
    if (line.startsWith('p')) pid = Number(line.slice(1));
    else if (
      line.startsWith('n') &&
      pid !== null &&
      pid !== process.pid &&
      inside(line.slice(1))
    )
      found.add(pid);
  }
  return [...found];
}

/** SIGTERM, then SIGKILL after the grace, for each process still working inside `root`. */
function stopProcessesInside(root) {
  if (process.platform === 'win32') return 0;
  const pids = processesInside(root);
  const signal = (pid, name) => {
    try {
      process.kill(pid, name);
    } catch {
      // already gone
    }
  };
  pids.forEach(pid => signal(pid, 'SIGTERM'));
  const pause = new Int32Array(new SharedArrayBuffer(4));
  const deadline = Date.now() + GRACE_MS;
  while (Date.now() < deadline && pids.some(alive))
    Atomics.wait(pause, 0, 0, 50);
  pids.filter(alive).forEach(pid => signal(pid, 'SIGKILL'));
  return pids.length;
}

/** Owner access to every directory under `dir`: a test can leave one at mode 000. */
function unlock(dir) {
  let stat;
  try {
    stat = fs.lstatSync(dir);
  } catch {
    return;
  }
  if (!stat.isDirectory()) return;
  fs.chmodSync(dir, 0o700);
  for (const name of fs.readdirSync(dir)) unlock(path.join(dir, name));
}

/** Stops what the run left in `root` and removes it; false when it could not be removed. */
function removeRoot(root) {
  const stopped = stopProcessesInside(root);
  if (stopped > 0)
    console.error(
      `test-tmp: stopped ${stopped} process(es) the run left in ${root}`
    );
  try {
    try {
      fs.rmSync(root, { recursive: true, force: true });
    } catch {
      unlock(root);
      fs.rmSync(root, { recursive: true, force: true });
    }
    return true;
  } catch (error) {
    console.error(`test-tmp: ${root} was not removed: ${error.message}`);
    return false;
  }
}

/** Windows runs package bins (.cmd) only through its shell. */
function spawnCommand(command, args, options) {
  if (process.platform !== 'win32') return spawn(command, args, options);
  const quote = arg =>
    /[\s"]/.test(arg) ? `"${arg.replaceAll('"', '\\"')}"` : arg;
  return spawn([command, ...args].map(quote).join(' '), {
    ...options,
    shell: true,
  });
}

function main(argv) {
  const [command, ...args] = argv;
  if (!command) {
    console.error('Usage: node scripts/test-tmp.mjs <command> [args...]');
    process.exit(2);
  }
  const root = rootFor(REPO, process.cwd(), process.pid);
  const problem =
    process.platform === 'win32' ? null : socketProblem(root, process.getuid());
  if (problem) {
    console.error(`test-tmp: ${problem}`);
    process.exit(2);
  }
  fs.mkdirSync(path.dirname(root), { recursive: true, mode: 0o700 });
  // A root with this pid is a leftover of a killed run: no live wrapper owns it.
  fs.rmSync(root, { recursive: true, force: true });
  fs.mkdirSync(root, { mode: 0o700 });
  fs.writeFileSync(path.join(root, 'package.json'), '{}\n');

  const env = { ...process.env };
  for (const name of ENV_NAMES) env[name] = root;
  env.GIT_CEILING_DIRECTORIES = [
    path.dirname(root),
    process.env.GIT_CEILING_DIRECTORIES,
  ]
    .filter(Boolean)
    .join(path.delimiter);
  const child = spawnCommand(command, args, { stdio: 'inherit', env });
  const forward = signal => child.kill(signal);
  for (const signal of FORWARDED) process.on(signal, forward);

  let started = true;
  child.once('error', error => {
    started = false;
    console.error(`test-tmp: cannot run ${command}: ${error.message}`);
  });
  child.once('close', (code, signal) => {
    for (const name of FORWARDED) process.off(name, forward);
    const removed = removeRoot(root);
    if (!started) process.exit(127);
    if (signal) {
      process.kill(process.pid, signal);
      // A signal whose default action does not end this process: exit as a shell reports it.
      process.exit(128 + (constants.signals[signal] ?? 0));
    }
    process.exit(code !== 0 ? (code ?? 1) : removed ? 0 : 1);
  });
}

if (import.meta.main) main(process.argv.slice(2));
