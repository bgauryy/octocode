#!/usr/bin/env node
import { readFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { once } from 'node:events';
import { isMainModule } from './lib/entrypoint.mjs';
import { formatResult, HINT, runQuery } from './query-api.mjs';
export * from './query-api.mjs';

function dataPath(arg) {
  const p = resolve(arg);
  if (!statSync(p, { throwIfNoEntry: false })?.isDirectory()) return p;
  const final = join(p, 'data.json');
  const checkpoint = join(p, 'stitched.json');
  return !statSync(final, { throwIfNoEntry: false }) && statSync(checkpoint, { throwIfNoEntry: false }) ? checkpoint : final;
}

async function main(argv) {
  const json = argv.includes('--json');
  const stdin = argv.includes('--stdin');
  const rest = argv.filter(a => a !== '--json' && a !== '--stdin');
  const help = argv.includes('--help') || argv.includes('-h');
  if ((stdin ? rest.length !== 1 : rest.length < 2) || help) {
    (help ? process.stdout : process.stderr).write(`usage: query.mjs <result-dir|data.json> ("<query>" | --stdin) [--json]\n  --stdin reads one query per line against the same saved snapshot. --json writes one result per line.\n  ${HINT}\n`);
    process.exitCode = help ? 0 : 2;
    return;
  }
  const D = JSON.parse(readFileSync(dataPath(rest[0]), 'utf8'));
  const queries = stdin ? createInterface({ input: process.stdin, crlfDelay: Infinity }) : [rest.slice(1).join(' ')];
  for await (const query of queries) {
    if (stdin && !query.trim()) continue;
    const r = runQuery(D, query);
    if (!process.stdout.write(`${json ? JSON.stringify(r) : formatResult(r)}\n`)) await once(process.stdout, 'drain');
    if (!r.ok) process.exitCode = 1;
  }
}

if (isMainModule(import.meta.url)) await main(process.argv.slice(2));
