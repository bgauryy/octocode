import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import test from 'node:test';
import { EventEmitter } from 'node:events';
import { configureMockLsp, stopTestBrokers } from './mock-lsp-config.mjs';
import { createProtocol, frame, frameReader } from './mock-lsp.mjs';

const fixture = path.resolve(import.meta.dirname, '../../fixtures/tool-health');
const code = path.join(fixture, 'advanced-local/code');
const uri = name => pathToFileURL(path.join(code, name)).href;
const protocol = root => {
  const replies = [];
  const receive = createProtocol(message => replies.push(message));
  receive({ id: 1, method: 'initialize', params: { rootUri: pathToFileURL(root).href } });
  return { receive, replies };
};

test('mock config pins all fixture languages to an explicit script and disables discovery side effects', () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-mock-lsp-'));
  try {
    const env = configureMockLsp(home);
    assert.equal(env.OCTOCODE_LSP_AUTO_INSTALL, 'off');
    assert.equal(env.OCTOCODE_LSP_PREWARM, 'off');
    assert.equal(env.OCTOCODE_TRUST_PROJECT_LSP_CONFIG, 'false');
    assert.equal(env.OCTOCODE_LSP_CONFIG, undefined);
    const config = JSON.parse(fs.readFileSync(path.join(home, '.octocode/lsp-servers.json'), 'utf8'));
    for (const extension of ['.ts', '.js', '.rs']) {
      assert.equal(config.languageServers[extension].command, process.execPath);
      assert.ok(config.languageServers[extension].args[0].endsWith('/mock-lsp.mjs'));
    }
  } finally { fs.rmSync(home, { recursive: true, force: true }); }
});

test('protocol framing handles split UTF-8 frames and multiple messages without timers', () => {
  const messages = [{ id: 1, method: 'echo', params: '界😀' }, { id: 2, method: 'shutdown' }];
  const observed = [];
  const read = frameReader(message => observed.push(message));
  for (const byte of Buffer.concat(messages.map(frame))) read(Buffer.from([byte]));
  assert.deepEqual(observed, messages);
});

test('mock responds with LSP locations, not tool envelopes, and respects declaration filtering', () => {
  const { receive, replies } = protocol(code);
  const params = { textDocument: { uri: uri('model.ts') }, position: { line: 13, character: 16 }, context: { includeDeclaration: false } };
  receive({ id: 2, method: 'textDocument/references', params });
  assert.deepEqual(replies.at(-1).result.map(row => [path.basename(new URL(row.uri).pathname), row.range.start.line]), [['model.ts', 18], ['model.ts', 19], ['use.ts', 3]]);
  assert.ok(!('results' in replies.at(-1)));
  params.context.includeDeclaration = true;
  receive({ id: 3, method: 'textDocument/references', params });
  assert.equal(replies.at(-1).result.length, 5);
});

test('Rust fixture references follow received initialization context rather than a test label', () => {
  const root = path.join(fixture, 'advanced-local/rust');
  const replies = [];
  const receive = createProtocol(message => replies.push(message));
  receive({ id: 1, method: 'initialize', params: { rootUri: pathToFileURL(root).href, initializationOptions: { cargo: { noDefaultFeatures: true, features: ['dim'], cfgs: ['transport_cfg', '!transport_cfg'] } } } });
  receive({ id: 2, method: 'textDocument/references', params: { textDocument: { uri: pathToFileURL(path.join(root, 'src/lib.rs')).href }, position: { line: 0, character: 7 }, context: { includeDeclaration: false } } });
  assert.deepEqual(replies.at(-1).result.map(row => path.basename(new URL(row.uri).pathname)).toSorted(), ['always.rs', 'dim_feature.rs']);
});

test('occurrence definitions remain stable when native resolves the returned declaration again', () => {
  const { receive, replies } = protocol(code);
  const line = fs.readFileSync(path.join(code, 'model.ts'), 'utf8').split('\n')[24];
  receive({ id: 2, method: 'textDocument/definition', params: { textDocument: { uri: uri('model.ts') }, position: { line: 24, character: line.lastIndexOf('run') } } });
  const declaration = replies.at(-1).result[0];
  assert.equal(declaration.range.start.line, 23);
  receive({ id: 3, method: 'textDocument/definition', params: { textDocument: { uri: declaration.uri }, position: declaration.range.start } });
  assert.deepEqual(replies.at(-1).result, [declaration]);
});

test('unsupported protocol methods fail explicitly instead of fabricating empty success', () => {
  const { receive, replies } = protocol(code);
  receive({ id: 9, method: 'unsupported/example', params: {} });
  assert.equal(replies.at(-1).error.code, -32601);
  receive({ id: 10, method: 'shutdown' });
  assert.deepEqual(replies.at(-1), { jsonrpc: '2.0', id: 10, result: null });
});

test('cleanup bounds unresponsive owned sockets with fake time and never visits outside temp', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const visited = [], connections = [];
  const tmp = path.resolve('/owned-test');
  const filesystem = {
    readdirSync(directory) {
      visited.push(directory);
      if (directory === tmp) return [{ name: 'octocode-123', isDirectory: () => true }, { name: 'home', isDirectory: () => true }];
      assert.equal(directory, path.join(tmp, 'octocode-123'));
      return ['serve-owned.sock'];
    },
    statSync: () => ({ isSocket: () => true }),
  };
  const connect = socketPath => {
    const socket = new EventEmitter();
    socket.path = socketPath;
    socket.end = message => { socket.message = message; };
    socket.destroy = () => { socket.destroyed = true; };
    connections.push(socket);
    return socket;
  };
  const closing = stopTestBrokers(tmp, { filesystem, connect, deadlineMs: 100 });
  assert.equal(connections.length, 1);
  connections[0].emit('connect');
  assert.equal(connections[0].message, '{"stop":true}\n');
  t.mock.timers.tick(100);
  await closing;
  assert.equal(connections[0].destroyed, true);
  assert.deepEqual(visited, [tmp, path.join(tmp, 'octocode-123')]);
  assert.equal(connections[0].path, path.join(tmp, 'octocode-123/serve-owned.sock'));
});
