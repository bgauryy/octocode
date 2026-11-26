import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

const script = new URL('../mcp-surface.mjs', import.meta.url).pathname;
const run = (...args) => spawnSync(process.execPath, [script, ...args], { encoding: 'utf8' });

const tool = (name, inputSchema, extra = {}) => ({ name, description: `${name} does one job.`, inputSchema, ...extra });
const schema = () => ({
  type: 'object',
  required: ['path', 'mode'],
  properties: {
    path: { type: 'string', description: 'File path.', maxLength: 200 },
    mode: { type: 'string', enum: ['a', 'b'], default: 'a' },
    description: { type: 'string', description: 'A field named description is data, not prose.' },
  },
});

function captureFile(dir, name, tools, instructions = 'Use tools — carefully.') {
  const file = join(dir, name);
  writeFileSync(file, JSON.stringify({ command: 'x', initialize: { protocolVersion: '2025-11-25', instructions }, toolsList: [{ cursor: null, result: { tools } }] }));
  return file;
}

const SERVER_INFO = 'io.modelcontextprotocol/serverInfo';
// A capture of the stateless handshake: instructions on the discover result, server info in its _meta.
function discoverFile(dir, name, tools, instructions = 'Use tools — carefully.') {
  const file = join(dir, name);
  writeFileSync(file, JSON.stringify({ command: 'x', handshake: 'discover', protocolVersion: '2026-07-28',
    discover: { resultType: 'complete', supportedVersions: ['2026-07-28'], capabilities: { tools: {} }, instructions,
      _meta: { [SERVER_INFO]: { name: 'modern', version: '2' } } },
    toolsList: [{ cursor: null, result: { resultType: 'complete', tools } }] }));
  return file;
}

// A stdio server of the handshake revisions: pings the client, emits a notification and serves tools/list in two
// pages. Before initialize it ignores an unknown request, or answers -32601 (argv "reject"), or exits 8 (argv "strict").
const SERVER = `
import { createInterface } from 'node:readline';
const send = (m) => process.stdout.write(JSON.stringify({ jsonrpc: '2.0', ...m }) + '\\n');
const pages = { '': { tools: [{ name: 'a', inputSchema: { type: 'object' } }], nextCursor: 'p2' },
  p2: { tools: [{ name: 'b', inputSchema: { type: 'object' } }, { name: 'c', inputSchema: { type: 'object' } }] } };
createInterface({ input: process.stdin }).on('line', (line) => {
  const m = JSON.parse(line);
  if (m.method === 'initialize') {
    send({ id: 900, method: 'ping' });
    send({ method: 'notifications/message', params: {} });
    send({ id: m.id, result: { protocolVersion: m.params.protocolVersion, capabilities: { tools: {} }, serverInfo: { name: 'fake', version: '1' }, instructions: 'é' } });
  } else if (m.method === 'tools/list') send({ id: m.id, result: pages[m.params.cursor ?? ''] });
  else if (m.id === 900 && !m.result) process.exit(9);
  else if (m.method === 'server/discover' && process.argv[2] === 'strict') process.exit(8);
  else if (m.method === 'server/discover' && process.argv[2] === 'reject') send({ id: m.id, error: { code: -32601, message: 'Method not found' } });
});
`;

// A stdio server of the stateless revision only: every request must carry the version and client capabilities in
// _meta. argv: mode (strict exits 9 on any handshake message; slow answers after 400 ms and rejects initialize;
// legacyOnly answers every version with the unsupported-version error listing no stateless revision).
const STATELESS_SERVER = `
import { createInterface } from 'node:readline';
const KEY = 'io.modelcontextprotocol/';
const SUPPORTED = ['2026-07-28'];
const mode = process.argv[2];
const write = (m) => process.stdout.write(JSON.stringify({ jsonrpc: '2.0', ...m }) + '\\n');
const send = (m) => (mode === 'slow' ? setTimeout(() => write(m), 400) : write(m));
const info = { [KEY + 'serverInfo']: { name: 'modern', version: '2' } };
const pages = { '': { tools: [{ name: 'a', inputSchema: { type: 'object' } }], nextCursor: 'p2' },
  p2: { tools: [{ name: 'b', inputSchema: { type: 'object' } }, { name: 'c', inputSchema: { type: 'object' } }] } };
createInterface({ input: process.stdin }).on('line', (line) => {
  const m = JSON.parse(line);
  if (m.method === 'initialize' || m.method === 'notifications/initialized') {
    if (mode !== 'slow') process.exit(9);
    if (m.id !== undefined) send({ id: m.id, error: { code: -32601, message: 'Method not found; supported: ' + SUPPORTED } });
    return;
  }
  const meta = m.params?._meta ?? {};
  const version = meta[KEY + 'protocolVersion'];
  if (typeof version !== 'string' || typeof meta[KEY + 'clientCapabilities'] !== 'object') {
    send({ id: m.id, error: { code: -32602, message: 'Invalid params' } });
  } else if (mode === 'legacyOnly' || !SUPPORTED.includes(version)) {
    send({ id: m.id, error: { code: -32022, message: 'Unsupported protocol version',
      data: { supported: mode === 'legacyOnly' ? ['2025-11-25'] : SUPPORTED, requested: version } } });
  } else if (m.method === 'server/discover') {
    send({ id: m.id, result: { resultType: 'complete', supportedVersions: SUPPORTED, capabilities: { tools: {} },
      instructions: 'é', ttlMs: 0, cacheScope: 'private', _meta: info } });
  } else if (m.method === 'tools/list') {
    send({ id: m.id, result: { resultType: 'complete', ...pages[m.params.cursor ?? ''], ttlMs: 0, cacheScope: 'private', _meta: info } });
  } else send({ id: m.id, error: { code: -32601, message: 'Method not found' } });
});
`;

test('capture follows every tools/list page, answers pings and refuses to overwrite', () => {
  const dir = mkdtempSync(join(tmpdir(), 'mcp-surface-'));
  try {
    writeFileSync(join(dir, 'server.mjs'), SERVER);
    const out = join(dir, 'cap.json');
    const result = run('capture', '--cmd', `${process.execPath} server.mjs reject`, '--cwd', dir, '--out', out);
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), { ok: true, out, handshake: 'initialize', protocolVersion: '2025-11-25', serverInfo: { name: 'fake', version: '1' }, pages: 2, tools: 3 });
    const capture = JSON.parse(readFileSync(out, 'utf8'));
    assert.equal(capture.handshake, 'initialize');
    assert.equal(capture.initialize.instructions, 'é');
    assert.deepEqual(capture.toolsList.map((page) => page.cursor), [null, 'p2']);
    const again = run('capture', '--cmd', `${process.execPath} server.mjs reject`, '--cwd', dir, '--out', out);
    assert.equal(again.status, 2);
    assert.equal(JSON.parse(again.stdout).error.code, 'outputExists');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('capture falls back to initialize when the probe gets no answer, and skips the probe for an earlier --protocol', () => {
  const dir = mkdtempSync(join(tmpdir(), 'mcp-surface-'));
  try {
    writeFileSync(join(dir, 'server.mjs'), SERVER);
    const silent = run('capture', '--cmd', `${process.execPath} server.mjs`, '--cwd', dir, '--out', join(dir, 'silent.json'), '--probe-wait', '100');
    assert.equal(silent.status, 0, silent.stdout + silent.stderr);
    assert.equal(JSON.parse(silent.stdout).handshake, 'initialize');
    const pinned = run('capture', '--cmd', `${process.execPath} server.mjs strict`, '--cwd', dir, '--out', join(dir, 'pinned.json'), '--protocol', '2025-06-18');
    assert.equal(pinned.status, 0, pinned.stdout + pinned.stderr);
    assert.equal(JSON.parse(pinned.stdout).protocolVersion, '2025-06-18');
    assert.equal(JSON.parse(readFileSync(join(dir, 'pinned.json'), 'utf8')).protocolVersion, '2025-06-18');
    assert.equal(run('capture', '--cmd', 'true', '--out', join(dir, 'x.json'), '--probe-wait', '0').status, 2);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('capture speaks the stateless revision: server/discover and per-request _meta, no initialize', () => {
  const dir = mkdtempSync(join(tmpdir(), 'mcp-surface-'));
  try {
    writeFileSync(join(dir, 'modern.mjs'), STATELESS_SERVER);
    const out = join(dir, 'cap.json');
    const result = run('capture', '--cmd', `${process.execPath} modern.mjs strict`, '--cwd', dir, '--out', out);
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), { ok: true, out, handshake: 'discover', protocolVersion: '2026-07-28', serverInfo: { name: 'modern', version: '2' }, pages: 2, tools: 3 });
    const capture = JSON.parse(readFileSync(out, 'utf8'));
    assert.equal(capture.handshake, 'discover');
    assert.equal(capture.discover.instructions, 'é');
    assert.equal(Object.hasOwn(capture, 'initialize'), false);
    assert.deepEqual(capture.toolsList.map((page) => page.cursor), [null, 'p2']);

    const sized = run('size', out);
    assert.equal(sized.status, 0, sized.stdout);
    const size = JSON.parse(sized.stdout);
    assert.equal(size.handshake, 'discover');
    assert.equal(size.protocolVersion, capture.protocolVersion);
    assert.deepEqual(size.serverInfo, capture.discover['_meta'][SERVER_INFO]);
    assert.equal(size.totals.tools, 3);
    assert.equal(size.totals.instructionsBytes, Buffer.byteLength(capture.discover.instructions));
    const same = run('diff', out, out);
    assert.equal(same.status, 0, same.stdout);
    assert.deepEqual(JSON.parse(same.stdout).handshake, { before: 'discover', after: 'discover' });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('capture detects a stateless server from its version error or a late probe answer, without falling back', () => {
  const dir = mkdtempSync(join(tmpdir(), 'mcp-surface-'));
  try {
    writeFileSync(join(dir, 'modern.mjs'), STATELESS_SERVER);
    const capture = (name, mode, ...args) => run('capture', '--cmd', `${process.execPath} modern.mjs ${mode}`, '--cwd', dir, '--out', join(dir, name), ...args);
    // An unsupported version is retried with one the server lists.
    const retried = capture('retried.json', 'strict', '--protocol', '2027-01-01');
    assert.equal(retried.status, 0, retried.stdout + retried.stderr);
    const body = JSON.parse(retried.stdout);
    assert.deepEqual([body.handshake, body.protocolVersion, body.tools], ['discover', '2026-07-28', 3]);
    // A server slower than the probe wait receives initialize too; its discover answer still decides.
    const slow = capture('slow.json', 'slow', '--probe-wait', '50');
    assert.equal(slow.status, 0, slow.stdout + slow.stderr);
    assert.equal(JSON.parse(slow.stdout).handshake, 'discover');
    assert.equal(Object.hasOwn(JSON.parse(readFileSync(join(dir, 'slow.json'), 'utf8')), 'initialize'), false);
    // A stateless error with no usable version fails; strict mode proves initialize was not tried.
    const none = capture('none.json', 'legacyOnly');
    assert.equal(none.status, 1, none.stdout);
    assert.equal(JSON.parse(none.stdout).error.code, 'rpcError');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('capture fails with a stable code when the server exits or breaks the stdio framing', () => {
  const dir = mkdtempSync(join(tmpdir(), 'mcp-surface-'));
  try {
    const exited = run('capture', '--cmd', `${process.execPath} -e "console.error('boom'); process.exit(3)"`, '--out', join(dir, 'a.json'));
    assert.equal(exited.status, 1);
    const body = JSON.parse(exited.stdout);
    assert.equal(body.error.code, 'serverExited');
    assert.match(body.stderr, /boom/);
    const noisy = run('capture', '--cmd', `${process.execPath} -e "console.log('hello'); setInterval(() => {}, 1000)"`, '--out', join(dir, 'b.json'));
    assert.equal(noisy.status, 1);
    assert.equal(JSON.parse(noisy.stdout).error.code, 'protocolError');
    assert.equal(run('capture', '--out', join(dir, 'c.json')).status, 2);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('size reports UTF-8 bytes per tool and totals that add up', () => {
  const dir = mkdtempSync(join(tmpdir(), 'mcp-surface-'));
  try {
    const tools = [tool('read', schema()), tool('wrïte', { type: 'object' }, { outputSchema: { type: 'object', description: 'Résumé' } })];
    const instructions = 'Use tools — carefully.';
    const result = run('size', captureFile(dir, 'cap.json', tools, instructions));
    assert.equal(result.status, 0);
    const { totals, tools: rows } = JSON.parse(result.stdout);
    rows.forEach((row, i) => {
      assert.equal(row.bytes, Buffer.byteLength(JSON.stringify(tools[i])));
      assert.equal(row.descriptionBytes, Buffer.byteLength(tools[i].description));
    });
    assert.equal(rows[1].schemaProseBytes, Buffer.byteLength('Résumé'));
    assert.equal(totals.toolBytes, rows.reduce((sum, row) => sum + row.bytes, 0));
    assert.equal(totals.instructionsBytes, Buffer.byteLength(instructions));
    assert.equal(totals.surfaceBytes, totals.toolBytes + totals.instructionsBytes);
    writeFileSync(join(dir, 'bad.json'), '{"toolsList":[]}');
    assert.equal(run('size', join(dir, 'bad.json')).status, 2);

    const old = JSON.parse(result.stdout);
    assert.equal(old.handshake, 'initialize');
    const modern = JSON.parse(run('size', discoverFile(dir, 'modern.json', tools, instructions)).stdout);
    assert.deepEqual([modern.handshake, modern.protocolVersion, modern.serverInfo], ['discover', '2026-07-28', { name: 'modern', version: '2' }]);
    assert.deepEqual(modern.totals, old.totals);
    // A capture must hold the result of the handshake it names.
    for (const [name, body] of [['nodiscover.json', { handshake: 'discover', initialize: {} }], ['unknown.json', { handshake: 'other', initialize: {} }]]) {
      writeFileSync(join(dir, name), JSON.stringify({ ...body, toolsList: [{ cursor: null, result: { tools } }] }));
      const refused = run('size', join(dir, name));
      assert.equal(refused.status, 2);
      assert.equal(JSON.parse(refused.stdout).error.code, 'invalidCapture');
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('diff ignores prose and required order, and reports each structural change', () => {
  const dir = mkdtempSync(join(tmpdir(), 'mcp-surface-'));
  try {
    const before = captureFile(dir, 'before.json', [tool('read', schema()), tool('gone', { type: 'object' })]);
    const reworded = schema();
    reworded.required.reverse();
    reworded.properties.path.description = 'Path to the file.';
    const same = captureFile(dir, 'same.json', [{ ...tool('read', reworded), description: 'Reads.' }, tool('gone', { type: 'object' })], 'Other words.');
    const unchanged = run('diff', before, same);
    assert.equal(unchanged.status, 0, unchanged.stdout);
    const quiet = JSON.parse(unchanged.stdout);
    assert.deepEqual(quiet.changes, []);
    assert.deepEqual(quiet.prose, { instructionsChanged: true, tools: ['read'] });

    const changed = schema();
    changed.required = ['path'];
    changed.properties.path.maxLength = 100;
    changed.properties.mode.enum = ['a', 'b', 'c'];
    changed.properties.mode.default = 'b';
    changed.properties.description.type = 'number';
    changed.properties.extra = { type: 'boolean' };
    const after = captureFile(dir, 'after.json', [tool('read', changed, { annotations: { readOnlyHint: false } }), tool('new', { type: 'object' })]);
    const result = run('diff', before, after);
    assert.equal(result.status, 1);
    const { changes, counts } = JSON.parse(result.stdout);
    const kinds = new Map(changes.map((change) => [`${change.tool} ${change.path} ${change.change}`, change.kind]));
    assert.equal(kinds.get('read /inputSchema/required removed'), 'required');
    assert.equal(kinds.get('read /inputSchema/properties/path/maxLength changed'), 'bound');
    assert.equal(kinds.get('read /inputSchema/properties/mode/enum changed'), 'enum');
    assert.equal(kinds.get('read /inputSchema/properties/mode/default changed'), 'default');
    assert.equal(kinds.get('read /inputSchema/properties/description/type changed'), 'type');
    assert.equal(kinds.get('read /inputSchema/properties/extra added'), 'property');
    assert.equal(kinds.get('read /annotations/readOnlyHint added'), 'annotation');
    assert.equal(kinds.get('gone  removed'), 'name');
    assert.equal(kinds.get('new  added'), 'name');
    assert.equal(Object.values(counts).reduce((sum, n) => sum + n, 0), changes.length);

    const dupes = captureFile(dir, 'dupes.json', [tool('read', schema()), tool('read', schema())]);
    assert.equal(run('diff', before, dupes).status, 2);

    // Mixed handshakes: an initialize capture before, a discover capture after.
    const beforeTools = [tool('read', schema()), tool('gone', { type: 'object' })];
    const mixedSame = JSON.parse(run('diff', before, discoverFile(dir, 'modern-same.json', beforeTools)).stdout);
    assert.deepEqual(mixedSame.handshake, { before: 'initialize', after: 'discover' });
    assert.deepEqual([mixedSame.structural, mixedSame.prose.instructionsChanged], [false, false]);
    const mixed = run('diff', before, discoverFile(dir, 'modern-after.json', [tool('read', changed), tool('gone', { type: 'object' })], 'Other words.'));
    assert.equal(mixed.status, 1, mixed.stdout);
    const mixedBody = JSON.parse(mixed.stdout);
    assert.equal(mixedBody.prose.instructionsChanged, true);
    assert.ok(mixedBody.changes.some((change) => change.path === '/inputSchema/properties/path/maxLength' && change.kind === 'bound'));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
