#!/usr/bin/env node
// Upgrade-path check for one release line: installs the published (`--from`)
// launchers, uses them and writes state, upgrades to the local tarballs in
// `--packages`, checks the new launchers read that state, rolls back, and
// covers global, npx and skipped-optional-dependency installs. Every install,
// home, cache, npm prefix and temp dir lives under one temporary root.
//
//   node packages/octocode-native/scripts/upgrade-path.mjs --packages <dir> \
//     [--from octocode@20.1.0,octocode-mcp@19.1.0] --json <out.json> [--keep]
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import {
  callText, findTarballs, graphResultIds, graphVerdict, isolatedEnv, killTree, listDaemons, listFiles,
  McpSession, packageDir, parseArgs, readJson, run, shim, USAGE,
} from './upgrade-path-lib.mjs';
import { pathChecks } from './upgrade-path-steps.mjs';

const require = createRequire(import.meta.url);
const { getPlatformSuffix, platformPackageName } = require('../bin/platform.cjs');

const DUMMY_TOKEN = 'ghp_upgradePathDummyToken000000000000000';
const NPM = shim('npm');
const NPX = shim('npx');

let options;
try {
  options = parseArgs(process.argv.slice(2));
} catch (error) {
  console.error(error.message);
  process.exit(2);
}
if (options.help) {
  console.log(USAGE);
  process.exit(0);
}

const suffix = getPlatformSuffix();
if (!suffix) {
  console.error(`upgrade-path: unsupported host ${process.platform}-${process.arch}`);
  process.exit(2);
}
const packagesDir = resolve(options.packages);
let tarballs;
try {
  tarballs = findTarballs(readdirSync(packagesDir), suffix);
} catch (error) {
  console.error(`upgrade-path: ${packagesDir}: ${error.message}`);
  process.exit(2);
}
const tgz = name => join(packagesDir, tarballs[name]);

// A short root: the warm-server socket path (<root>/t/octocode-<uid>/serve-<16 hex>.sock)
// must fit sockaddr_un, 104 bytes on macOS. A long TMPDIR falls back to /tmp.
const root = mkdtempSync(join(process.platform !== 'win32' && tmpdir().length > 56 ? '/tmp' : tmpdir(), 'ocu'));
const dirs = {
  home: join(root, 'home'),
  tmp: join(root, 't'),
  npmCache: join(root, 'npm-cache'),
  globalPrefix: join(root, 'global'),
};
for (const dir of Object.values(dirs)) mkdirSync(dir, { recursive: true });
// The home in its resolved form (macOS /var → /private/var): older servers allow
// only paths under the home and compare them resolved. The temp dir stays short.
dirs.home = realpathSync(dirs.home);
const env = isolatedEnv(process.env, dirs);
const workspace = join(dirs.home, 'ws');
mkdirSync(join(workspace, 'src'), { recursive: true });
writeFileSync(join(workspace, 'src', 'util.ts'), 'export function helper(x: number) {\n  return x + 1;\n}\n');
writeFileSync(join(workspace, 'src', 'main.ts'), "import { helper } from './util';\n\nexport function main() {\n  return helper(2);\n}\n");

const report = {
  schema: 1,
  startedAt: new Date().toISOString(),
  host: { platform: process.platform, arch: process.arch, suffix, node: process.version },
  packages: packagesDir,
  to: { version: tarballs.version, tarballs },
  from: {},
  root,
  steps: [],
};
const sessions = new Set();
const children = new Set();
let current = null;

/** Run a command inside the current step and record it. */
function exec(command, args, extra = {}) {
  const result = run(command, args, { cwd: root, timeoutMs: 120_000, ...extra, env: { ...env, ...extra.env } });
  current?.commands.push({
    command: result.command,
    cwd: result.cwd,
    status: result.status,
    signal: result.signal,
    timedOut: result.timedOut,
    error: result.error,
    ms: result.ms,
    stdout: result.stdout.slice(0, 2000),
    stderr: result.stderr.slice(0, 2000),
  });
  return result;
}

const npm = (args, cwd, extra = {}) => exec(NPM, [...args, '--no-audit', '--no-fund'], { cwd, timeoutMs: 600_000, ...extra });

/** One recorded step: `body` returns `{ ok, note?, skip? }` or throws. */
async function step(id, title, body, { required = true } = {}) {
  current = { id, title, required, status: 'FAIL', note: '', ms: 0, commands: [] };
  const started = Date.now();
  try {
    const outcome = (await body()) ?? { ok: true };
    current.note = outcome.note ?? '';
    current.status = outcome.skip ? 'SKIP' : outcome.ok ? 'PASS' : required ? 'FAIL' : 'WARN';
    if (outcome.data) current.data = outcome.data;
  } catch (error) {
    current.note = `threw: ${error.stack ?? error}`;
    current.status = required ? 'FAIL' : 'WARN';
  }
  current.ms = Date.now() - started;
  report.steps.push(current);
  console.log(`${current.status.padEnd(4)} ${id} — ${title}${current.note ? `\n       ${current.note.split('\n').join('\n       ')}` : ''}`);
  const finished = current;
  current = null;
  return finished;
}

function newProject(name) {
  const dir = join(root, 'projects', name);
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'package.json'), `${JSON.stringify({ name: `upgrade-path-${name}`, version: '1.0.0', private: true }, null, 2)}\n`);
  return dir;
}

/** Start an MCP session from an installed `octocode-mcp` (node + its `bin` entry, the file the shim runs). */
function startMcp(installDir, kind, extraEnv = {}) {
  const pkg = packageDir(installDir, 'octocode-mcp', kind);
  const manifest = readJson(join(pkg, 'package.json'));
  const entry = typeof manifest.bin === 'string' ? manifest.bin : manifest.bin['octocode-mcp'];
  return mcpCommand(process.execPath, [join(pkg, entry)], extraEnv);
}

/** An MCP client session; a dummy GitHub token keeps every credential lookup off the OS store. */
function mcpCommand(command, args, extraEnv = {}) {
  const session = new McpSession(command, args, {
    cwd: workspace,
    env: { ...env, GITHUB_TOKEN: DUMMY_TOKEN, ...extraEnv },
  });
  sessions.add(session);
  return session;
}

/** A local tool call that works on every contract era: the first argument shape the server accepts. */
async function mcpLocalSearch(session) {
  const shapes = [
    { path: workspace, matchString: ['helper'] },
    { path: workspace, matchString: 'helper' },
    { path: workspace, operation: 'text', searchText: 'helper' },
  ].map(query => ({ queries: [query] }));
  let reply;
  for (const args of shapes) {
    reply = await session.request('tools/call', { name: 'localSearch', arguments: args }, 90_000);
    if (reply.result && !reply.result.isError && callText(reply).includes('util.ts')) return { ok: true, reply, args };
  }
  return { ok: false, reply };
}

/** initialize → tools/list → tools/call localSearch on one session, recorded in the current step. */
async function mcpCheck(session, expectVersion) {
  const init = await session.initialize();
  const list = init.result ? await session.request('tools/list', {}) : init;
  const names = (list.result?.tools ?? []).map(tool => tool.name);
  const call = names.includes('localSearch') ? await mcpLocalSearch(session) : { ok: false };
  current.commands.push(mcpRecord(session, init, names, call));
  const server = init.result?.serverInfo ?? {};
  const versionOk = !expectVersion || server.version === expectVersion;
  const expected = versionOk ? '' : `; expected version ${expectVersion}`;
  return {
    ok: Boolean(init.result) && call.ok && versionOk,
    note: `serverInfo ${server.name ?? '?'} ${server.version ?? '?'}; ${names.length} tools; localSearch ${call.ok ? 'ok' : 'failed'}${expected}`,
    names,
  };
}

function mcpRecord(session, init, names, call) {
  const initError = init.error ?? init.closed ?? (init.timedOut ? 'timeout' : undefined);
  return {
    command: `${session.command} (stdio MCP: initialize, tools/list, tools/call localSearch)`,
    serverInfo: init.result?.serverInfo ?? null,
    initError,
    tools: names,
    call: callText(call.reply).slice(0, 600),
    callError: call.reply?.error ?? call.reply?.closed,
    stderr: session.stderr.slice(0, 1500),
  };
}

/** Write a query file and run `<bin> <tool> --input <file>` (no JSON on the command line: cmd.exe quoting). */
function toolCall(bin, tool, query, cwd = workspace) {
  const file = join(root, `query-${Date.now()}-${Math.random().toString(16).slice(2)}.json`);
  writeFileSync(file, JSON.stringify(query));
  return exec(bin, [tool, '--input', file], { cwd });
}

/** `localSearch` across contract eras: matchString as an array (20.2+) or a string (older). */
function cliLocalSearch(bin) {
  let result;
  for (const matchString of [['helper'], 'helper']) {
    result = toolCall(bin, 'localSearch', { queries: [{ path: workspace, matchString }] });
    if (result.status === 0 && result.stdout.includes('util.ts')) return { ok: true, result };
  }
  return { ok: false, result };
}

/** An npm registry read that fills the home response cache, across contract eras. */
function artifactSearch(bin) {
  let result;
  for (const query of [{ operation: 'resolve', ecosystem: 'npm', packageName: 'semver' }, { ecosystem: 'npm', packageName: 'semver' }]) {
    result = toolCall(bin, 'artifactSearch', { queries: [query] });
    if (result.status === 0) return result;
  }
  return result;
}

// The graph tools through their generated CLI commands, scoped to the workspace root.
function graphBuild(bin) {
  return exec(bin, ['graphBuild', '--json', JSON.stringify({ queries: [{ path: '.' }] })], { cwd: workspace });
}

function graphCallers(bin) {
  const query = { operation: 'callers', path: '.', target: 'src/util.ts#helper' };
  return exec(bin, ['graphQuery', '--json', JSON.stringify({ queries: [query] })], { cwd: workspace });
}

function startOldDaemon(binary) {
  const socketDir = join(dirs.tmp, `octocode-${process.getuid()}`);
  mkdirSync(socketDir, { recursive: true, mode: 0o700 });
  const socket = join(socketDir, 'serve-0123456789abcdef.sock');
  const child = spawn(binary, ['serve', '--socket', socket], { cwd: workspace, env, stdio: 'ignore' });
  const exit = new Promise(resolveExit => child.once('exit', (code, signal) => resolveExit({ code, signal })));
  children.add(child);
  return { child, socket, exit };
}

/** The installed platform binary: hoisted, or nested under the package that depends on it. */
function nativeBinary(dir, kind) {
  const name = platformPackageName(suffix);
  const file = process.platform === 'win32' ? 'octocode.exe' : 'octocode';
  const candidates = [
    packageDir(dir, name, kind),
    ...['octocode', 'octocode-mcp', '@octocodeai/octocode-native'].map(parent => join(packageDir(dir, parent, kind), 'node_modules', name)),
  ].map(candidate => join(candidate, file));
  return candidates.find(candidate => existsSync(candidate)) ?? candidates[0];
}

async function resolveFrom() {
  for (const name of ['octocode', 'octocode-mcp']) {
    if (options.from[name]) {
      report.from[name] = options.from[name];
      continue;
    }
    const view = run(NPM, ['view', name, 'dist-tags.latest'], { cwd: root, env, timeoutMs: 60_000 });
    if (view.status !== 0 || !view.stdout.trim()) throw new Error(`npm view ${name} dist-tags.latest failed: ${view.stderr}`);
    report.from[name] = view.stdout.trim();
  }
}

async function cleanup() {
  for (const session of sessions) await session.close(3_000);
  for (const child of children) killTree(child.pid);
  for (const daemon of listDaemons(dirs.tmp)) killTree(daemon.pid);
  if (!options.keep) {
    try {
      rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 500 });
    } catch (error) {
      console.error(`upgrade-path: could not remove ${root}: ${error.message}`);
    }
  }
}

const context = {
  DUMMY_TOKEN, NPX, artifactSearch, cliLocalSearch, dirs, env, exec, graphBuild, graphCallers, graphResultIds, graphVerdict, listDaemons,
  listFiles, mcpCheck, mcpCommand, mcpLocalSearch, nativeBinary, newProject, npm, report, root,
  startMcp, startOldDaemon, step, tarballs, tgz, workspace, suffix,
  get current() { return current; },
};

let exitCode = 0;
try {
  await resolveFrom();
  console.log(`upgrade-path: ${report.from.octocode} / octocode-mcp ${report.from['octocode-mcp']} → ${tarballs.version} on ${suffix}; root ${root}`);
  await pathChecks(context);
} catch (error) {
  report.fatal = String(error.stack ?? error);
  console.error(`upgrade-path: ${report.fatal}`);
  exitCode = 1;
} finally {
  await cleanup();
}
const count = status => report.steps.filter(entry => entry.status === status).length;
report.summary = { pass: count('PASS'), fail: count('FAIL'), warn: count('WARN'), skip: count('SKIP') };
report.finishedAt = new Date().toISOString();
mkdirSync(dirname(resolve(options.json)), { recursive: true });
writeFileSync(resolve(options.json), `${JSON.stringify(report, null, 2)}\n`);
console.log(`upgrade-path: ${report.summary.pass} pass, ${report.summary.fail} fail, ${report.summary.warn} warn, ${report.summary.skip} skip → ${resolve(options.json)}`);
process.exit(exitCode || (report.summary.fail ? 1 : 0));
