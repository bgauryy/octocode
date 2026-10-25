// Shared MCP stdio client for the local-tool test suites.
// Spawns the repo's built octocode-mcp server and records every call.
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { CLI, RESULTS, ROOT } from './paths.mjs';
import { collect } from './responses.mjs';

/** OCTOCODE_LIVE=1 enables suites and sections that reach GitHub, package registries or the paid classifier. */
export const LIVE = process.env.OCTOCODE_LIVE === '1';

/** Start the MCP server at `entry`; `env` is merged over process.env, or with `replaceEnv` is the whole environment. */
export async function startServer({ env = {}, replaceEnv = false, cwd = ROOT, entry = path.join(ROOT, 'packages/octocode-mcp/dist/index.js'), timeoutMs = Number(process.env.OCTOCODE_TEST_CALL_TIMEOUT_MS ?? 240_000) } = {}) {
  const server = spawn(process.execPath, [entry], {
    cwd,
    env: replaceEnv ? env : { ...process.env, ...env },
    stdio: ['pipe', 'pipe', 'pipe'], detached: process.platform !== 'win32',
  });
  let buffer = '';
  let stderr = '';
  let id = 0;
  const pending = new Map();
  let closed = false;
  const terminate = () => { try { process.kill(-server.pid, 'SIGTERM'); } catch { server.kill(); } };
  const forceTerminate = () => { terminate(); setTimeout(() => { try { process.kill(-server.pid, 'SIGKILL'); } catch { server.kill('SIGKILL'); } }, 500); };
  const fail = error => { closed = true; for (const entry of pending.values()) entry.reject(error); pending.clear(); };
  const onSignal = () => { process.exitCode = 1; fail(new Error('suite interrupted')); forceTerminate(); };
  process.once('SIGINT', onSignal); process.once('SIGTERM', onSignal);
  const deadline = setTimeout(() => { process.exitCode = 1; fail(new Error('suite MCP deadline exceeded')); forceTerminate(); }, Number(process.env.OCTOCODE_TEST_SUITE_TIMEOUT_MS ?? 1800000));
  deadline.unref();
  server.on('error', error => { clearTimeout(deadline); fail(error); });
  server.on('exit', (code, signal) => { clearTimeout(deadline); process.removeListener('SIGINT', onSignal); process.removeListener('SIGTERM', onSignal); if (!closed && (code !== 0 || signal)) process.exitCode = 1; fail(new Error(`MCP server exited ${code}/${signal}: ${stderr}`)); });
  server.stdin.on('error', fail);
  server.stdout.on('data', chunk => {
    buffer += chunk;
    let index;
    while ((index = buffer.indexOf('\n')) >= 0) {
      const line = buffer.slice(0, index);
      buffer = buffer.slice(index + 1);
      if (line.trim()) {
        let message; try { message = JSON.parse(line); } catch { fail(new Error('invalid MCP JSON frame')); terminate(); return; }
        pending.get(message.id)?.resolve(message);
      }
    }
  });
  server.stderr.on('data', chunk => { stderr += chunk; });
  const rpc = (method, params, timeout = timeoutMs) => new Promise((resolve, reject) => {
    if (closed) { reject(new Error('MCP server unavailable')); return; }
    const n = ++id;
    const timer = setTimeout(() => { pending.delete(n); process.exitCode = 1; reject(new Error(`timeout ${method} after ${timeout}ms`)); }, timeout);
    pending.set(n, { resolve: message => { clearTimeout(timer); pending.delete(n); resolve(message); }, reject: error => { clearTimeout(timer); pending.delete(n); reject(error); } });
    server.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: n, method, params }) + '\n');
  });
  let init, tools;
  try {
  init = await rpc('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'packages/octocode-benchmark/local/e2e', version: '1' } });
  server.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
  if (init.error) throw new Error(init.error.message);
  const listing = await rpc('tools/list', {});
  if (listing.error || !Array.isArray(listing.result?.tools)) throw new Error('MCP tools/list failed');
  tools = listing.result.tools;
  } catch (error) { clearTimeout(deadline); terminate(); throw error; }
  const log = [];

  /**
   * Call a tool with one query (or an array) plus optional top-level args. No fields are invented.
   * A complete `{queries:[...], responseChars, ...}` envelope runs as given: wrapping it again would nest one
   * envelope in a row, which the runtime rejects.
   */
  async function call(tool, queries, extra = {}, label = '') {
    if (queries && !Array.isArray(queries) && typeof queries === 'object' && (Array.isArray(queries.queries) || typeof queries.cursor === 'string')) {
      return raw(tool, { ...queries, ...extra }, label);
    }
    return raw(tool, { queries: Array.isArray(queries) ? queries : [queries], ...extra }, label);
  }

  /**
   * Call with arguments exactly as given (e.g. a typed continuation or related input).
   * `bytes(structuredContent, text)` overrides the UTF-8 text measure; `keepRaw`
   * adds the unexpanded `raw` structuredContent and the `transport` error.
   */
  async function raw(tool, args, label = '', { bytes, keepRaw = false } = {}) {
    const started = Date.now();
    let response;
    try { response = await rpc('tools/call', { name: tool, arguments: args }); } catch (error) { response = { error: { message: error.message } }; }
    const text = response.result?.content?.filter(c => c.type === 'text').map(c => c.text).join('') ?? JSON.stringify(response.error ?? '');
    const rawSc = response.result?.structuredContent;
    const sc = rawSc;
    const entry = {
      label, tool, args, ms: Date.now() - started, bytes: bytes ? bytes(rawSc, text) : Buffer.byteLength(text, 'utf8'),
      responseBytes: Buffer.byteLength(JSON.stringify(response.result ?? response.error ?? null), 'utf8'), text, sc,
      isError: !!(response.error || response.result?.isError),
      rowErrors: collect(sc, r => r.status === 'error').length,
      ...(keepRaw && { raw: rawSc, transport: response.error?.message }),
    };
    log.push(entry);
    return entry;
  }

  /** Run a typed call verbatim: input is the complete request. */
  async function follow(hint, label = '') {
    return raw(hint.tool, hint.input, label);
  }

  return {
    server, init: init.result, tools, log, call, raw, follow, rpc,
    stderr: () => stderr,
    close: () => { closed = true; clearTimeout(deadline); fail(new Error('MCP client closed')); terminate(); const timer = setTimeout(() => { try { process.kill(-server.pid, 'SIGKILL'); } catch {} }, 3000); timer.unref(); },
  };
}

/** Tiny check recorder: `check(name, ok, detail)`; `summary()` prints and returns failures. */
export function checks(suite) {
  const results = [];
  const check = (name, ok, detail = '') => {
    results.push({ suite, name, ok: !!ok, detail: String(detail).slice(0, 220) });
    console.log(`${ok ? 'PASS' : 'FAIL'} [${suite}] ${name}${detail ? ` — ${String(detail).slice(0, 160)}` : ''}`);
    return !!ok;
  };
  /** Record a check that cannot run in this environment (never counted as passed or failed). */
  const skip = (name, reason) => {
    results.push({ suite, name, ok: null, skipped: String(reason) });
    console.log(`SKIP [${suite}] ${name} — ${reason}`);
  };
  const summary = () => {
    const ran = results.filter(r => r.ok !== null);
    const failed = ran.filter(r => !r.ok);
    const skipped = results.length - ran.length;
    console.log(`\n[${suite}] ${ran.length - failed.length}/${ran.length} passed${skipped ? `, ${skipped} skipped` : ''}`);
    return { results, failed };
  };
  return { check, skip, summary, results };
}

/** Why clasify checks cannot run on this server (null when the tool is registered). */
export function clasifyUnavailable(client) {
  return client.tools.some(tool => tool.name === 'clasify') ? null : 'clasify is not registered on this server (no classification provider key configured)';
}

export function writeResults(name, payload) {
  fs.mkdirSync(RESULTS, { recursive: true });
  const file = path.join(RESULTS, `${name}.json`);
  fs.writeFileSync(file, JSON.stringify(payload, null, 2));
  return file;
}

/**
 * Run the built CLI (`CLI <tool> <json> --json`) as a
 * shell would: `env` is merged over process.env; returns the spawnSync result
 * fields plus `ms`. Each suite keeps its own byte measure.
 */
export function runCli(tool, input, { cwd = ROOT, env = {}, timeout, maxBuffer = 64 << 20 } = {}) {
  const started = performance.now();
  const run = spawnSync(process.execPath, [CLI, tool, JSON.stringify(input), '--json'], {
    cwd, encoding: 'utf8', maxBuffer, env: { ...process.env, ...env }, ...(timeout === undefined ? {} : { timeout }),
  });
  return { status: run.status, stdout: run.stdout ?? '', stderr: run.stderr ?? '', error: run.error, ms: performance.now() - started };
}

/**
 * Run a CLI (`bin`, default the built one) with raw `argv`, asynchronously so
 * calls run in parallel: `env` is the whole environment and a call past
 * `timeoutMs` loses its whole process group/tree, including inherited pipes.
 * Resolves {status, stdout, stderr, ms}.
 */
export function spawnCli(argv, { bin = CLI, cwd = ROOT, env = process.env, timeoutMs = 120_000 } = {}) {
  return new Promise((resolve, reject) => {
    const started = performance.now();
    const child = spawn(process.execPath, [bin, ...argv], { cwd, env, stdio: ['ignore', 'pipe', 'pipe'], detached: process.platform !== 'win32' });
    let stdout = '', stderr = '';
    child.stdout.on('data', chunk => { stdout += chunk; });
    child.stderr.on('data', chunk => { stderr += chunk; });
    const timer = setTimeout(() => {
      if (!child.pid) return;
      if (process.platform === 'win32') {
        const taskkill = path.win32.join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'taskkill.exe');
        spawnSync(taskkill, ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true });
      } else {
        try { process.kill(-child.pid, 'SIGKILL'); } catch { child.kill('SIGKILL'); }
      }
    }, timeoutMs);
    child.on('error', error => { clearTimeout(timer); reject(error); });
    child.on('close', status => { clearTimeout(timer); resolve({ status, stdout, stderr, ms: performance.now() - started }); });
  });
}

/**
 * A CLI-only tool (contract `cliOnly`, e.g. ghCloneRepo) called through the
 * built CLI, returned in the `call`/`follow` entry shape. `args` is the
 * complete input, so a `next.*` query runs verbatim. A next page is still exit 0.
 */
export function cliEntry(tool, args, { env = {}, label = '', ...options } = {}) {
  const run = runCli(tool, args, { env: { OCTOCODE_BETA: 'true', ...env }, ...options });
  let parsed;
  try { parsed = JSON.parse(run.stdout); } catch {}
  const sc = parsed;
  // Exit 1 is the empty class (every row `status:"empty"`, as a grep with no
  // match, or `"outOfRange"`, a page past the last): a complete answer, not a
  // failed call.
  const rows = parsed?.results ?? [];
  const emptyAnswer = run.status === 1 && rows.length > 0 && rows.every(r => r.status === 'empty' || r.status === 'outOfRange');
  return {
    label, tool, args, ms: Math.round(run.ms), bytes: Buffer.byteLength(run.stdout, 'utf8'), text: run.stdout + run.stderr, sc,
    isError: !!run.error || !parsed || (run.status !== 0 && !emptyAnswer),
    rowErrors: collect(sc, r => r.status === 'error').length,
  };
}
