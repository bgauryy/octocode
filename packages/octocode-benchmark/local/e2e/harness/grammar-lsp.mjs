// Protocol evidence for native grammar position resolution, not compiler semantics.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { isMainModule } from '../../../../../skills/common/scripts/lib/entrypoint.mjs';
import { frame, frameReader } from './tool-health/mock-lsp.mjs';

const definitionLines = { TypeScript: 6, TSX: 2, JavaScript: 5, Python: 7, Go: 9, Rust: 6, Java: 4, C: 3, 'C++': 2, CUDA: 2, 'C#': 4, Scala: 3, Assembly: 3 };
export function grammarAnchors(language, fixture) {
  const symbol = fixture.call ?? 'helper';
  const lines = fixture.src.trimEnd().split('\n');
  const definition = definitionLines[language] - 1;
  const anchor = fixture.call ? lines.findIndex(line => line.includes('😀')) : lines.findIndex(line => line.includes('call helper'));
  const position = { line: anchor, character: lines[anchor].indexOf(symbol) };
  const locations = lines.flatMap((line, index) => [...line.matchAll(new RegExp(`\\b${symbol}\\b`, 'g'))].map(match => ({
    start: { line: index, character: match.index }, end: { line: index, character: match.index + symbol.length },
  })));
  const target = locations.find(range => range.start.line === definition);
  if (!target || position.character < 0) throw new Error(`Invalid independent anchor for ${language}`);
  return { symbol, position, target, locations };
}

export function configureGrammarLsp(home, cases) {
  const directory = path.join(home, '.octocode');
  fs.mkdirSync(directory, { recursive: true });
  const manifestFile = path.join(directory, 'grammar-lsp-cases.json');
  const traceFile = path.join(directory, 'grammar-lsp-trace.jsonl');
  const languageServers = {};
  const manifest = {};
  for (const entry of cases) {
    const languageId = entry.ext === 'jsx' ? 'javascriptreact' : entry.capability.languageId;
    manifest[pathToFileURL(entry.file).href] = { ...grammarAnchors(entry.language, entry.fixture), languageId };
    languageServers[`.${entry.ext}`] = {
      command: process.execPath, args: [fileURLToPath(import.meta.url), manifestFile, traceFile], languageId,
    };
  }
  fs.writeFileSync(manifestFile, JSON.stringify(manifest));
  fs.writeFileSync(path.join(directory, 'lsp-servers.json'), JSON.stringify({ languageServers }));
  return traceFile;
}

export function grammarProtocol(manifest, send, trace) {
  return message => {
    trace(message);
    if (message.id === undefined) return;
    let result;
    const params = message.params ?? {};
    if (message.method === 'initialize') result = { capabilities: { positionEncoding: 'utf-16', textDocumentSync: 1, definitionProvider: true, referencesProvider: true } };
    else if (message.method === 'shutdown') result = null;
    else {
      const uri = params.textDocument?.uri;
      const fixture = manifest[uri];
      if (!fixture || JSON.stringify(params.position) !== JSON.stringify(fixture.position)) {
        send({ jsonrpc: '2.0', id: message.id, error: { code: -32602, message: `Unexpected grammar anchor: ${JSON.stringify(params)}` } });
        return;
      }
      if (message.method === 'textDocument/definition') result = [{ uri, range: fixture.target }];
      else if (message.method === 'textDocument/references') result = fixture.locations
        .filter(range => params.context?.includeDeclaration || range.start.line !== fixture.target.start.line || range.start.character !== fixture.target.start.character)
        .map(range => ({ uri, range }));
      else {
        send({ jsonrpc: '2.0', id: message.id, error: { code: -32601, message: `Unsupported fixture method ${message.method}` } });
        return;
      }
    }
    send({ jsonrpc: '2.0', id: message.id, result });
  };
}

if (isMainModule(import.meta.url)) {
  const manifest = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
  const receive = grammarProtocol(manifest, message => process.stdout.write(frame(message)),
    message => fs.appendFileSync(process.argv[3], `${JSON.stringify(message)}\n`));
  process.stdin.on('data', frameReader(receive));
}
