#!/usr/bin/env node
// Captures the MCP surface a client receives, sizes it, and diffs its non-prose structure.

import { spawn } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { fail, errorMessage } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';

const HELP = `Capture, size and structurally diff the MCP surface a client receives.

Usage:
  node scripts/mcp-surface.mjs capture --cmd '<server command>' --out <new-file> [--cwd <dir>] [--protocol <version>] [--timeout <ms>]
  node scripts/mcp-surface.mjs size <capture.json>
  node scripts/mcp-surface.mjs diff <before.json> <after.json>

capture  Starts the stdio server with sh -c, sends initialize (protocol default 2025-11-25),
         notifications/initialized, then tools/list, following every nextCursor until none.
         Writes {command, protocolVersion, capturedAt, initialize, toolsList:[{cursor, result}]}
         to --out (refuses an existing file) and prints {ok, out, protocolVersion, serverInfo,
         pages, tools}. Answers server pings; refuses other server requests. Stops the server
         after the capture. --timeout bounds the whole capture; omit it to wait for the server.
size     Prints UTF-8 bytes in served order: per tool, bytes (compact JSON of the definition),
         descriptionBytes (tool description text), inputSchemaBytes and outputSchemaBytes (compact
         JSON), schemaProseBytes (description/title text inside schemas); totals add them up, plus
         instructionsBytes (initialize.instructions text) and surfaceBytes = toolBytes + instructionsBytes.
         Bytes are not tokens.
diff     Compares tools by name, ignoring prose (description, title, $comment, markdownDescription,
         annotations.title). Reports each change {tool, path, kind, change, before, after}; kind is
         name, required, default, bound, enum, type, composition, property, shape, annotation or other.
         required is compared as a set. Prose changes are listed under prose and do not count.

Output: JSON on stdout. Errors: {ok:false,error:{code,message}}.
Exit codes: 0 done (diff: no structural change); 1 capture failed or diff found a structural change;
2 invalid input (arguments, unreadable or malformed capture, duplicate tool names in diff).`;

const DEFAULT_PROTOCOL = '2025-11-25';
const PROSE = new Set(['description', 'title', '$comment', 'markdownDescription']);
const SCHEMA_MAPS = new Set(['properties', 'patternProperties', '$defs', 'definitions', 'dependentSchemas']);
const SCHEMA_ONE = new Set(['items', 'additionalProperties', 'additionalItems', 'not', 'if', 'then', 'else', 'contains',
  'propertyNames', 'unevaluatedProperties', 'unevaluatedItems', 'contentSchema']);
const SCHEMA_LIST = new Set(['anyOf', 'oneOf', 'allOf', 'prefixItems']);
const KIND = {
  required: 'required', dependentRequired: 'required', default: 'default', enum: 'enum', const: 'enum', type: 'type',
  ...Object.fromEntries(['minimum', 'maximum', 'exclusiveMinimum', 'exclusiveMaximum', 'multipleOf', 'minLength',
    'maxLength', 'pattern', 'format', 'minItems', 'maxItems', 'uniqueItems', 'minProperties', 'maxProperties',
    'minContains', 'maxContains'].map((key) => [key, 'bound'])),
  ...Object.fromEntries(['anyOf', 'oneOf', 'allOf', 'not', 'if', 'then', 'else', '$ref', '$defs', 'definitions',
    'dependentSchemas', 'discriminator'].map((key) => [key, 'composition'])),
  ...Object.fromEntries(['properties', 'patternProperties', 'additionalProperties', 'propertyNames',
    'unevaluatedProperties', 'items', 'prefixItems', 'additionalItems', 'contains', 'unevaluatedItems']
    .map((key) => [key, 'shape'])),
};
const TOOL_PROSE = new Set(['name', 'title', 'description']);

const isObject = (value) => Boolean(value) && typeof value === 'object' && !Array.isArray(value);
const bytes = (text) => Buffer.byteLength(text, 'utf8');
const jsonBytes = (value) => (value === undefined ? 0 : bytes(JSON.stringify(value)));
const pointer = (path, key) => `${path}/${String(key).replaceAll('~', '~0').replaceAll('/', '~1')}`;

function deepEqual(a, b) {
  if (a === b) return true;
  if (Array.isArray(a) || Array.isArray(b)) {
    return Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((item, i) => deepEqual(item, b[i]));
  }
  if (!isObject(a) || !isObject(b)) return false;
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every((key) => Object.hasOwn(b, key) && deepEqual(a[key], b[key]));
}

/** Calls visit(text) for every prose string a schema carries (description/title keywords, not property names). */
function walkProse(schema, visit) {
  if (Array.isArray(schema)) { schema.forEach((item) => walkProse(item, visit)); return; }
  if (!isObject(schema)) return;
  for (const [key, value] of Object.entries(schema)) {
    if (PROSE.has(key)) { if (typeof value === 'string') visit(value); }
    else if (SCHEMA_MAPS.has(key) && isObject(value)) Object.values(value).forEach((child) => walkProse(child, visit));
    else if (SCHEMA_ONE.has(key) || SCHEMA_LIST.has(key)) walkProse(value, visit);
  }
}

/** A schema with prose keywords removed and required sorted; data keywords (enum, default, const) stay verbatim. */
export function structure(schema) {
  if (Array.isArray(schema)) return schema.map(structure);
  if (!isObject(schema)) return schema;
  const out = {};
  for (const [key, value] of Object.entries(schema)) {
    if (PROSE.has(key)) continue;
    if (SCHEMA_MAPS.has(key) && isObject(value)) out[key] = Object.fromEntries(Object.entries(value).map(([name, child]) => [name, structure(child)]));
    else if (SCHEMA_ONE.has(key) || SCHEMA_LIST.has(key)) out[key] = structure(value);
    else if (key === 'required' && Array.isArray(value)) out[key] = value.toSorted();
    else out[key] = value;
  }
  return out;
}

function diffSchema(a, b, path, kind, changes) {
  if (deepEqual(a, b)) return;
  if (!isObject(a) || !isObject(b)) {
    if (Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every(isObject)) {
      a.forEach((item, i) => diffSchema(item, b[i], pointer(path, i), kind, changes));
      return;
    }
    changes.push({ path, kind, change: 'changed', before: a, after: b });
    return;
  }
  for (const key of new Set([...Object.keys(a), ...Object.keys(b)])) {
    const keyKind = KIND[key] ?? 'other';
    const at = pointer(path, key);
    if (!Object.hasOwn(b, key)) changes.push({ path: at, kind: keyKind, change: 'removed', before: a[key] });
    else if (!Object.hasOwn(a, key)) changes.push({ path: at, kind: keyKind, change: 'added', after: b[key] });
    else if (key === 'required' && Array.isArray(a[key]) && Array.isArray(b[key])) {
      for (const name of a[key]) if (!b[key].includes(name)) changes.push({ path: at, kind: 'required', change: 'removed', before: name });
      for (const name of b[key]) if (!a[key].includes(name)) changes.push({ path: at, kind: 'required', change: 'added', after: name });
    } else if (SCHEMA_MAPS.has(key) && isObject(a[key]) && isObject(b[key])) {
      for (const name of new Set([...Object.keys(a[key]), ...Object.keys(b[key])])) {
        const child = pointer(at, name);
        const childKind = key === 'properties' ? 'property' : keyKind;
        if (!Object.hasOwn(b[key], name)) changes.push({ path: child, kind: childKind, change: 'removed', before: a[key][name] });
        else if (!Object.hasOwn(a[key], name)) changes.push({ path: child, kind: childKind, change: 'added', after: b[key][name] });
        else diffSchema(a[key][name], b[key][name], child, keyKind, changes);
      }
    } else if (SCHEMA_ONE.has(key) || SCHEMA_LIST.has(key)) diffSchema(a[key], b[key], at, keyKind, changes);
    else if (!deepEqual(a[key], b[key])) changes.push({ path: at, kind: keyKind, change: 'changed', before: a[key], after: b[key] });
  }
}

/** Reads and checks a capture file; throws {code, exitCode} errors. */
export function loadCapture(file) {
  let capture;
  try { capture = JSON.parse(readFileSync(file, 'utf8')); }
  catch (error) { throw Object.assign(new Error(`${file}: ${errorMessage(error)}`), { code: 'invalidCapture' }); }
  const pages = capture?.toolsList;
  if (!isObject(capture) || !isObject(capture.initialize) || !Array.isArray(pages) || !pages.length ||
    !pages.every((page) => isObject(page?.result) && Array.isArray(page.result.tools))) {
    throw Object.assign(new Error(`${file}: expected a capture with initialize and toolsList[].result.tools (run capture first)`), { code: 'invalidCapture' });
  }
  const tools = pages.flatMap((page) => page.result.tools);
  if (!tools.every((tool) => isObject(tool) && typeof tool.name === 'string')) {
    throw Object.assign(new Error(`${file}: every tool needs a string name`), { code: 'invalidCapture' });
  }
  return { capture, tools };
}

const duplicates = (tools) => [...new Set(tools.map((t) => t.name).filter((name, i, all) => all.indexOf(name) !== i))];

export function sizeCapture({ capture, tools }) {
  const rows = tools.map((tool) => {
    let schemaProseBytes = 0;
    walkProse(tool.inputSchema, (text) => { schemaProseBytes += bytes(text); });
    walkProse(tool.outputSchema, (text) => { schemaProseBytes += bytes(text); });
    return {
      name: tool.name,
      bytes: jsonBytes(tool),
      descriptionBytes: typeof tool.description === 'string' ? bytes(tool.description) : 0,
      inputSchemaBytes: jsonBytes(tool.inputSchema),
      outputSchemaBytes: jsonBytes(tool.outputSchema),
      schemaProseBytes,
    };
  });
  const sum = (key) => rows.reduce((total, row) => total + row[key], 0);
  const instructions = capture.initialize.instructions;
  const instructionsBytes = typeof instructions === 'string' ? bytes(instructions) : 0;
  return {
    ok: true,
    protocolVersion: capture.initialize.protocolVersion ?? null,
    serverInfo: capture.initialize.serverInfo ?? null,
    pages: capture.toolsList.length,
    duplicateNames: duplicates(tools),
    totals: {
      tools: rows.length, toolBytes: sum('bytes'), descriptionBytes: sum('descriptionBytes'),
      inputSchemaBytes: sum('inputSchemaBytes'), outputSchemaBytes: sum('outputSchemaBytes'),
      schemaProseBytes: sum('schemaProseBytes'), instructionsBytes, surfaceBytes: sum('bytes') + instructionsBytes,
    },
    tools: rows,
  };
}

function proseOf(tool) {
  const texts = [tool.title, tool.description, tool.annotations?.title];
  walkProse(tool.inputSchema, (text) => texts.push(text));
  walkProse(tool.outputSchema, (text) => texts.push(text));
  return texts;
}

export function diffCaptures(before, after) {
  for (const [side, loaded] of [['before', before], ['after', after]]) {
    const dupes = duplicates(loaded.tools);
    if (dupes.length) throw Object.assign(new Error(`${side} capture repeats tool names: ${dupes.join(', ')}`), { code: 'duplicateTool' });
  }
  const a = new Map(before.tools.map((tool) => [tool.name, tool]));
  const b = new Map(after.tools.map((tool) => [tool.name, tool]));
  const changes = [];
  const proseChanged = [];
  for (const name of a.keys()) if (!b.has(name)) changes.push({ tool: name, path: '', kind: 'name', change: 'removed' });
  for (const name of b.keys()) if (!a.has(name)) changes.push({ tool: name, path: '', kind: 'name', change: 'added' });
  for (const [name, left] of a) {
    const right = b.get(name);
    if (!right) continue;
    const found = [];
    for (const key of ['inputSchema', 'outputSchema']) diffSchema(structure(left[key]), structure(right[key]), `/${key}`, 'other', found);
    const { title: _leftTitle, ...leftAnnotations } = isObject(left.annotations) ? left.annotations : {};
    const { title: _rightTitle, ...rightAnnotations } = isObject(right.annotations) ? right.annotations : {};
    diffSchema(leftAnnotations, rightAnnotations, '/annotations', 'annotation', found);
    for (const change of found) if (change.path.startsWith('/annotations/')) change.kind = 'annotation';
    for (const key of new Set([...Object.keys(left), ...Object.keys(right)])) {
      if (TOOL_PROSE.has(key) || ['inputSchema', 'outputSchema', 'annotations'].includes(key)) continue;
      if (!deepEqual(left[key], right[key])) found.push({ path: `/${key}`, kind: 'other', change: 'changed', before: left[key], after: right[key] });
    }
    changes.push(...found.map((change) => ({ tool: name, ...change })));
    if (!deepEqual(proseOf(left), proseOf(right))) proseChanged.push(name);
  }
  const counts = {};
  for (const change of changes) counts[change.kind] = (counts[change.kind] ?? 0) + 1;
  return {
    ok: true,
    structural: changes.length > 0,
    tools: { before: a.size, after: b.size },
    counts,
    changes,
    prose: {
      instructionsChanged: (before.capture.initialize.instructions ?? null) !== (after.capture.initialize.instructions ?? null),
      tools: proseChanged,
    },
  };
}

/** Runs the stdio handshake and tools/list pagination; resolves the capture object. */
export function captureServer({ cmd, cwd, protocol, timeout }) {
  return new Promise((resolve, reject) => {
    const child = spawn('sh', ['-c', cmd], { cwd, stdio: ['pipe', 'pipe', 'pipe'], detached: process.platform !== 'win32' });
    const pending = new Map();
    const pages = [];
    const seenCursors = new Set();
    let stderr = '';
    let buffer = '';
    let nextId = 1;
    let settled = false;
    let initialize;

    let exited = false;
    child.once('exit', () => { exited = true; });
    // The capture is complete before this runs: close stdin, then signal the process group if it stays.
    const stopServer = () => new Promise((done) => {
      if (exited) { done(); return; }
      // A negative pid signals the server's whole process group (POSIX); Windows has no groups.
      const signal = (name) => { try { const pid = Number(child.pid); process.kill(process.platform === 'win32' ? pid : -pid, name); } catch { /* gone */ } };
      const term = setTimeout(() => signal('SIGTERM'), 2000);
      const kill = setTimeout(() => signal('SIGKILL'), 4000);
      child.once('exit', () => { clearTimeout(term); clearTimeout(kill); done(); });
      child.stdin.end();
    });
    const finish = (error, value) => {
      if (settled) return;
      settled = true;
      if (timer) clearTimeout(timer);
      void stopServer().then(() => (error ? reject(Object.assign(error, { stderr })) : resolve(value)));
    };
    const send = (message) => child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', ...message })}\n`);
    const request = (method, params) => new Promise((done, failed) => {
      const id = nextId++;
      pending.set(id, { done, failed, method });
      send({ id, method, params });
    });
    const timer = timeout === undefined ? null
      : setTimeout(() => finish(Object.assign(new Error(`capture did not finish within ${timeout} ms`), { code: 'timeout' })), timeout);

    const onMessage = (message) => {
      if (Object.hasOwn(message, 'method')) {
        if (!Object.hasOwn(message, 'id')) return;
        if (message.method === 'ping') send({ id: message.id, result: {} });
        else send({ id: message.id, error: { code: -32601, message: `capture client does not handle ${message.method}` } });
        return;
      }
      const entry = pending.get(message.id);
      if (!entry) return;
      pending.delete(message.id);
      if (message.error) entry.failed(Object.assign(new Error(`${entry.method} failed: ${JSON.stringify(message.error)}`), { code: 'rpcError' }));
      else entry.done(message.result);
    };
    child.stdout.setEncoding('utf8');
    child.stdout.on('data', (chunk) => {
      buffer += chunk;
      let newline;
      while ((newline = buffer.indexOf('\n')) >= 0) {
        const line = buffer.slice(0, newline).trim();
        buffer = buffer.slice(newline + 1);
        if (!line) continue;
        let message;
        try { message = JSON.parse(line); }
        catch { finish(Object.assign(new Error(`server wrote a non-JSON stdout line: ${line}`), { code: 'protocolError' })); return; }
        onMessage(message);
      }
    });
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (chunk) => { stderr += chunk; });
    child.on('error', (error) => finish(Object.assign(error, { code: 'spawnFailed' })));
    child.on('exit', (code, signal) => finish(Object.assign(new Error(`server exited (${signal ?? code}) before the capture finished`), { code: 'serverExited' })));
    child.stdin.on('error', () => { /* reported by the exit handler */ });

    (async () => {
      initialize = await request('initialize', {
        protocolVersion: protocol, capabilities: {}, clientInfo: { name: 'mcp-surface', version: '1' },
      });
      send({ method: 'notifications/initialized' });
      let cursor;
      do {
        if (cursor !== undefined) {
          if (seenCursors.has(cursor)) throw Object.assign(new Error(`tools/list repeated cursor ${JSON.stringify(cursor)}`), { code: 'cursorLoop' });
          seenCursors.add(cursor);
        }
        const result = await request('tools/list', cursor === undefined ? {} : { cursor });
        pages.push({ cursor: cursor ?? null, result });
        cursor = result?.nextCursor ?? undefined;
      } while (cursor !== undefined);
      finish(null, { command: cmd, protocolVersion: protocol, capturedAt: new Date().toISOString(), initialize, toolsList: pages });
    })().catch((error) => finish(error));
  });
}

function parseOptions(argv, names) {
  const options = { _: [] };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (names.includes(arg)) {
      const value = argv[++i];
      if (value === undefined || value.startsWith('--')) fail('invalidArguments', `${arg} requires a value`, 2);
      options[arg.slice(2)] = value;
    } else if (arg.startsWith('--')) fail('invalidArguments', `Unknown option: ${arg} (see --help)`, 2);
    else options._.push(arg);
  }
  return options;
}

function load(file) {
  try { return loadCapture(file); }
  catch (error) { return fail(error.code ?? 'invalidCapture', errorMessage(error), 2); }
}

async function main(argv) {
  const [command, ...rest] = argv;
  if (!command || command === '--help' || command === '-h' || rest.includes('--help')) {
    console.log(HELP);
    if (!command) process.exitCode = 2;
    return;
  }
  if (command === 'capture') {
    const options = parseOptions(rest, ['--cmd', '--out', '--cwd', '--protocol', '--timeout']);
    if (options._.length || !options.cmd || !options.out) fail('invalidArguments', 'capture needs --cmd and --out (see --help)', 2);
    const timeout = options.timeout === undefined ? undefined : Number(options.timeout);
    if (timeout !== undefined && (!Number.isSafeInteger(timeout) || timeout < 1)) fail('invalidArguments', '--timeout must be a positive integer of milliseconds', 2);
    let capture;
    try {
      capture = await captureServer({ cmd: options.cmd, cwd: options.cwd ?? process.cwd(), protocol: options.protocol ?? DEFAULT_PROTOCOL, timeout });
    } catch (error) {
      fail(error.code ?? 'captureFailed', errorMessage(error), 1, error.stderr ? { stderr: error.stderr } : {});
    }
    try { writeFileSync(options.out, `${JSON.stringify(capture, null, 2)}\n`, { flag: 'wx' }); }
    catch (error) { fail(error.code === 'EEXIST' ? 'outputExists' : 'writeFailed', errorMessage(error), 2); }
    const tools = capture.toolsList.reduce((total, page) => total + (page.result?.tools?.length ?? 0), 0);
    console.log(JSON.stringify({ ok: true, out: options.out, protocolVersion: capture.initialize.protocolVersion ?? null,
      serverInfo: capture.initialize.serverInfo ?? null, pages: capture.toolsList.length, tools }));
    return;
  }
  if (command === 'size') {
    const options = parseOptions(rest, []);
    if (options._.length !== 1) fail('invalidArguments', 'size needs one capture file', 2);
    console.log(JSON.stringify({ capture: options._[0], ...sizeCapture(load(options._[0])) }, null, 1));
    return;
  }
  if (command === 'diff') {
    const options = parseOptions(rest, []);
    if (options._.length !== 2) fail('invalidArguments', 'diff needs <before> and <after> capture files', 2);
    let result;
    try { result = diffCaptures(load(options._[0]), load(options._[1])); }
    catch (error) { fail(error.code ?? 'invalidCapture', errorMessage(error), 2); }
    console.log(JSON.stringify({ before: options._[0], after: options._[1], ...result }, null, 1));
    process.exitCode = result.structural ? 1 : 0;
    return;
  }
  fail('invalidArguments', `Unknown command: ${command} (capture, size or diff; see --help)`, 2);
}

if (isMainModule(import.meta.url)) await main(process.argv.slice(2));
