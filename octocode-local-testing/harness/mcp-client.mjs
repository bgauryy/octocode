// Shared MCP stdio client for the local-tool test suites.
// Spawns the repo's built octocode-mcp server and records every call.
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const TESTING = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const ROOT = path.resolve(TESTING, '..');
export const REPOS = path.join(TESTING, 'repos');
export const FIXTURES = path.join(TESTING, 'fixtures');
export const RESULTS = path.join(TESTING, 'results');

export async function startServer({ env = {}, cwd = ROOT, timeoutMs = Number(process.env.OCTOCODE_TEST_CALL_TIMEOUT_MS ?? 240_000) } = {}) {
  const server = spawn(process.execPath, [path.join(ROOT, 'packages/octocode-mcp/dist/index.js')], {
    cwd,
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

  /** Call a tool with one query (or an array) plus optional top-level args. Briefs are optional, so none is added. */
  async function call(tool, queries, extra = {}, label = '') {
    return raw(tool, { queries: Array.isArray(queries) ? queries : [queries], ...extra }, label);
  }

  /**
   * Call with arguments exactly as given (e.g. a pasted next.* or hints.* query).
   * `bytes(structuredContent, text)` overrides the UTF-8 text measure; `keepRaw`
   * adds the unexpanded `raw` structuredContent and the `transport` error.
   */
  async function raw(tool, args, label = '', { bytes, keepRaw = false } = {}) {
    const started = Date.now();
    let response;
    try { response = await rpc('tools/call', { name: tool, arguments: args }); } catch (error) { response = { error: { message: error.message } }; }
    const text = response.result?.content?.filter(c => c.type === 'text').map(c => c.text).join('') ?? JSON.stringify(response.error ?? '');
    const rawSc = response.result?.structuredContent;
    const sc = expandShared(rawSc);
    const entry = {
      label, tool, args, ms: Date.now() - started, bytes: bytes ? bytes(rawSc, text) : Buffer.byteLength(text, 'utf8'), text, sc,
      isError: !!(response.error || response.result?.isError),
      rowErrors: collect(sc, r => r.status === 'error').length,
      ...(keepRaw && { raw: rawSc, transport: response.error?.message }),
    };
    log.push(entry);
    return entry;
  }

  /** Run a `next.*`/`hints.*` call verbatim: its query is the complete input. */
  async function follow(hint, label = '') {
    return raw(hint.tool, hint.query, label);
  }

  return {
    server, init: init.result, tools, log, call, raw, follow, rpc,
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

/**
 * Every executable continuation (`next.*` pages and `hints.*` leads, with its
 * path) plus clasify's `next.clasify` self-continuation, whose query is a
 * whole `{queries:[...]}` input.
 */
export function nextHints(value, pathLabel = '') {
  const hints = [];
  const walk = (node, at) => {
    if (!node || typeof node !== 'object') return;
    if (typeof node.tool === 'string' && node.query && typeof node.query === 'object') hints.push({ ...node, path: at });
    else if (/\.(?:hints|next)\.clasify$/.test(at) && Array.isArray(node.queries)) hints.push({ tool: 'clasify', query: node, path: at });
    for (const [key, child] of Object.entries(node)) walk(child, `${at}.${key}`);
  };
  walk(value, pathLabel);
  return hints;
}

/** A ghGetHistoryItem patch view without its gutter (`N\t` on every diff line, a bare tab on `\ No newline` markers): the raw patch. */
export function rawPatch(view) {
  return (view ?? '').split(/(?<=\n)/).map(line => line.startsWith('@@') ? line : line.replace(/^\d*\t/, '')).join('');
}

/**
 * Whether a patch view numbers each line on its sign's side (native `number_patch`,
 * HI3 both-side numbering): kept and added lines carry their new-file line number,
 * removed lines their old-file number (both counted from the `@@ -a,b +c,d @@`
 * header), and a `\ No newline` marker a bare tab. With `oldLines`/`newLines`
 * (the file at the parent / at the change), each numbered line must also equal
 * that side's file line.
 */
export function patchNumbersOk(view, { oldLines, newLines } = {}) {
  return patchNumbersIssue(view, { oldLines, newLines }) === null;
}

/** The first line of a patch view whose gutter number is wrong (null when all are right). */
export function patchNumbersIssue(view, { oldLines, newLines } = {}) {
  let oldNext = null, newNext = null;
  const same = (lines, number, text) => !lines || (lines[number - 1] ?? '').replace(/\r$/, '') === text.replace(/\r$/, '');
  for (const line of (view ?? '').split('\n')) {
    if (line === '') continue;
    const header = line.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
    if (header) { oldNext = +header[1]; newNext = +header[2]; continue; }
    const m = line.match(/^(\d*)\t([\s\S])([\s\S]*)$/);
    if (!m || newNext === null) return `unnumbered: ${line.slice(0, 80)}`;
    const [, gutter, sign, text] = m;
    if (sign === '\\') { if (gutter !== '') return `numbered marker: ${line.slice(0, 80)}`; continue; }
    if (sign === '-') {
      if (+gutter !== oldNext || !same(oldLines, oldNext, text)) return `old ${oldNext}: ${line.slice(0, 80)}`;
      oldNext++;
      continue;
    }
    if (+gutter !== newNext || !same(newLines, newNext, text)) return `new ${newNext}: ${line.slice(0, 80)}`;
    if (sign !== '+') oldNext++;
    newNext++;
  }
  return newNext === null ? 'no hunk header' : null;
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
 * An `omitted` file (GitHub sent neither patch nor counts) prints its letter
 * only, "M !omitted path", and expands without counts; a row
 * missing its counts for any other reason does not parse.
 * Object rows (patch responses) pass through unchanged.
 */
const INVENTORY_STATUS = { A: 'added', D: 'removed', M: 'modified', R: 'renamed', C: 'copied', T: 'changed', U: 'unchanged' };
function inventoryRow(row, dir = '') {
  const m = /^(\S+)(?: \+(\d+) -(\d+))?(?: !(\w+))? (.+?)(?: <- (.+))?$/.exec(row);
  if (!m || ((m[2] === undefined) !== (m[4] === 'omitted'))) return { path: null, raw: row };
  const file = { path: dir + m[5], status: INVENTORY_STATUS[m[1]] ?? m[1] };
  if (m[2] !== undefined) Object.assign(file, { additions: +m[2], deletions: +m[3] });
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

/**
 * structureSearch rows (tree and files) in one object shape. Every row is a
 * {dir, files: [...]} group whose `dir` is workspace-relative (like a row
 * path; "." is the workspace root). Entries ("<name>[/][ (<fields>)]";
 * fields: size in bytes, "symlink", "lineCount=N", "modifiedMs=N"; "/" marks
 * a directory) expand in order to {path: dir/name, size?, type?, lineCount?,
 * modifiedMs?}, `path` resolving against the response `root`. Object rows
 * pass through.
 */
export function structureFiles(items = []) {
  const join = (dir, name) => (dir === '.' || dir === '' ? name : `${dir}/${name}`);
  const expand = (dir, text) => {
    const m = /^(.*) \(([^()]*)\)$/.exec(text);
    let name = m ? m[1] : text;
    const row = {};
    if (name.endsWith('/')) { name = name.slice(0, -1); row.type = 'directory'; }
    row.path = name === '.' ? dir : join(dir, name);
    for (const field of m ? m[2].split(', ') : []) {
      if (field === 'symlink') row.type = 'symlink';
      else if (field.startsWith('lineCount=')) row.lineCount = Number(field.slice(10));
      else if (field.startsWith('modifiedMs=')) row.modifiedMs = Number(field.slice(11));
      else row.size = Number(field);
    }
    return row;
  };
  return (items ?? []).flatMap(item => {
    if (!item || typeof item !== 'object' || typeof item.dir !== 'string' || !Array.isArray(item.files)) return [item];
    return item.files.map(text => expand(item.dir, text));
  });
}

/** A response path resolved against that response's `root`. */
export const rootPath = (entry, p) => path.resolve(entry.sc?.root ?? '/', p);

export function sourcePath(entry, location, fallback) {
  const value = location?.path ?? rowData(entry)?.path ?? fallback;
  if (typeof value !== 'string') throw new Error('response location has no source path');
  return value.startsWith('file:') ? fileURLToPath(value) : path.resolve(entry.sc?.root ?? ROOT, value);
}

/**
 * lspSearch location rows in one shape: flat `payload.matches` location
 * objects, or the compact per-file `payload.files[].matches` reference rows
 * {line, column, endLine?, value?} long reference lists default to,
 * expanded to {path, displayRange, content}.
 */
export function lspLocations(entry, index = 0) {
  const payload = rowData(entry, index)?.payload;
  if (Array.isArray(payload?.matches)) return payload.matches;
  return (payload?.files ?? []).flatMap(file => (file.matches ?? []).filter(ref => ref && typeof ref.line === 'number' && !ref.sites).map(ref => ({
    path: file.path,
    displayRange: { startLine: ref.line, startCharacter: ref.column, endLine: ref.endLine ?? ref.line },
    content: ref.value ?? '',
  })));
}

/**
 * One symbols outline entry string (P1, a declaration without members):
 * `<symbolName> (<line>[-<endLine>][, <kind>][, doc <docStartLine>][, exported][, <key>=<value>]…)`.
 * The last ` (` opens the fields; a `<key>=<value>` value is bare words or
 * JSON. Returns the declaration's fields ({symbolName, line, endLine?, kind?,
 * docStartLine?, exported?, exportedAs?, startLine?, column?, parent?,
 * parentLine?}) or null when `text` is not an entry.
 */
export function parseOutlineEntry(text) {
  if (typeof text !== 'string') return null;
  const open = text.lastIndexOf(' (');
  if (open <= 0 || !text.endsWith(')')) return null;
  const fields = [];
  let cur = '', quoted = false, escaped = false;
  const inner = text.slice(open + 2, -1);
  for (let i = 0; i < inner.length; i++) {
    const ch = inner[i];
    if (quoted) {
      if (escaped) escaped = false; else if (ch === '\\') escaped = true; else if (ch === '"') quoted = false;
    } else if (ch === '"') quoted = true;
    else if (ch === ',' && inner[i + 1] === ' ') { fields.push(cur); cur = ''; i++; continue; }
    cur += ch;
  }
  fields.push(cur);
  const range = /^(\d+)(?:-(\d+))?$/.exec(fields[0]);
  if (!range) return null;
  const decl = { symbolName: text.slice(0, open), line: +range[1] };
  if (range[2]) decl.endLine = +range[2];
  for (const field of fields.slice(1)) {
    if (field === 'exported') decl.exported = true;
    else if (/^doc \d+$/.test(field)) decl.docStartLine = +field.slice(4);
    else if (/^[A-Za-z_]+=/.test(field)) {
      const eq = field.indexOf('=');
      const raw = field.slice(eq + 1);
      let value = raw;
      if (/^-?\d+$/.test(raw) || /^["[{]/.test(raw) || raw === 'true' || raw === 'false') {
        try { value = JSON.parse(raw); } catch { return null; }
      }
      decl[field.slice(0, eq)] = value;
    } else if (field && decl.kind === undefined) decl.kind = field;
    else return null;
  }
  return decl;
}

/**
 * astSearch / lspSearch outline rows as harness declarations: an entry string
 * (a declaration without members, see parseOutlineEntry) or a container
 * object {symbolName, kind, line, endLine?, docStartLine?, exported?, shared?,
 * members}. `name` = symbolName, `character` = column. `extra` (e.g. a
 * directory outline's file path) is merged in.
 */
export function outlineRows(rows = [], extra = {}, parent = {}) {
  // Containers nest members under them (`members`, with fields every member
  // shares in the container's `shared`); flatten them in source order with
  // `parent`/`parentLine` restored.
  return (rows ?? []).map(row => (typeof row === 'string' ? parseOutlineEntry(row) : row)).filter(row => row && typeof row === 'object').flatMap(row => {
    const { symbolName, column, members, shared, ...rest } = row;
    const decl = { ...extra, ...parent, ...rest, name: symbolName };
    if (column !== undefined) decl.character = column;
    const nested = (members ?? []).map(member => (typeof member === 'string' ? parseOutlineEntry(member) : member)).filter(Boolean).map(member => ({ ...shared, ...member }));
    return [decl, ...outlineRows(nested, extra, { parent: symbolName, parentLine: row.line })];
  });
}

/** Every declaration of an astSearch symbols row (single file or directory). */
export function declarations(entry, index = 0) {
  const data = rowData(entry, index);
  if (!data) return [];
  return [...outlineRows(data.symbols), ...(data.files ?? []).flatMap(file => outlineRows(file.symbols, { path: file.path }))];
}

/** One astSearch match row {line, column, endLine?, value, ...}. */
export function matchRow(row) {
  return row && typeof row === 'object' ? row : { raw: row };
}

/** astSearch match rows of a response as {path, line, endLine?, value, ...}. */
export function astMatchRows(entry, index = 0) {
  return (rowData(entry, index)?.files ?? []).flatMap(file => (file.matches ?? []).map(row => ({ path: file.path, ...matchRow(row) })));
}

/**
 * lspSearch callers as {name, kind, detail?, path, lines, declLine?, via?}:
 * flat `payload.matches` call-hierarchy edges, or the compact per-file call
 * rows {symbolName, kind, line, endLine?, sites, detail?, via?} of a
 * `callers` payload (a `callees` payload lists no callers).
 */
export function lspCallers(entry, index = 0) {
  const payload = rowData(entry, index)?.payload;
  if (Array.isArray(payload?.matches)) {
    return payload.matches.filter(item => item.from).map(item => ({
      name: item.from.name, kind: item.from.kind, detail: item.from.detail, path: item.from.path,
      lines: (item.fromRanges ?? []).map(range => range.startLine), declLine: item.from.displayRange?.startLine,
    }));
  }
  if (payload?.kind === 'callees') return [];
  return (payload?.files ?? []).flatMap(file => (file.matches ?? []).filter(row => row && Array.isArray(row.sites)).map(row => ({
    name: row.symbolName, kind: row.kind, detail: row.detail, path: file.path,
    lines: row.sites.map(site => site.line), declLine: row.line,
    via: row.via ? { name: row.via.symbolName, line: row.via.line, ...(row.via.path ? { path: row.via.path } : {}) } : undefined,
  })));
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
  // A gap marker, or one gap-run marker for single lines (ghGetFileContent).
  const marker = /^\.\.\. \[(?:lines? \d+(?:-\d+)?|\d+ gaps in lines \d+-\d+) (?:omitted|not requested)\] \.\.\.$/;
  const numbered = records.length > 0 && records.some(l => /^\d+\t/.test(l)) && records.every(l => /^\d+\t/.test(l) || marker.test(l));
  // Verbatim views keep `sourceLineRanges` as `{line, endLine}` (X1); the
  // view's ranges are `{start, end}` in both branches.
  if (!numbered) return { text: content, ranges: (file?.sourceLineRanges ?? []).map(r => ({ start: r.line, end: r.endLine })), numbered: false };
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

/** The first `next.*`/`hints.*` continuation whose path ends with `.<key>` (e.g. `continue`, `hints.read`). */
export function findHint(sc, key) {
  return nextHints(sc).find(h => h.path.endsWith(`.${key}`));
}

/**
 * Follow `next.<key>` from `first` with `client.follow` until it disappears or
 * `max` pages are collected; returns the page entries. A page with a call error
 * (or a row error, unless `rowErrors: false`) ends the walk; `keepError` keeps
 * that page as the last entry. `label` names each followed page "<label> page N".
 */
export async function walk(client, first, key, max, { keepError = false, rowErrors = true, label } = {}) {
  const pages = [first];
  let current = first;
  while (pages.length < max) {
    const h = findHint(current.sc, key);
    if (!h) break;
    current = await client.follow(h, ...(label === undefined ? [] : [`${label} page ${pages.length + 1}`]));
    if (current.isError || (rowErrors && current.rowErrors)) { if (keepError) pages.push(current); break; }
    pages.push(current);
  }
  return pages;
}

/**
 * Run the built CLI (`packages/octocode/out/octocode.js <tool> <json>`) as a
 * shell would: `env` is merged over process.env; returns the spawnSync result
 * fields plus `ms`. Each suite keeps its own byte measure.
 */
export function cli(tool, input, { cwd = ROOT, env = {}, timeout, maxBuffer = 64 << 20 } = {}) {
  const started = performance.now();
  const run = spawnSync(process.execPath, [path.join(ROOT, 'packages/octocode/out/octocode.js'), tool, JSON.stringify(input)], {
    cwd, encoding: 'utf8', maxBuffer, env: { ...process.env, ...env }, ...(timeout === undefined ? {} : { timeout }),
  });
  return { status: run.status, stdout: run.stdout ?? '', stderr: run.stderr ?? '', error: run.error, ms: performance.now() - started };
}

/**
 * A CLI-only tool (contract `cliOnly`, e.g. astTopology) called through the
 * built CLI, returned in the `call`/`follow` entry shape. `args` is the
 * complete input, so a `next.*` query runs verbatim. Exit 6 is a partial row.
 */
export function cliEntry(tool, args, { env = {}, label = '', ...options } = {}) {
  const run = cli(tool, args, { env: { OCTOCODE_BETA: 'true', ...env }, ...options });
  let parsed;
  try { parsed = JSON.parse(run.stdout); } catch {}
  const sc = expandShared(parsed);
  return {
    label, tool, args, ms: Math.round(run.ms), bytes: Buffer.byteLength(run.stdout, 'utf8'), text: run.stdout + run.stderr, sc,
    isError: !!run.error || !parsed || (run.status !== 0 && run.status !== 6),
    rowErrors: collect(sc, r => r.status === 'error').length,
  };
}

/** `walk`/`follow` over the CLI, for continuations of a CLI-only tool. */
export const cliClient = { follow: async (hint, label = '') => cliEntry(hint.tool, hint.query, { label }) };
