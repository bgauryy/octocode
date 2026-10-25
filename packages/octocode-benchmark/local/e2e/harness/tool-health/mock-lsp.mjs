import fs from 'node:fs';
import { isMainModule } from '../../../../../../skills/common/scripts/lib/entrypoint.mjs';
import { fixtureModel } from './mock-lsp-model.mjs';

export function frame(message) {
  const body = Buffer.from(JSON.stringify(message));
  return Buffer.concat([Buffer.from(`Content-Length: ${body.length}\r\n\r\n`), body]);
}

export function frameReader(receive) {
  let buffer = Buffer.alloc(0);
  return chunk => {
    buffer = Buffer.concat([buffer, chunk]);
    while (true) {
      const end = buffer.indexOf('\r\n\r\n');
      if (end < 0) return;
      const length = Number(/Content-Length:\s*(\d+)/i.exec(buffer.subarray(0, end).toString())?.[1]);
      if (!Number.isSafeInteger(length)) throw new Error('Missing Content-Length');
      if (buffer.length < end + 4 + length) return;
      const body = buffer.subarray(end + 4, end + 4 + length);
      buffer = buffer.subarray(end + 4 + length);
      receive(JSON.parse(body));
    }
  };
}

export function createProtocol(send, { exit = () => {}, trace = () => {} } = {}) {
  let model;
  return message => {
    trace(message);
    if (message.method === 'exit') { exit(); return; }
    if (message.id === undefined) return;
    try {
      let result;
      if (message.method === 'initialize') {
        model = fixtureModel(message.params.rootUri, message.params.initializationOptions ?? {});
        send({ jsonrpc: '2.0', method: '$/progress', params: { token: 'fixture', value: { kind: 'begin', title: 'Fixture ready' } } });
        send({ jsonrpc: '2.0', method: '$/progress', params: { token: 'fixture', value: { kind: 'end' } } });
        result = { serverInfo: { name: 'octocode-protocol-fixture', version: '1' }, capabilities: { positionEncoding: 'utf-16', textDocumentSync: 1, definitionProvider: true, typeDefinitionProvider: true, implementationProvider: true, referencesProvider: true, hoverProvider: true, documentSymbolProvider: true, workspaceSymbolProvider: true, callHierarchyProvider: true, typeHierarchyProvider: true, diagnosticProvider: { interFileDependencies: false, workspaceDiagnostics: false } } };
      } else if (message.method === 'shutdown') result = null;
      else if (!model) throw new Error('initialize required');
      else result = model(message.method, message.params ?? {});
      send({ jsonrpc: '2.0', id: message.id, result });
    } catch (error) { send({ jsonrpc: '2.0', id: message.id, error: { code: error.code ?? -32603, message: error.message } }); }
  };
}

if (isMainModule(import.meta.url)) {
  const trace = process.env.OCTOCODE_TEST_LSP_TRACE;
  const traceFile = trace ? fs.openSync(trace, 'a') : undefined;
  const receive = createProtocol(message => process.stdout.write(frame(message)), {
    exit: () => process.exit(0),
    trace: message => { if (traceFile !== undefined) fs.writeSync(traceFile, `${JSON.stringify({ pid: process.pid, ...message })}\n`); },
  });
  process.stdin.on('data', frameReader(receive));
}
