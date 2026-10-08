#!/usr/bin/env node
import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
import assert from 'node:assert/strict';
import { connectStdio } from '../dist/runtime.js';
const output = resolve(
  process.argv[2] ??
    '.octocode/benchmarks/chrome-persistent-client/results/2026-10-07-v3'
);
mkdirSync(output, { recursive: true });
const file = resolve(output, 'numeric-boundaries.json');
writeFileSync(
  file,
  '[{"id":"above","n":1.0000000000000001},{"id":"below","n":0.9999999999999999},{"id":"one","n":1},{"id":"huge","n":1e10000},{"id":"tiny","n":1e-10000}]'
);
const client = await connectStdio({
  command: process.execPath,
  args: [
    'packages/octocode-chrome-devtools/bin/octocode-chrome-devtools.mjs',
    '--preset',
    'research',
  ],
});
const records = [];
try {
  for (const [op, value, expected] of [
    ['eq', 1, ['one']],
    ['gte', 1, ['above', 'one', 'huge']],
    ['lte', 1, ['below', 'one', 'tiny']],
    ['eq', 0, []],
  ]) {
    const input = { file, where: [{ path: '/n', op, value }] };
    for (const surface of ['mcp', 'typed-cli']) {
      const start = performance.now();
      let response, raw;
      if (surface === 'mcp') {
        response = await client.callTool({ name: 'query', arguments: input });
        raw = JSON.stringify(response);
      } else {
        raw = execFileSync(
          process.execPath,
          [
            'packages/octocode-chrome-devtools/bin/octocode-chrome-devtools.mjs',
            '/cli',
            'query',
            '--file',
            file,
            '--where',
            JSON.stringify(input.where),
            '--json',
          ],
          { encoding: 'utf8', timeout: 10000 }
        );
        response = JSON.parse(raw);
      }
      const durationMs = performance.now() - start;
      let parsed = response.structuredContent;
      if (!parsed)
        for (const c of response.content ?? [])
          if (c.type === 'text')
            try {
              parsed = JSON.parse(c.text);
            } catch {}
      assert.equal(parsed.ok, true, raw);
      const page = parsed.data;
      const ids = page.rows.map(row => row.value.id);
      records.push({
        surface,
        input,
        raw,
        durationMs,
        chars: raw.length,
        ids,
        expected,
        scanned: page.scanned,
      });
      assert.deepEqual(ids, expected);
      assert.equal(page.scanned, 5);
    }
  }
} finally {
  await client.close();
  writeFileSync(
    resolve(output, 'numeric-boundary-results.json'),
    JSON.stringify(records, null, 2)
  );
}
console.log(
  'Eight exact numeric checks passed across MCP and typed CLI; graded IDs only.'
);
