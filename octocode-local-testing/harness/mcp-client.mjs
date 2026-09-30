// Shared MCP stdio client for the local-tool test suites.
// Spawns the repo's built octocode-mcp server and records every call.
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const TESTING = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const ROOT = path.resolve(TESTING, '..');
export const REPOS = path.join(TESTING, 'repos');
export const FIXTURES = path.join(TESTING, 'fixtures');
export const RESULTS = path.join(TESTING, 'results');

export async function startServer({ env = {}, timeoutMs = 240_000 } = {}) {
  const server = spawn(process.execPath, [path.join(ROOT, 'packages/octocode-mcp/dist/index.js')], {
    cwd: ROOT,
    env: { ...process.env, ...env },
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  let buffer = '';
  let stderr = '';
  let id = 0;
  const pending = new Map();
  server.stdout.on('data', chunk => {
    buffer += chunk;
    let index;
    while ((index = buffer.indexOf('\n')) >= 0) {
      const line = buffer.slice(0, index);
      buffer = buffer.slice(index + 1);
      if (line.trim()) {
        const message = JSON.parse(line);
        pending.get(message.id)?.(message);
      }
    }
  });
  server.stderr.on('data', chunk => { stderr += chunk; });
  const rpc = (method, params, timeout = timeoutMs) => new Promise((resolve, reject) => {
    const n = ++id;
    const timer = setTimeout(() => { pending.delete(n); reject(new Error(`timeout ${method} after ${timeout}ms`)); }, timeout);
    pending.set(n, message => { clearTimeout(timer); pending.delete(n); resolve(message); });
    server.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: n, method, params }) + '\n');
  });
  const init = await rpc('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'octocode-local-testing', version: '1' } });
  server.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
  const tools = (await rpc('tools/list', {})).result.tools;
  const log = [];

  /** Call a tool with one query (or an array) plus optional top-level args. */
  async function call(tool, queries, extra = {}, label = '') {
    const list = (Array.isArray(queries) ? queries : [queries]).map(q => ({ goal: 'octocode-local-testing regression check', reasoning: 'octocode-local-testing', ...q }));
    return raw(tool, { queries: list, ...extra }, label);
  }

  /** Call with arguments exactly as given (e.g. a pasted next.query). */
  async function raw(tool, args, label = '') {
    const started = Date.now();
    let response;
    try { response = await rpc('tools/call', { name: tool, arguments: args }); } catch (error) { response = { error: { message: error.message } }; }
    const text = response.result?.content?.map(c => c.text).join('') ?? JSON.stringify(response.error);
    const sc = expandShared(response.result?.structuredContent);
    const rows = sc?.results ?? [];
    const entry = {
      label, tool, args, ms: Date.now() - started, bytes: text.length, text, sc,
      isError: !!(response.error || response.result?.isError),
      rowErrors: rows.filter(r => r?.status === 'error').length,
    };
    log.push(entry);
    return entry;
  }

  return {
    server, init: init.result, tools, log, call, raw, rpc,
    stderr: () => stderr,
    close: () => server.kill(),
  };
}

/**
 * Apply the response's `shared` defaults (TOOL_DATA_CONTRACT "Paths, shared
 * fields, and anchors"): identical scalars hoisted out of object entries in
 * arrays directly inside row `data` are restored on those entries only.
 */
export function expandShared(sc) {
  if (!sc?.shared || typeof sc.shared !== 'object') return sc;
  const copy = structuredClone(sc);
  for (const row of copy.results ?? []) {
    const data = row?.data;
    if (!data || typeof data !== 'object') continue;
    for (const value of Object.values(data)) {
      if (!Array.isArray(value)) continue;
      for (const item of value) {
        if (!item || typeof item !== 'object' || Array.isArray(item)) continue;
        for (const [key, def] of Object.entries(copy.shared)) if (!(key in item)) item[key] = def;
      }
    }
  }
  return copy;
}

/** Every `{tool, query}` continuation anywhere in a structured result. */
export function nextHints(value, pathLabel = '') {
  const hints = [];
  const walk = (node, at) => {
    if (!node || typeof node !== 'object') return;
    if (typeof node.tool === 'string' && node.query && typeof node.query === 'object') hints.push({ path: at, tool: node.tool, query: node.query });
    for (const [key, child] of Object.entries(node)) walk(child, `${at}.${key}`);
  };
  walk(value, pathLabel);
  return hints;
}

/** Collect objects matching a predicate anywhere in a value. */
export function collect(value, predicate, out = []) {
  if (value && typeof value === 'object') {
    if (predicate(value)) out.push(value);
    for (const child of Object.values(value)) collect(child, predicate, out);
  }
  return out;
}

/**
 * Expand ghGetHistoryItem's compact changed-file inventory — rows
 * "M +3 -1 [!reason ]path[ <- full/old/path]" or {"dir/": [rows named in dir]} —
 * into {path, status, additions, deletions, patchUnavailable?, previousPath?}.
 * Object rows (patch responses) pass through unchanged.
 */
const INVENTORY_STATUS = { A: 'added', D: 'removed', M: 'modified', R: 'renamed', C: 'copied', T: 'changed', U: 'unchanged' };
function inventoryRow(row, dir = '') {
  const m = /^(\S+) \+(\d+) -(\d+)(?: !(\w+))? (.+?)(?: <- (.+))?$/.exec(row);
  if (!m) return { path: null, raw: row };
  const file = { path: dir + m[5], status: INVENTORY_STATUS[m[1]] ?? m[1], additions: +m[2], deletions: +m[3] };
  if (m[4]) file.patchUnavailable = m[4];
  if (m[6]) file.previousPath = m[6];
  return file;
}
export function inventoryRows(items = []) {
  return (items ?? []).flatMap(item => {
    if (typeof item === 'string') return [inventoryRow(item)];
    if (item && typeof item === 'object' && !('path' in item)) return Object.entries(item).flatMap(([dir, rows]) => rows.map(row => inventoryRow(row, dir)));
    return [item];
  });
}

export function rowData(entry, index = 0) {
  const row = entry.sc?.results?.[index];
  return row?.data ?? row;
}

/** Tiny check recorder: `check(name, ok, detail)`; `summary()` prints and returns failures. */
export function checks(suite) {
  const results = [];
  const check = (name, ok, detail = '') => {
    results.push({ suite, name, ok: !!ok, detail: String(detail).slice(0, 220) });
    console.log(`${ok ? 'PASS' : 'FAIL'} [${suite}] ${name}${detail ? ` — ${String(detail).slice(0, 160)}` : ''}`);
    return !!ok;
  };
  const summary = () => {
    const failed = results.filter(r => !r.ok);
    console.log(`\n[${suite}] ${results.length - failed.length}/${results.length} passed`);
    return { results, failed };
  };
  return { check, summary, results };
}

export function writeResults(name, payload) {
  fs.mkdirSync(RESULTS, { recursive: true });
  const file = path.join(RESULTS, `${name}.json`);
  fs.writeFileSync(file, JSON.stringify(payload, null, 2));
  return file;
}
