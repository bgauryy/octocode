#!/usr/bin/env node

let buffer = Buffer.alloc(0);
let fixtureName = 'missing-init-options';

const symbolRange = {
  start: { line: 0, character: 7 },
  end: { line: 0, character: 16 },
};

function writeMessage(message) {
  const body = JSON.stringify(message);
  process.stdout.write(
    `Content-Length: ${Buffer.byteLength(body, 'utf8')}\r\n\r\n${body}`
  );
}

function respond(id, result) {
  writeMessage({
    jsonrpc: '2.0',
    id,
    result,
  });
}

function location(uri) {
  return {
    uri,
    range: symbolRange,
  };
}

function documentSymbol() {
  return {
    name: 'FooSymbol',
    detail: fixtureName,
    kind: 12,
    range: {
      start: { line: 0, character: 0 },
      end: { line: 1, character: 13 },
    },
    selectionRange: symbolRange,
  };
}

function handleRequest(message) {
  const { id, method, params } = message;

  if (method === 'initialize') {
    fixtureName = params?.initializationOptions?.fixtureName ?? fixtureName;
    respond(id, {
      capabilities: {
        textDocumentSync: 1,
        definitionProvider: true,
        referencesProvider: true,
        hoverProvider: true,
        documentSymbolProvider: true,
      },
    });
    return;
  }

  if (method === 'shutdown') {
    respond(id, null);
    return;
  }

  const uri = params?.textDocument?.uri;
  if (method === 'textDocument/definition') {
    respond(id, uri ? [location(uri)] : []);
    return;
  }

  if (method === 'textDocument/references') {
    respond(id, uri ? [location(uri)] : []);
    return;
  }

  if (method === 'textDocument/hover') {
    respond(id, {
      contents: {
        kind: 'markdown',
        value: `FooSymbol from ${fixtureName}`,
      },
      range: symbolRange,
    });
    return;
  }

  if (method === 'textDocument/documentSymbol') {
    respond(id, [documentSymbol()]);
    return;
  }

  if (id !== undefined) {
    respond(id, null);
  }
}

function readMessages() {
  while (true) {
    const headerEnd = buffer.indexOf('\r\n\r\n');
    if (headerEnd === -1) return;

    const header = buffer.slice(0, headerEnd).toString('utf8');
    const match = /Content-Length:\s*(\d+)/i.exec(header);
    if (!match) {
      buffer = buffer.slice(headerEnd + 4);
      continue;
    }

    const length = Number(match[1]);
    const bodyStart = headerEnd + 4;
    const bodyEnd = bodyStart + length;
    if (buffer.length < bodyEnd) return;

    const body = buffer.slice(bodyStart, bodyEnd).toString('utf8');
    buffer = buffer.slice(bodyEnd);
    handleRequest(JSON.parse(body));
  }
}

process.stdin.on('data', chunk => {
  buffer = Buffer.concat([buffer, chunk]);
  readMessages();
});

process.stdin.on('end', () => {
  process.exit(0);
});
