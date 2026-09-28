#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const cli = resolve(here, '../out/octocode.js');
const sourceEntry = resolve(here, '../src/index.ts');
if (!existsSync(cli)) {
  console.error(`Missing built CLI: ${cli}`);
  console.error('Run: yarn workspace octocode build:dev');
  process.exit(2);
}

const cases = [
  { name: 'root-help', args: ['--help'], maxBytes: 4500, instructions: 'text' },
  { name: 'scheme-catalog', args: ['scheme', '--compact'], maxBytes: 7500, instructions: 'json' },
  {
    name: 'localSearch-compact-schema',
    args: ['scheme', 'localSearch', '--view', 'query', '--compact'],
    maxBytes: 14000,
  },
  {
    name: 'default-tool-json',
    args: [
      'localSearch',
      JSON.stringify({
        path: sourceEntry,
        searchText: 'runCLI',
        regex: 'literal',
        resultView: 'files',
        pageSize: 1,
        reasoning: 'Exercise the direct localSearch CLI within the output budget.',
      }),
    ],
    maxBytes: 5000,
    validJson: true,
  },
];

function runCli(args) {
  return spawnSync(process.execPath, [cli, ...args], {
    encoding: 'utf8',
    env: {
      ...process.env,
      OCTOCODE_NO_STALE_BUILD_WARNING: '1',
      NO_COLOR: '1',
    },
    timeout: 60_000,
    maxBuffer: 10 * 1024 * 1024,
  });
}

const results = [];
for (const item of cases) {
  const run = runCli(item.args);
  // Core owns the canonical instruction block. Keep the existing CLI presentation
  // budgets, accounting for that shared block separately instead of raising limits.
  let measured = run.stdout || '';
  let instructionBytes = 0;
  if (item.instructions === 'text') {
    const marker = '\nAgent instructions:\n';
    const split = measured.indexOf(marker);
    if (split >= 0) {
      instructionBytes = Buffer.byteLength(measured.slice(split + marker.length), 'utf8');
      measured = measured.slice(0, split);
    }
  } else if (item.instructions === 'json' && run.status === 0) {
    const catalog = JSON.parse(measured);
    instructionBytes = Buffer.byteLength(catalog.instructions ?? '', 'utf8');
    delete catalog.instructions;
    measured = JSON.stringify(catalog);
  }
  const bytes = Buffer.byteLength(measured, 'utf8');
  let jsonOk = true;
  if (item.validJson) {
    try {
      JSON.parse(run.stdout);
    } catch {
      jsonOk = false;
    }
  }
  const ok = run.status === 0 && bytes <= item.maxBytes && jsonOk;
  results.push({
    name: item.name,
    ok,
    status: run.status,
    bytes,
    maxBytes: item.maxBytes,
    ...(item.validJson ? { jsonOk } : {}),
    ...(item.instructions ? { instructionBytes } : {}),
  });
}

const noArgs = runCli([]);
let defaultCatalog;
try { defaultCatalog = JSON.parse(noArgs.stdout); } catch { /* reported below */ }
results.push({
  name: 'default-shows-catalog',
  ok: noArgs.status === 0 && defaultCatalog?.kind === 'octocode.toolCatalog',
  status: noArgs.status,
  bytes: Buffer.byteLength(noArgs.stdout || '', 'utf8'),
});

const unknown = runCli(['definitely-not-a-command']);
results.push({
  name: 'unknown-command-exit',
  ok: unknown.status === 2,
  status: unknown.status,
  bytes: Buffer.byteLength(unknown.stderr || '', 'utf8'),
  maxBytes: 1000,
});

for (const r of results) {
  console.log(
    `${r.ok ? 'PASS' : 'FAIL'} ${r.name} bytes=${r.bytes}${r.maxBytes ? `/${r.maxBytes}` : ''} status=${r.status}${'instructionBytes' in r ? ` coreInstructions=${r.instructionBytes}` : ''}${'jsonOk' in r ? ` json=${r.jsonOk}` : ''}`
  );
}
const failed = results.filter(r => !r.ok);
if (failed.length > 0) process.exit(1);
