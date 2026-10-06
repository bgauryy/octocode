// CommonJS importer recovery must look past unrelated lexical occurrences.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {
  ROOT,
  checks,
  rowData,
  startServer,
  writeResults,
} from './mcp-client.mjs';

const { check, summary } = checks('lsp-importers');
const base = path.join(os.homedir(), '.octocode', 'tmp');
fs.mkdirSync(base, { recursive: true });
const root = fs.mkdtempSync(path.join(base, 'lsp-importers-'));
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
    lineHint: 1,
    operation: 'references',
    includeDeclaration: false,
  });
  const data = rowData(entry);
  check(
    'reference request succeeds',
    !entry.isError && !entry.rowErrors,
    data?.errorCode ?? entry.text
  );
  const consumer = data?.payload?.files?.find(
    file => path.basename(file.path) === 'consumer.cjs'
  );
  check(
    'call after five comment mentions is included',
    consumer?.matches?.some(ref => typeof ref === 'string' && /^7:/.test(ref)),
    JSON.stringify(data?.payload)
  );
  check(
    'recovery records a verified importer',
    data?.payload?.coverage?.verifiedImporterFiles >= 1,
    JSON.stringify(data?.payload?.coverage)
  );
  const result = summary();
  writeResults('lsp-importers', { ...result, calls: client.log });
  process.exitCode = result.failed.length ? 1 : 0;
} finally {
  client?.close();
  fs.rmSync(root, { recursive: true, force: true });
}
