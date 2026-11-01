// Helpers for upgrade-path.mjs: argument parsing, tarball discovery, the
// isolated environment, process runs, a stdio MCP client, warm-daemon
// listing, and the graph-compatibility verdict. Pure functions are exported
// for tests/upgrade-path.test.mjs.
import { spawn, spawnSync } from 'node:child_process';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

export const USAGE =
  'Usage: node packages/octocode-native/scripts/upgrade-path.mjs --packages <dir> ' +
  '[--from octocode@<v>,octocode-mcp@<v>] --json <out.json> [--keep]';

/** Launcher names the `--from` list may pin. */
export const LAUNCHERS = ['octocode', 'octocode-mcp'];

export function parseArgs(argv) {
  const options = { packages: null, from: {}, json: null, keep: false, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    const value = () => {
      const next = argv[++index];
      if (next === undefined || next.startsWith('--')) throw new Error(`${flag} needs a value\n${USAGE}`);
      return next;
    };
    if (flag === '--packages') options.packages = value();
    else if (flag === '--from') options.from = parseFrom(value());
    else if (flag === '--json') options.json = value();
    else if (flag === '--keep') options.keep = true;
    else if (flag === '--help' || flag === '-h') options.help = true;
    else throw new Error(`unknown argument '${flag}'\n${USAGE}`);
  }
  if (!options.help && (!options.packages || !options.json)) throw new Error(`--packages and --json are required\n${USAGE}`);
  return options;
}

/** `octocode@20.1.0,octocode-mcp@19.1.0` → { octocode: '20.1.0', 'octocode-mcp': '19.1.0' }. */
export function parseFrom(text) {
  const from = {};
  for (const entry of text.split(',').map(part => part.trim()).filter(Boolean)) {
    const at = entry.lastIndexOf('@');
    const name = at > 0 ? entry.slice(0, at) : '';
    const version = at > 0 ? entry.slice(at + 1) : '';
    if (!LAUNCHERS.includes(name) || !version) {
      throw new Error(`--from entry '${entry}' must be <${LAUNCHERS.join('|')}>@<version>`);
    }
    from[name] = version;
  }
  return from;
}

const VERSION = String.raw`(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)`;

/**
 * The tarballs one upgrade needs, from a directory listing: both launchers
 * and the host platform's package, all at one version.
 */
export function findTarballs(names, suffix) {
  const pick = (pattern, label) => {
    const hits = names.map(name => [name, pattern.exec(name)]).filter(([, match]) => match);
    if (hits.length !== 1) {
      throw new Error(`expected one ${label} tarball, found ${hits.length ? hits.map(([name]) => name).join(', ') : 'none'}`);
    }
    return { file: hits[0][0], version: hits[0][1][1] };
  };
  const cli = pick(new RegExp(`^octocode-${VERSION}\\.tgz$`), 'octocode');
  const mcp = pick(new RegExp(`^octocode-mcp-${VERSION}\\.tgz$`), 'octocode-mcp');
  const platform = pick(new RegExp(`^octocodeai-octocode-native-${suffix}-${VERSION}\\.tgz$`), `${suffix} platform`);
  const versions = new Set([cli.version, mcp.version, platform.version]);
  if (versions.size !== 1) throw new Error(`tarball versions differ: ${[...versions].join(', ')}`);
  return { version: cli.version, cli: cli.file, mcp: mcp.file, platform: platform.file };
}

/** Variables that could reach real user state or credentials: never inherited. */
export function isInheritedSecretOrState(name) {
  const upper = name.toUpperCase();
  return (
    upper.startsWith('OCTOCODE_') ||
    upper.startsWith('NPM_CONFIG_') ||
    upper.startsWith('NPM_PACKAGE_') ||
    upper.startsWith('NPM_LIFECYCLE_') ||
    ['GH_TOKEN', 'GITHUB_TOKEN', 'GITHUB_PERSONAL_ACCESS_TOKEN', 'GH_CONFIG_DIR', 'XDG_CONFIG_HOME',
      'XDG_CACHE_HOME', 'XDG_DATA_HOME', 'NODE_OPTIONS', 'WORKSPACE_ROOT', 'ALLOWED_PATHS', 'TOOLS_TO_RUN',
      'DISABLE_TOOLS', 'ENABLE_LOCAL', 'ENABLE_CLONE', 'NPM_EXECPATH', 'INIT_CWD'].includes(upper)
  );
}

/** The environment every step runs with: real home, caches, prefix and temp replaced. */
export function isolatedEnv(base, dirs, platform = process.platform) {
  const env = {};
  for (const [name, value] of Object.entries(base)) if (!isInheritedSecretOrState(name)) env[name] = value;
  Object.assign(env, {
    HOME: dirs.home,
    USERPROFILE: dirs.home,
    OCTOCODE_HOME: join(dirs.home, '.octocode'),
    TMPDIR: dirs.tmp,
    TMP: dirs.tmp,
    TEMP: dirs.tmp,
    XDG_CONFIG_HOME: join(dirs.home, '.config'),
    XDG_CACHE_HOME: join(dirs.home, '.cache'),
    XDG_DATA_HOME: join(dirs.home, '.local', 'share'),
    npm_config_cache: dirs.npmCache,
    npm_config_prefix: dirs.globalPrefix,
    npm_config_userconfig: join(dirs.home, '.npmrc'),
    npm_config_update_notifier: 'false',
    npm_config_fund: 'false',
    npm_config_audit: 'false',
    npm_config_yes: 'true',
    npm_config_loglevel: 'warn',
  });
  if (platform === 'win32') {
    env.APPDATA = join(dirs.home, 'AppData', 'Roaming');
    env.LOCALAPPDATA = join(dirs.home, 'AppData', 'Local');
  }
  return env;
}

/** One argument for a cmd.exe command line (`shell: true` on Windows). */
export function quoteWin(arg) {
  const text = String(arg);
  if (text && !/[\s"&|<>^()%!,;=]/.test(text)) return text;
  return `"${text.replace(/(\\*)"/g, '$1$1\\"').replace(/(\\+)$/, '$1$1')}"`;
}

/** An npm shim: `npm.cmd`/`octocode.cmd` on Windows, the bare name elsewhere. */
export function shim(name, platform = process.platform) {
  return platform === 'win32' ? `${name}.cmd` : name;
}

/** A launcher's command in a project (`node_modules/.bin`) or a `--prefix` global install. */
export function binPath(dir, name, kind, platform = process.platform) {
  if (kind === 'project') return join(dir, 'node_modules', '.bin', shim(name, platform));
  return platform === 'win32' ? join(dir, shim(name, platform)) : join(dir, 'bin', name);
}

/** The installed package directory of `name` in a project or a `--prefix` global install. */
export function packageDir(dir, name, kind, platform = process.platform) {
  if (kind === 'project') return join(dir, 'node_modules', name);
  return platform === 'win32' ? join(dir, 'node_modules', name) : join(dir, 'lib', 'node_modules', name);
}

/** Code-unit order for sorting strings (locale-independent). */
export function compareText(left, right) {
  if (left === right) return 0;
  return left < right ? -1 : 1;
}

export function excerpt(text, limit = 600) {
  const value = String(text ?? '').replace(/\r\n/g, '\n').trim();
  return value.length > limit ? `${value.slice(0, limit)}… (+${value.length - limit} chars)` : value;
}

/** Run one command to completion; never throws. `.cmd` shims go through cmd.exe. */
export function run(command, args, { cwd, env, input, timeoutMs = 120_000 } = {}) {
  const viaShell = process.platform === 'win32' && /\.(cmd|bat)$/i.test(command);
  const started = Date.now();
  const result = viaShell
    ? spawnSync([command, ...args].map(quoteWin).join(' '), { cwd, env, input, encoding: 'utf8', timeout: timeoutMs, shell: true, windowsHide: true })
    : spawnSync(command, args, { cwd, env, input, encoding: 'utf8', timeout: timeoutMs, windowsHide: true });
  return {
    command: [command, ...args].join(' '),
    cwd,
    status: result.status,
    signal: result.signal,
    timedOut: result.error?.code === 'ETIMEDOUT',
    error: result.error && result.error.code !== 'ETIMEDOUT' ? result.error.message : undefined,
    ms: Date.now() - started,
    stdout: result.stdout ?? '',
    stderr: result.stderr ?? '',
  };
}

/** A newline-delimited JSON-RPC client over a child's stdio (an MCP stdio server). */
export class McpSession {
  constructor(command, args, { cwd, env }) {
    this.command = [command, ...args].join(' ');
    const viaShell = process.platform === 'win32' && /\.(cmd|bat)$/i.test(command);
    const stdio = ['pipe', 'pipe', 'pipe'];
    this.child = viaShell
      ? spawn([command, ...args].map(quoteWin).join(' '), { cwd, env, stdio, shell: true, windowsHide: true })
      : spawn(command, args, { cwd, env, stdio, windowsHide: true });
    this.stderr = '';
    this.pending = new Map();
    this.nextId = 1;
    this.exit = new Promise(resolve => {
      this.child.once('exit', (code, signal) => resolve({ code, signal }));
      this.child.once('error', error => resolve({ code: null, signal: null, error: error.message }));
    });
    let buffer = '';
    this.child.stdout.setEncoding('utf8');
    this.child.stdout.on('data', chunk => {
      buffer += chunk;
      let newline;
      while ((newline = buffer.indexOf('\n')) >= 0) {
        const line = buffer.slice(0, newline).trim();
        buffer = buffer.slice(newline + 1);
        let message;
        try { message = JSON.parse(line); } catch { continue; }
        const waiter = message && this.pending.get(message.id);
        if (waiter) {
          this.pending.delete(message.id);
          waiter(message);
        }
      }
    });
    this.child.stderr.setEncoding('utf8');
    this.child.stderr.on('data', chunk => { this.stderr += chunk; });
    this.child.stdin.on('error', () => {});
  }

  /** The reply to one request: `{ result }`, `{ error }`, or `{ closed }`/`{ timedOut }`. */
  request(method, params, timeoutMs = 60_000) {
    const id = this.nextId++;
    return new Promise(resolve => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        resolve({ timedOut: true });
      }, timeoutMs);
      this.pending.set(id, message => {
        clearTimeout(timer);
        resolve(message);
      });
      void this.exit.then(exit => {
        if (!this.pending.has(id)) return;
        this.pending.delete(id);
        clearTimeout(timer);
        resolve({ closed: exit });
      });
      this.child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
    });
  }

  notify(method, params = {}) {
    this.child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method, params })}\n`);
  }

  async initialize(timeoutMs = 120_000) {
    const reply = await this.request('initialize', {
      protocolVersion: '2025-06-18',
      capabilities: {},
      clientInfo: { name: 'octocode-upgrade-path', version: '1.0.0' },
    }, timeoutMs);
    if (reply.result) this.notify('notifications/initialized');
    return reply;
  }

  /** End stdin (a stdio server exits on EOF), then kill the tree if it lingers. */
  async close(graceMs = 8_000) {
    this.child.stdin.end();
    const timer = new Promise(resolve => setTimeout(() => resolve(null), graceMs));
    const exited = await Promise.race([this.exit, timer]);
    if (exited) return exited;
    killTree(this.child.pid);
    return (await Promise.race([this.exit, new Promise(resolve => setTimeout(() => resolve(null), 3_000))])) ?? { code: null, signal: 'SIGKILL' };
  }
}

export function killTree(pid) {
  if (!pid) return;
  if (process.platform === 'win32') spawnSync('taskkill', ['/pid', String(pid), '/T', '/F'], { windowsHide: true });
  else try { process.kill(pid, 'SIGKILL'); } catch { /* already gone */ }
}

/** Text of an MCP tools/call result, for content checks. */
export function callText(reply) {
  const result = reply?.result;
  if (!result) return '';
  const text = (result.content ?? []).map(part => part?.text ?? '').join('\n');
  return result.structuredContent ? `${text}\n${JSON.stringify(result.structuredContent)}` : text;
}

/**
 * The live `octocode serve` daemons whose sockets sit under `tmp` (posix):
 * pid, socket, and the socket's modification time (the server touches it on
 * every request it answers).
 */
export function listDaemons(tmp) {
  if (process.platform === 'win32') return [];
  const listing = spawnSync('ps', ['-axo', 'pid=,command='], { encoding: 'utf8' }).stdout ?? '';
  return parseDaemons(listing, tmp).map(daemon => ({ ...daemon, socketMtimeMs: mtimeMs(daemon.socket) }));
}

export function parseDaemons(psListing, tmp) {
  const daemons = [];
  for (const line of psListing.split('\n')) {
    const match = /^\s*(\d+)\s+(.*\bserve\s+--socket\s+(\S+).*)$/.exec(line);
    if (match && match[3].startsWith(tmp)) daemons.push({ pid: Number(match[1]), command: match[2], socket: match[3] });
  }
  return daemons;
}

function mtimeMs(path) {
  try { return statSync(path).mtimeMs; } catch { return null; }
}

/**
 * Whether a graph query by a different version than the one that built the
 * snapshot is safe: the same answer as a fresh build, or a clear error that
 * names the rebuild. A signal death, a timeout, or a different successful
 * answer is not.
 */
export function graphVerdict(query, expectedIds) {
  if (query.timedOut) return { ok: false, kind: 'timeout' };
  if (query.signal || (typeof query.status === 'number' && query.status >= 128)) return { ok: false, kind: 'crash' };
  if (query.status === 0) {
    const ids = graphResultIds(query.stdout);
    const same = ids !== null && ids.join('\n') === expectedIds.toSorted(compareText).join('\n');
    return { ok: same, kind: same ? 'same-data' : 'wrong-data', ids };
  }
  const message = `${query.stdout}\n${query.stderr}`;
  const clear = /rebuild|graphBuild|version|format|incompatible/i.test(message);
  return { ok: clear, kind: clear ? 'clear-error' : 'unclear-error' };
}

/**
 * A launcher run whose platform package npm skipped: it either fetched the
 * pinned binary from the registry and ran it, or failed fast naming the
 * missing package. A hang, a signal or an unexplained exit is not acceptable.
 */
export function fallbackVerdict(result, version, packageName) {
  if (result.timedOut) return { ok: false, kind: 'hang', note: `no answer within ${result.ms} ms` };
  if (result.status === 0 && result.stdout.trim() === `octocode ${version}`) {
    return { ok: true, kind: 'fetched', note: `registry fallback fetched ${packageName}@${version} (${result.ms} ms)` };
  }
  const text = `${result.stdout}\n${result.stderr}`;
  const clear = typeof result.status === 'number' && result.status > 0 && result.status < 128 && !result.signal &&
    text.includes(packageName) && /not installed/i.test(text);
  return {
    ok: clear,
    kind: clear ? 'clear-error' : 'unclear',
    note: `exit ${result.status} in ${result.ms} ms: ${excerpt(result.stderr || result.stdout, 400)}`,
  };
}

/** Sorted row ids of a `graphQuery --json` answer (`results[0].data.results[].id`), or null when unparsable. */
export function graphResultIds(stdout) {
  try {
    const parsed = JSON.parse(String(stdout).trim().split('\n').filter(Boolean).at(-1));
    const rows = parsed?.results?.[0]?.data?.results;
    return Array.isArray(rows) ? rows.map(row => row.id).toSorted(compareText) : null;
  } catch {
    return null;
  }
}

/** Relative paths of every file under `dir` (a home snapshot), sorted. */
export function listFiles(dir, prefix = '') {
  let entries;
  try { entries = readdirSync(join(dir, prefix), { withFileTypes: true }); } catch { return []; }
  return entries.flatMap(entry => {
    const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) return listFiles(dir, relative);
    return entry.isSocket?.() ? [] : [relative];
  }).toSorted(compareText);
}

export function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}
