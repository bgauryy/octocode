// CommonJS importer recovery must look past unrelated lexical occurrences.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { ROOT } from './paths.mjs';
import { checks, startServer, writeResults, LIVE } from './mcp-client.mjs';
import { rowData, sourceLines } from './responses.mjs';

if (!LIVE) throw new Error('real LSP integration requires OCTOCODE_LIVE=1');
const { check, summary } = checks('lsp-importers');
const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-lsp-importers-')));
let client;
try {
  fs.mkdirSync(path.join(root, '.git'));
  fs.writeFileSync(
    path.join(root, 'package.json'),
    JSON.stringify({ name: 'lsp-importers-fixture', private: true })
  );
  const name = 'lateImporterTarget';
  const source = path.join(root, 'source.cjs');
  fs.writeFileSync(
    source,
    `function ${name}() { return 42; }\nmodule.exports = { ${name} };\n`
  );
  fs.writeFileSync(
    path.join(root, 'consumer.cjs'),
    `// ${name}\n// ${name}\n// ${name}\n// ${name}\n// ${name}\nconst { ${name} } = require('./source.cjs');\n${name}();\n`
  );
  client = await startServer({
    cwd: root,
    env: {
      WORKSPACE_ROOT: root,
      PATH:
        path.join(ROOT, 'node_modules', '.bin') +
        path.delimiter +
        process.env.PATH,
    },
    timeoutMs: 60_000,
  });
  const entry = await client.call('lspSearch', {
    path: source,
    symbolName: name,
    line: 1,
    operation: 'references',
    includeDeclaration: false,
    view: 'files',
  });
  const data = rowData(entry);
  check(
    'reference request succeeds',
    !entry.isError && !entry.rowErrors,
    data?.errorCode ?? entry.text
  );
  const consumer = data?.files?.find(
    file => path.basename(file.path) === 'consumer.cjs'
  );
  check(
    'call after five comment mentions is included',
    // A hit row is an object `{line, column, value}`, not a packed "7:…" string.
    consumer?.lines?.includes(7),
    JSON.stringify(data)
  );
  if (consumer) {
    const read = await client.call('localFetch', { path: consumer.path, ranges: [{ startLine: 7, endLine: 7 }] });
    check('returned reference coordinate reads the actual invocation', !read.isError && !read.rowErrors && sourceLines(rowData(read)).some(([line, text]) => line === 7 && text === 'lateImporterTarget();'), read.text);
  }
  check(
    'recovery records a complete scan with a verified importer',
    data?.coverage?.importerScan === 'complete' && data?.coverage?.verifiedImporterFiles >= 1,
    JSON.stringify(data?.coverage)
  );
  const result = summary();
  writeResults('lsp-importers', { ...result, calls: client.log });
  process.exitCode = result.failed.length ? 1 : 0;
} finally {
  client?.close();
  fs.rmSync(root, { recursive: true, force: true });
}
