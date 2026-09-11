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
  { name: 'root-help', args: ['--help'], maxBytes: 4500 },
  { name: 'tools-json', args: ['tools', '--json'], maxBytes: 6500 },
  { name: 'context-compact', args: ['context', '--compact'], maxBytes: 4100 },
  { name: 'context-minimal', args: ['context', '--minimal'], maxBytes: 1800 },
  {
    name: 'localSearch-compact-schema',
    args: ['tools', 'localSearch', '--scheme', '--json', '--compact'],
    maxBytes: 5000,
  },
  {
    name: 'default-tool-json',
    args: [
      'tools',
      'localSearch',
      '--path',
      sourceEntry,
      '--search-text',
      'runCLI',
      '--max-files',
      '1',
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
  const bytes = Buffer.byteLength(run.stdout || '', 'utf8');
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
  });
}

const noArgs = runCli([]);
const explicitHelp = runCli(['--help']);
results.push({
  name: 'default-equals-help',
  ok:
    noArgs.status === 0 &&
    explicitHelp.status === 0 &&
    noArgs.stdout === explicitHelp.stdout,
  status: noArgs.status,
  bytes: Buffer.byteLength(noArgs.stdout || '', 'utf8'),
  maxBytes: 4500,
});

const unknown = runCli(['definitely-not-a-command']);
results.push({
  name: 'unknown-command-exit',
  ok: unknown.status === 3,
  status: unknown.status,
  bytes: Buffer.byteLength(unknown.stdout || '', 'utf8'),
  maxBytes: 1000,
});

for (const r of results) {
  console.log(
    `${r.ok ? 'PASS' : 'FAIL'} ${r.name} bytes=${r.bytes}/${r.maxBytes} status=${r.status}${'jsonOk' in r ? ` json=${r.jsonOk}` : ''}`
  );
}
const failed = results.filter(r => !r.ok);
if (failed.length > 0) process.exit(1);
