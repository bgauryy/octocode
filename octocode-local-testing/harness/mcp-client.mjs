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

export async function startServer({ env = {}, timeoutMs = Number(process.env.OCTOCODE_TEST_CALL_TIMEOUT_MS ?? 240_000) } = {}) {
  const server = spawn(process.execPath, [path.join(ROOT, 'packages/octocode-mcp/dist/index.js')], {
    cwd: ROOT,
    env: { ...process.env, ...env },
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
  server.on('exit', (code, signal) => { clearTimeout(deadline); process.removeListener('SIGINT', onSignal); process.removeListener('SIGTERM', onSignal); if (!closed && (code !== 0 || signal)) process.exitCode = 1; fail(new Error(`MCP server exited ${code}/${signal}`)); });
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
  init = await rpc('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'octocode-local-testing', version: '1' } });
  server.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
  if (init.error) throw new Error(init.error.message);
  const listing = await rpc('tools/list', {});
  if (listing.error || !Array.isArray(listing.result?.tools)) throw new Error('MCP tools/list failed');
  tools = listing.result.tools;
  } catch (error) { clearTimeout(deadline); terminate(); throw error; }
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
    const text = response.result?.content?.filter(c => c.type === 'text').map(c => c.text).join('') ?? JSON.stringify(response.error ?? '');
    const sc = expandShared(response.result?.structuredContent);
    const rows = sc?.results ?? [];
    const entry = {
      label, tool, args, ms: Date.now() - started, bytes: text.length, text, sc,
      isError: !!(response.error || response.result?.isError),
      rowErrors: collect(sc, r => r.status === 'error').length,
    };
    log.push(entry);
    return entry;
  }

  return {
    server, init: init.result, tools, log, call, raw, rpc,
    stderr: () => stderr,
    close: () => { closed = true; clearTimeout(deadline); fail(new Error('MCP client closed')); terminate(); const timer = setTimeout(() => { try { process.kill(-server.pid, 'SIGKILL'); } catch {} }, 3000); timer.unref(); },
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
    if (typeof node.tool === 'string' && node.query && typeof node.query === 'object') hints.push({ ...node, path: at });
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

export function sourcePath(entry, location, fallback) {
  const value = location?.uri ?? location?.path ?? rowData(entry)?.uri ?? rowData(entry)?.path ?? fallback;
  if (typeof value !== 'string') throw new Error('response location has no source URI/path');
  return value.startsWith('file:') ? fileURLToPath(value) : path.resolve(entry.sc?.base ?? ROOT, value);
}

/**
 * lspSearch location rows in one shape: `payload.locations`, or the compact
 * per-file `payload.byFile[].refs` rows ("line:col text", "start-end:col text")
 * long reference lists default to, expanded to {path, displayRange, content}.
 */
export function lspLocations(entry, index = 0) {
  const payload = rowData(entry, index)?.payload;
  if (Array.isArray(payload?.locations)) return payload.locations;
  return (payload?.byFile ?? []).flatMap(file => (file.refs ?? []).map(ref => {
    const [, start, end, column, text] = /^(\d+)(?:-(\d+))?:(\d+)(?: (.*))?$/.exec(ref) ?? [];
    return { path: file.path, displayRange: { startLine: Number(start), startCharacter: Number(column), endLine: Number(end ?? start) }, content: text ?? '' };
  }));
}

/**
 * astSearch symbols rows in one object shape. Outline strings
 * "<line>[-<endLine>] <kind> <name>[ +][ as a,b][ doc|doc@N][ from@N][ col N][ (in Parent@L)]",
 * indented two spaces per nesting level under the preceding row, parse to
 * {name, kind, line, endLine?, exported?, exportedAs?, docStartLine?,
 * startLine?, character?, parent?, parentLine?, parentKind?}; object rows (the earlier
 * shape) pass through. `extra` (e.g. a directory outline's file path) is
 * merged into each row.
 */
const OUTLINE_ROW = /^( *)(\d+)(?:-(\d+))? (\S+) (.+?)( \+)?(?: as (\S+))?( doc(?:@(\d+))?)?(?: from@(\d+))?(?: col (\d+))?(?: \(in (.+?)(?:@(\d+))?\))?$/;
export function outlineRows(rows = [], extra = {}) {
  const stack = [];
  return (rows ?? []).map(row => {
    if (typeof row !== 'string') return { ...extra, ...row };
    const m = OUTLINE_ROW.exec(row);
    if (!m) return { ...extra, raw: row };
    const [, indent, line, end, kind, name, exported, as, doc, docAt, from, col, parent, parentLine] = m;
    const out = { ...extra, name, kind, line: +line };
    if (end) out.endLine = +end;
    if (exported) out.exported = true;
    if (as) out.exportedAs = as.split(',');
    if (doc) out.docStartLine = docAt ? +docAt : out.line - 1;
    if (from) out.startLine = +from;
    if (col) out.character = +col;
    if (parent) { out.parent = parent; if (parentLine) out.parentLine = +parentLine; }
    const depth = indent.length / 2;
    const holder = depth > 0 ? stack[depth - 1] : undefined;
    if (holder) { out.parent = holder.name; out.parentLine = holder.line; out.parentKind = holder.kind; }
    stack.length = depth;
    stack[depth] = out;
    return out;
  });
}

/** Every declaration of an astSearch symbols row (single file or directory). */
export function declarations(entry, index = 0) {
  const data = rowData(entry, index);
  if (!data) return [];
  return [...outlineRows(data.declarations), ...(data.files ?? []).flatMap(file => outlineRows(file.declarations, { path: file.path }))];
}

/**
 * One astSearch match row as {line, endLine?, value, ...}: lean rows are
 * "<line>[-<endLine>]\t<value>"; captureText rows (and the earlier shape)
 * are objects and pass through.
 */
export function matchRow(row) {
  if (typeof row !== 'string') return row;
  const m = /^(\d+)(?:-(\d+))?\t([\s\S]*)$/.exec(row);
  if (!m) return { raw: row };
  const out = { line: +m[1], value: m[3] };
  if (m[2]) out.endLine = +m[2];
  return out;
}

/** astSearch match rows of a response as {path, line, endLine?, value, ...}. */
export function astMatchRows(entry, index = 0) {
  return (rowData(entry, index)?.files ?? []).flatMap(file => (file.matches ?? []).map(row => ({ path: file.path, ...matchRow(row) })));
}

/**
 * lspSearch callers as {name, kind, detail?, path, lines, declLine?}:
 * `payload.items` call-hierarchy edges, or the compact per-file rows
 * `payload.byFile[].calls` ("<line>:<col>[,…] in <kind> <name>[ (<detail>)] <start>-<end>")
 * direct callers default to.
 */
const CALL_ROW = /^(\d+(?::\d+)?(?:,\d+(?::\d+)?)*) in (\S+) (\S+)(?: \((.*)\))?(?: (\d+)(?:-(\d+))?)?$/;
export function lspCallers(entry, index = 0) {
  const payload = rowData(entry, index)?.payload;
  if (Array.isArray(payload?.items)) {
    return payload.items.filter(item => item.from).map(item => ({
      name: item.from.name, kind: item.from.kind, detail: item.from.detail, path: item.from.path ?? item.from.uri,
      lines: (item.fromRanges ?? []).map(range => range.startLine), declLine: item.from.displayRange?.startLine,
    }));
  }
  return (payload?.byFile ?? []).flatMap(file => (file.calls ?? []).map(row => {
    const m = CALL_ROW.exec(row);
    if (!m) return { path: file.path, raw: row, lines: [] };
    return { name: m[3], kind: m[2], detail: m[4], path: file.path, lines: m[1].split(',').map(site => +site.split(':')[0]), declLine: m[5] ? +m[5] : undefined };
  }));
}

/**
 * A file-read row's source view. Reads number source lines in `content`
 * (`<line>\t<text>`, omission markers unnumbered; docs/TOOL_DATA_CONTRACT.md
 * "Numbered source content") and then omit `sourceLineRanges`: `text` strips
 * the gutter and `ranges` comes from the numbers. Verbatim views pass through.
 */
export function sourceView(file) {
  const content = file?.content ?? '';
  const records = content.split('\n');
  const trailing = records.at(-1) === '' ? records.pop() : undefined;
  const marker = /^\.\.\. \[lines? \d+(?:-\d+)? omitted\] \.\.\.$/;
  const numbered = records.length > 0 && records.some(l => /^\d+\t/.test(l)) && records.every(l => /^\d+\t/.test(l) || marker.test(l));
  if (!numbered) return { text: content, ranges: file?.sourceLineRanges ?? [], numbered: false };
  const ranges = [];
  const text = records.map(l => {
    const m = /^(\d+)\t/.exec(l);
    if (!m) return l;
    const n = Number(m[1]);
    const last = ranges.at(-1);
    if (last && last.end + 1 === n) last.end = n; else ranges.push({ start: n, end: n });
    return l.slice(m[0].length);
  });
  if (trailing !== undefined) text.push('');
  return { text: text.join('\n'), ranges, numbered: true };
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
