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
import { Harness, CORPUS } from './harness.mjs';

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
    path: ws,
    searchText: 'GREETING_TOKEN',
  });
  console.log('  ✓ lexical search: GREETING_TOKEN');

  // --- Test 2: regex search ---
  await harness.testTool('localSearch', {
    path: ws,
    searchText: 'greet\\(',
    isRegex: true,
  });
  console.log('  ✓ regex search: greet(');

  // --- Test 3: search with file extension filter ---
  await harness.testTool('localSearch', {
    path: ws,
    searchText: 'greet',
    filePattern: '*.ts',
  });
  console.log('  ✓ filtered search: *.ts');

  // --- Test 4: no results (absent token) ---
  await harness.testTool('localSearch', {
    path: ws,
    searchText: 'xyzzy_no_such_token_12345',
  });
  console.log('  ✓ empty search: no results');

  // --- Test 5: invalid input (missing required field) — error envelope parity ---
  await harness.testTool('localSearch', {
    // Missing `path` → both should return a validation error envelope
    searchText: 'anything',
  });
  console.log('  ✓ invalid input: error envelope parity');

  // Mark corpus entry as covered.
  CORPUS.localSearch.status = 'covered';
  console.log(JSON.stringify({ tool: 'localSearch', tests: 5, status: 'covered' }));
} finally {
  await harness.close();
}
