/**
 * Parity corpus entry: `localSearch` (S7).
 *
 * Runs localSearch on both the reference (TS tools-core) and native Rust
 * runtimes against an identical temp workspace and byte-compares sanitized
 * response envelopes.
 *
 * Gate: this file must be green before `localSearch` is promoted to
 * native-default in the CLI/MCP delegation tables.
 *
 * Usage (typically invoked from the vitest-managed harness runner or CI):
 *   node local-search.mjs <reference-server> <native-server> <addon> <regex-worker>
 */

import assert from 'node:assert/strict';
import process from 'node:process';
import { writeFile, mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { Harness } from './harness.mjs';

const [referenceServer, nativeServer, addon, regexWorker] = process.argv.slice(2);
assert.ok(
  referenceServer && nativeServer && addon && regexWorker,
  'usage: node local-search.mjs <reference> <native> <addon> <regex-worker>'
);

const harness = await Harness.connect({ reference: referenceServer, native: nativeServer, addon, regexWorker });

try {
  // --- Self-validation (required by S6) ---
  await harness.selfTest();
  console.log('  ✓ selfTest: harness correctly detects diffs');

  // --- Fixture setup ---
  const ws = harness.workspaceDir;
  await mkdir(join(ws, 'src'), { recursive: true });
  await writeFile(join(ws, 'src', 'alpha.ts'), `
export function greet(name: string): string {
  return \`Hello, \${name}\`;
}
export const GREETING_TOKEN = 'hello-world';
`);
  await writeFile(join(ws, 'src', 'beta.ts'), `
import { greet } from './alpha.js';
const result = greet('octocode');
export { result };
`);
  await writeFile(join(ws, 'README.md'), `# Octocode test fixture\nGREETING_TOKEN is defined in src/alpha.ts\n`);

  // --- Test 1: lexical search for a unique token ---
  await harness.testTool('localSearch', {
    queries: [{ path: ws, searchText: 'GREETING_TOKEN', regex: 'literal' }],
  });
  console.log('  ✓ lexical search: GREETING_TOKEN');

  // --- Test 2: regex search ---
  await harness.testTool('localSearch', {
    queries: [{ path: ws, searchText: 'greet\\(', regex: 'rust' }],
  });
  console.log('  ✓ regex search: greet(');

  // --- Test 3: search with file extension filter ---
  await harness.testTool('localSearch', {
    queries: [{ path: ws, searchText: 'greet', include: ['*.ts'] }],
  });
  console.log('  ✓ filtered search: *.ts');

  // --- Test 4: no results (absent token) ---
  await harness.testTool('localSearch', {
    queries: [{ path: ws, searchText: 'xyzzy_no_such_token_12345' }],
  });
  console.log('  ✓ empty search: no results');

  // --- Test 5: invalid input (missing required field) — error envelope parity ---
  await harness.testTool('localSearch', {
    // Missing `path` → both should return a validation error envelope
    queries: [{ searchText: 'anything' }],
  });
  console.log('  ✓ invalid input: error envelope parity');

  // --- Test 6: executable pagination covers every fixture exactly once ---
  for (let index = 0; index < 7; index += 1) {
    await harness.writeFixture(
      `paged/result-${index}.txt`,
      `PAGE_UNION_TOKEN ${index}\n`
    );
  }
  const pagination = await harness.testPagination('localSearch', {
    queries: [{
      path: ws,
      searchText: 'PAGE_UNION_TOKEN',
      resultView: 'paginated',
      pageSize: 3,
    }],
  });
  const files = pagination.reference.flatMap(page =>
    page.structuredContent.results[0].data.files.map(file => file.path)
  );
  assert.equal(files.length, 7, 'pagination union must return all seven fixtures');
  assert.equal(new Set(files).size, 7, 'pagination union must not duplicate fixtures');
  console.log('  ✓ executable pagination union: 7/7 unique files');

  console.log(JSON.stringify({ tool: 'localSearch', tests: 6, status: 'covered' }));
} finally {
  await harness.close();
}
