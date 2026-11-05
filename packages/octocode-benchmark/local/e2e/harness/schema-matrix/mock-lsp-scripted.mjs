// A language server that answers from a script file, for the states a real server
// reaches rarely: failed requests, partial answers, a stalled document sync, exit.
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { frame, frameReader } from '../tool-health/mock-lsp.mjs';

const script = JSON.parse(fs.readFileSync(process.env.OCTOCODE_TEST_LSP_SCRIPT, 'utf8'));
const CAPABILITIES = {
  positionEncoding: 'utf-16', textDocumentSync: 1, definitionProvider: true, typeDefinitionProvider: true, implementationProvider: true, referencesProvider: true,
  callHierarchyProvider: true, typeHierarchyProvider: true, hoverProvider: true, documentSymbolProvider: true, workspaceSymbolProvider: true, diagnosticProvider: { interFileDependencies: false, workspaceDiagnostics: false },
};
const send = message => process.stdout.write(frame(message));
const wordAt = params => {
  const uri = params.textDocument?.uri;
  if (!uri || !params.position) return undefined;
  const line = fs.readFileSync(fileURLToPath(uri), 'utf8').split('\n')[params.position.line] ?? '';
  return [...line.matchAll(/[A-Za-z_$][\w$]*/g)].find(match => match.index <= params.position.character && params.position.character < match.index + match[0].length)?.[0];
};
const applies = (rule, params) => (!rule.uri || (params.textDocument?.uri ?? params.item?.uri ?? '').endsWith(rule.uri))
  && (!rule.word || wordAt(params) === rule.word) && (!rule.item || params.item?.name === rule.item);

const defaultAnswer = method => (method.startsWith('textDocument/') || method.startsWith('workspace/') ? [] : null);

function respond(message) {
  const params = message.params ?? {};
  const rule = (script.answers ?? []).find(entry => entry.method === message.method && applies(entry, params));
  if (rule?.exit) process.exit(1);
  if (rule?.hang) return;
  for (const note of rule?.notify ?? []) send({ jsonrpc: '2.0', ...note });
  if (rule?.error) { send({ jsonrpc: '2.0', id: message.id, error: rule.error }); return; }
  send({ jsonrpc: '2.0', id: message.id, result: rule ? rule.result ?? null : defaultAnswer(message.method) });
}

process.stdin.on('data', frameReader(message => {
  if (message.method === 'exit') process.exit(0);
  if (message.method === 'initialized') {
    if (script.pauseAfterInitialized) process.stdin.pause();
    for (const note of script.afterInitialized ?? []) send({ jsonrpc: '2.0', ...note });
    return;
  }
  if (message.method === 'textDocument/didOpen') {
    for (const note of script.afterOpen ?? []) send({ jsonrpc: '2.0', ...note, params: { uri: message.params.textDocument.uri, ...note.params } });
    return;
  }
  if (message.id === undefined) return;
  if (message.method === 'initialize') {
    send({ jsonrpc: '2.0', id: message.id, result: { serverInfo: { name: 'matrix-scripted', version: '1' }, capabilities: { ...CAPABILITIES, ...script.capabilities } } });
    return;
  }
  if (message.method === 'shutdown') { send({ jsonrpc: '2.0', id: message.id, result: null }); return; }
  respond(message);
}));
