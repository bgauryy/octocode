#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const cli = resolve(here, '../out/octocode.js');
if (!existsSync(cli)) {
  console.error(`Missing built CLI: ${cli}`);
  console.error('Run: yarn workspace octocode build:dev');
  process.exit(2);
}

const cases = [
  { name: 'root-help', args: ['--help'], maxBytes: 4500 },
  { name: 'tools-json', args: ['tools', '--json'], maxBytes: 6500 },
  { name: 'context-compact', args: ['context', '--compact'], maxBytes: 4000 },
  { name: 'context-minimal', args: ['context', '--minimal'], maxBytes: 1800 },
  {
    name: 'localSearch-compact-schema',
    args: ['tools', 'localSearch', '--scheme', '--json', '--compact'],
    maxBytes: 5000,
  },
];

const results = [];
for (const item of cases) {
  const run = spawnSync(process.execPath, [cli, ...item.args], {
    encoding: 'utf8',
    env: {
      ...process.env,
      OCTOCODE_NO_STALE_BUILD_WARNING: '1',
      NO_COLOR: '1',
    },
    timeout: 60_000,
    maxBuffer: 10 * 1024 * 1024,
  });
  const bytes = Buffer.byteLength(run.stdout || '', 'utf8');
  const ok = run.status === 0 && bytes <= item.maxBytes;
  results.push({
    name: item.name,
    ok,
    status: run.status,
    bytes,
    maxBytes: item.maxBytes,
  });
}

for (const r of results) {
  console.log(
    `${r.ok ? 'PASS' : 'FAIL'} ${r.name} bytes=${r.bytes}/${r.maxBytes} status=${r.status}`
  );
}
const failed = results.filter(r => !r.ok);
if (failed.length > 0) process.exit(1);
