#!/usr/bin/env node
/**
 * Runtime smoke of one staged platform package (`npm/<platform>`), on that
 * platform: the proof a cross-built binary runs where it ships.
 *
 *   smoke-binary.cjs <dir> [--json FILE]                 run the binary on this host
 *   smoke-binary.cjs <dir> --container IMAGE [--json F]  run it inside `docker run IMAGE`
 *   smoke-binary.cjs <dir> --static [--json FILE]        identity checks only, any host
 *
 * Identity (binary-identity.cjs): format, CPU, libc, embedded version and
 * contract fingerprint. Runtime: `--version` reports the package version, the
 * binary reports the provenance fingerprint, `schema` lists the tools, and
 * `localSearch` and `localFetch` find a fixture line through the CLI and
 * through a stdio `octocode mcp` session. Node stays on the host and drives a
 * container over stdio, so a minimal image (glibc floor, Alpine) needs no Node.
 * Calls run with a temporary OCTOCODE_HOME and OCTOCODE_STORAGE_MODE=memory:
 * in process, so no warm `octocode serve` outlives the smoke.
 *
 * Prints one line per check on stderr and the JSON report on stdout or to
 * --json; exits 1 when a check fails.
 */
'use strict';

const { spawn, spawnSync } = require('child_process');
const { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } = require('fs');
const { tmpdir } = require('os');
const path = require('path');
const { parseArgs } = require('util');
const { getPlatformSuffix } = require('../bin/platform.cjs');
const { expectedFingerprint, identityChecks, readPackage } = require('./binary-identity.cjs');
const { SMOKE_TIMEOUT_MS, verifyBinaryRuns } = require('./stage-binary.cjs');

const NEEDLE = 'octocodeSmokeNeedle';
const FIXTURE = { path: 'src/smoke.txt', line: 2, text: `smoke fixture\nconst ${NEEDLE} = 1;\n` };
const QUERY = { path: FIXTURE.path, matchString: [NEEDLE] };
const SEARCH = { path: '.', matchString: [NEEDLE] };
const MCP_PROTOCOL = '2025-06-18';

/** The first result row's `data` of a tool envelope. */
function firstData(envelope) {
  const data = envelope?.results?.[0]?.data;
  if (!data) throw new Error(`no result data: ${JSON.stringify(envelope)}`);
  return data;
}

/** A `localSearch` envelope must hold the fixture line. */
function checkSearch(envelope) {
  const file = firstData(envelope).files?.find(entry => entry.path.replace(/\\/g, '/') === FIXTURE.path);
  if (!file?.matches?.some(match => match.line === FIXTURE.line && match.value.includes(NEEDLE))) {
    throw new Error(`${FIXTURE.path}:${FIXTURE.line} not matched: ${JSON.stringify(envelope)}`);
  }
  return `${FIXTURE.path}:${FIXTURE.line}`;
}

/** A `localFetch` matchString envelope must return the fixture line. */
function checkFetch(envelope) {
  const data = firstData(envelope);
  if (!data.content?.includes(NEEDLE) || !data.matchedLines?.includes(FIXTURE.line)) {
    throw new Error(`line ${FIXTURE.line} not fetched: ${JSON.stringify(envelope)}`);
  }
  return `matchedLines ${data.matchedLines.join(',')}`;
}

/** Tool names must include both tools the smoke calls. */
function checkToolNames(names) {
  const missing = ['localSearch', 'localFetch'].filter(name => !names.includes(name));
  if (missing.length) throw new Error(`missing ${missing.join(', ')} in [${names.join(', ')}]`);
  return `${names.length} tools`;
}

/** `schema --json`: the CLI tool catalog, counted consistently. */
function checkCatalog(catalog) {
  if (catalog?.kind !== 'octocode.toolCatalog' || catalog.tools?.length !== catalog.toolCount) {
    throw new Error(`not a consistent tool catalog: ${JSON.stringify(catalog)}`);
  }
  return checkToolNames(catalog.tools.map(tool => tool.name));
}

/** A temp workspace and home, and how to start the binary in them (on the host or in `container`). */
function smokeContext(binary, container) {
  const work = realpathSync(mkdtempSync(path.join(tmpdir(), 'octocode-smoke-')));
  const home = path.join(work, 'home');
  const workspace = path.join(work, 'workspace');
  mkdirSync(home);
  mkdirSync(path.join(workspace, path.dirname(FIXTURE.path)), { recursive: true });
  writeFileSync(path.join(workspace, FIXTURE.path), FIXTURE.text);
  const env = { OCTOCODE_HOME: home, OCTOCODE_STORAGE_MODE: 'memory', HOME: home };
  const command = args => {
    if (!container) return [binary, args];
    const user = process.getuid ? ['--user', `${process.getuid()}:${process.getgid()}`] : [];
    const binaryDir = path.dirname(binary);
    return ['docker', [
      'run', '--rm', '-i', ...user, '-v', `${binaryDir}:${binaryDir}:ro`, '-v', `${work}:${work}`, '-w', workspace,
      ...Object.entries(env).flatMap(([key, value]) => ['-e', `${key}=${value}`]), container, binary, ...args,
    ]];
  };
  const inherited = Object.entries(process.env).filter(([key]) => !key.startsWith('OCTOCODE_'));
  const options = container ? {} : { cwd: workspace, env: { ...Object.fromEntries(inherited), ...env } };
  const run = (args, input) => {
    const [file, argv] = command(args);
    return spawnSync(file, argv, { ...options, input, encoding: 'utf8', timeout: SMOKE_TIMEOUT_MS, maxBuffer: 256 * 1024 * 1024 });
  };
  return { command, options, run, cleanup: () => rmSync(work, { recursive: true, force: true }) };
}

/** Run a command that must exit 0 and print JSON; returns the parsed stdout. */
function runJson(context, args, input) {
  const ran = context.run(args, input);
  if (ran.status !== 0) throw new Error(`exit ${ran.status ?? ran.signal ?? ran.error?.message}: ${ran.stderr}${ran.stdout}`);
  return JSON.parse(ran.stdout);
}

const toolCall = (context, tool, query) => runJson(context, [tool, '--json', '--input', '-'], JSON.stringify({ queries: [query] }));

/** One stdio MCP session: initialize, tools/list, a localFetch tools/call, then EOF; the server must exit 0. */
async function mcpSession(context) {
  const [file, argv] = context.command(['mcp']);
  const child = spawn(file, argv, { ...context.options, stdio: ['pipe', 'pipe', 'pipe'] });
  const pending = new Map();
  let stderr = '';
  let buffer = '';
  const failAll = error => { for (const reject of pending.values()) reject.fail(error); pending.clear(); };
  const exited = new Promise(done => child.on('close', (code, signal) => { failAll(new Error(`server exited ${code ?? signal}: ${stderr}`)); done(code ?? signal); }));
  child.on('error', failAll);
  child.stderr.setEncoding('utf8').on('data', chunk => { stderr += chunk; });
  child.stdout.setEncoding('utf8').on('data', chunk => {
    buffer += chunk;
    for (let end = buffer.indexOf('\n'); end >= 0; end = buffer.indexOf('\n')) {
      const line = buffer.slice(0, end);
      buffer = buffer.slice(end + 1);
      if (!line.trim()) continue;
      try {
        const message = JSON.parse(line);
        pending.get(message.id)?.reply(message);
      } catch (error) {
        failAll(new Error(`invalid MCP frame ${line}: ${error.message}`));
      }
    }
  });
  let id = 0;
  const rpc = (method, params) => new Promise((done, fail) => {
    const request = ++id;
    const timer = setTimeout(() => fail(new Error(`${method}: no reply in ${SMOKE_TIMEOUT_MS} ms; stderr: ${stderr}`)), SMOKE_TIMEOUT_MS);
    const settle = action => value => { clearTimeout(timer); pending.delete(request); action(value); };
    pending.set(request, {
      reply: settle(message => (message.error ? fail(new Error(`${method}: ${JSON.stringify(message.error)}`)) : done(message.result))),
      fail: settle(fail),
    });
    child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id: request, method, params })}\n`);
  });
  try {
    const init = await rpc('initialize', { protocolVersion: MCP_PROTOCOL, capabilities: {}, clientInfo: { name: 'octocode-smoke', version: '1' } });
    child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' })}\n`);
    const list = await rpc('tools/list', {});
    const call = await rpc('tools/call', { name: 'localFetch', arguments: { queries: [QUERY] } });
    child.stdin.end();
    let timer;
    const exit = await Promise.race([exited, new Promise(done => { timer = setTimeout(done, SMOKE_TIMEOUT_MS, 'no exit after stdin closed'); })]);
    clearTimeout(timer);
    return { init, list, call, exit };
  } finally {
    if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
  }
}

async function check(checks, name, action) {
  const started = Date.now();
  try {
    checks.push({ name, ok: true, detail: await action(), ms: Date.now() - started });
  } catch (error) {
    checks.push({ name, ok: false, detail: error.message, ms: Date.now() - started });
  }
}

async function runtimeChecks(checks, { platform, version, binary }, container) {
  const host = getPlatformSuffix();
  if (!container && host !== platform) {
    checks.push({ name: 'host', ok: false, detail: `${platform} runs on its own platform (this host: ${host}), in a --container, or with --static` });
    return;
  }
  const context = smokeContext(binary, container);
  try {
    const fingerprint = expectedFingerprint();
    await check(checks, 'version', () => { verifyBinaryRuns(binary, version, args => context.run(args)); return `octocode ${version}`; });
    await check(checks, 'reportedFingerprint', () => {
      const reply = runJson(context, ['config', '--manage'], JSON.stringify({ expectedFingerprint: fingerprint, operation: 'inspect' }));
      if (reply.fingerprint !== fingerprint) throw new Error(`reports ${reply.fingerprint}, want ${fingerprint}`);
      return reply.fingerprint;
    });
    await check(checks, 'schema', () => checkCatalog(runJson(context, ['schema', '--json'])));
    await check(checks, 'cliLocalSearch', () => checkSearch(toolCall(context, 'localSearch', SEARCH)));
    await check(checks, 'cliLocalFetch', () => checkFetch(toolCall(context, 'localFetch', QUERY)));
    const session = mcpSession(context);
    await check(checks, 'mcpInitialize', async () => {
      const reported = (await session).init.serverInfo?.version;
      if (reported !== version) throw new Error(`serverInfo.version ${reported}, want ${version}`);
      return `serverInfo.version ${reported}`;
    });
    await check(checks, 'mcpToolsList', async () => checkToolNames((await session).list.tools.map(tool => tool.name)));
    await check(checks, 'mcpToolsCall', async () => {
      const { call } = await session;
      if (call.isError) throw new Error(JSON.stringify(call));
      return checkFetch(call.structuredContent ?? JSON.parse(call.content[0].text));
    });
    await check(checks, 'mcpExit', async () => {
      const { exit } = await session;
      if (exit !== 0) throw new Error(`exit ${exit}`);
      return 'exit 0 on stdin EOF';
    });
  } finally {
    context.cleanup();
  }
}

async function main() {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: { container: { type: 'string' }, static: { type: 'boolean', default: false }, json: { type: 'string' } },
  });
  if (positionals.length !== 1 || (values.static && values.container)) {
    console.error('Usage: smoke-binary.cjs <npm/<platform> dir> [--container IMAGE | --static] [--json FILE]');
    process.exit(2);
  }
  const dir = path.resolve(positionals[0]);
  const pkg = readPackage(dir);
  const checks = identityChecks(dir);
  if (!values.static && checks.every(entry => entry.ok)) await runtimeChecks(checks, pkg, values.container);
  const report = {
    platform: pkg.platform,
    version: pkg.version,
    binary: pkg.binary,
    host: `${process.platform}-${process.arch}`,
    container: values.container ?? null,
    mode: values.static ? 'static' : 'runtime',
    ok: checks.every(entry => entry.ok),
    checks,
  };
  for (const entry of checks) console.error(`${entry.ok ? 'ok  ' : 'FAIL'} ${entry.name}: ${entry.detail}`);
  const json = `${JSON.stringify(report, null, 2)}\n`;
  if (values.json) writeFileSync(values.json, json);
  else process.stdout.write(json);
  process.exitCode = report.ok ? 0 : 1;
}

if (require.main === module) {
  main().catch(error => {
    console.error(error);
    process.exitCode = 1;
  });
}
else module.exports = { FIXTURE, checkCatalog, checkFetch, checkSearch, checkToolNames };
