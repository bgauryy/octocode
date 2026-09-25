import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const target = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (.+)$/m)[1];
const binary = process.env.COMMUNICATION_BINARY || join(root, 'skills/octocode-agents-communication/scripts/bin', target, 'octocode-agents-communication');
function fixture(t) {
  const workspace = mkdtempSync(join(tmpdir(), 'communication-tools-'));
  t.after(() => rmSync(workspace, { recursive: true, force: true }));
  const database = join(workspace, 'audit.sqlite');
  const run = (args, input = '') => spawnSync(binary, [...args, '--workspace', workspace, '--database', database], { input, encoding: 'utf8' });
  return { workspace, database, run };
}

test('MCP explicit tool selection reduces discovery and refuses hidden tools', t => {
  const f = fixture(t);
  const session = JSON.parse(f.run(['join', '{"name":"reviewer","vendor":"raw"}']).stdout).id;
  const frames = [
    { jsonrpc: '2.0', id: 1, method: 'tools/list' },
    { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'peers', arguments: {} } },
    { jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'lock', arguments: { path: 'blocked', reasoning: 'This tool was not selected' } } },
  ].map(JSON.stringify).join('\n') + '\n';
  const run = f.run(['mcp', '--tools', 'ack,send_message,peers', '--session', session], frames);
  assert.equal(run.status, 0, run.stderr);
  const rows = run.stdout.trim().split('\n').map(JSON.parse);
  assert.deepEqual(rows[0].result.tools.map(t => t.name), ['peers', 'send_message', 'ack'], 'catalog order is stable independent of selection order');
  assert.equal(rows[1].result.isError, undefined);
  assert.equal(rows[2].result.isError, true);
  const full = JSON.parse(f.run(['schema']).stdout).tools;
  assert.deepEqual(JSON.parse(f.run(['schema', 'tools', '--tools', 'ack,send_message,peers']).stdout), rows[0].result.tools);
  assert.ok(Buffer.byteLength(JSON.stringify(rows[0].result.tools)) < Buffer.byteLength(JSON.stringify(full)) / 2);
  const leases = JSON.parse(f.run(['entity', 'list', 'lease', '--session', session]).stdout);
  assert.equal(leases.items.length, 0);
});

test('invalid selections fail before opening storage or starting vendor agents', t => {
  const f = fixture(t);
  for (const tools of ['unknown', 'join', 'peers,peers', '', 'peers,']) {
    for (const args of [
      ['mcp', '--session', 'missing'],
      ['run', '--vendor', 'codex', '--model', 'unused', '--prompt', 'unused'],
    ]) {
      const result = f.run([...args, '--tools', tools]);
      assert.notEqual(result.status, 0, tools);
      assert.equal(existsSync(f.database), false);
    }
  }
  assert.notEqual(f.run(['join', '{"name":"bad","vendor":"raw"}', '--tools', 'peers']).status, 0);
  assert.equal(existsSync(f.database), false);
});
