import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { ROOT } from '../paths.mjs';
import { nativeBinaryStamp } from '../native-binary.mjs';
import { startSurfaces } from './surfaces.mjs';

test('hermetic CLI and MCP execute the binary recorded in provenance', { skip: process.platform === 'win32' }, async () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-native-provenance-'));
  const binary = path.join(directory, 'probe');
  const source = `#!${process.execPath}
const marker = 'isolated-native-provenance';
const result = {results:[{index:0,data:{marker,isolated:process.env.GITHUB_TOKEN !== 'inherited-sentinel'}}]};
if (process.argv.includes('mcp')) {
  require('node:readline').createInterface({input:process.stdin}).on('line', line => {
    const message = JSON.parse(line);
    if (message.id === undefined) return;
    let value;
    if (message.method === 'initialize') value = {protocolVersion:'2025-06-18',capabilities:{tools:{}},serverInfo:{name:marker,version:'1'}};
    else if (message.method === 'tools/list') value = {tools:[{name:'provenanceProbe',inputSchema:{type:'object'}}]};
    else value = {content:[{type:'text',text:JSON.stringify(result)}],structuredContent:result};
    process.stdout.write(JSON.stringify({jsonrpc:'2.0',id:message.id,result:value})+'\\n');
  });
} else process.stdout.write(JSON.stringify(result));
`;
  fs.writeFileSync(binary, source, { mode: 0o755 });
  const previous = { binary: process.env.OCTOCODE_NATIVE_BIN, token: process.env.GITHUB_TOKEN };
  process.env.OCTOCODE_NATIVE_BIN = binary;
  process.env.GITHUB_TOKEN = 'inherited-sentinel';
  let surfaces, server;
  try {
    const stamp = nativeBinaryStamp();
    assert.equal(stamp.sha256, createHash('sha256').update(source).digest('hex'));
    surfaces = await startSurfaces(path.join(ROOT, 'packages/octocode-benchmark/local/e2e/fixtures/tool-health'));
    const cli = await surfaces.cliCall('provenanceProbe', {});
    assert.equal(cli.isError, false, cli.text);
    server = await surfaces.mcpServer('provenanceProbe');
    const mcp = await surfaces.mcpCall(server, 'provenanceProbe', {});
    for (const response of [cli, mcp]) {
      assert.deepEqual(response.json.results[0].data, { marker: 'isolated-native-provenance', isolated: true });
    }
    assert.equal(nativeBinaryStamp().sha256, stamp.sha256);
  } finally {
    server?.close();
    await surfaces?.close();
    if (previous.binary === undefined) delete process.env.OCTOCODE_NATIVE_BIN;
    else process.env.OCTOCODE_NATIVE_BIN = previous.binary;
    if (previous.token === undefined) delete process.env.GITHUB_TOKEN;
    else process.env.GITHUB_TOKEN = previous.token;
    fs.rmSync(directory, { recursive: true, force: true });
  }
});
