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

// A stdio server that pings the client, emits a notification and serves tools/list in two pages.
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
});
`;

test('capture follows every tools/list page, answers pings and refuses to overwrite', () => {
  const dir = mkdtempSync(join(tmpdir(), 'mcp-surface-'));
  try {
    writeFileSync(join(dir, 'server.mjs'), SERVER);
    const out = join(dir, 'cap.json');
    const result = run('capture', '--cmd', `${process.execPath} server.mjs`, '--cwd', dir, '--out', out);
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), { ok: true, out, protocolVersion: '2025-11-25', serverInfo: { name: 'fake', version: '1' }, pages: 2, tools: 3 });
    const capture = JSON.parse(readFileSync(out, 'utf8'));
    assert.deepEqual(capture.toolsList.map((page) => page.cursor), [null, 'p2']);
    const again = run('capture', '--cmd', `${process.execPath} server.mjs`, '--cwd', dir, '--out', out);
    assert.equal(again.status, 2);
    assert.equal(JSON.parse(again.stdout).error.code, 'outputExists');
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
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
