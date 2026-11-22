#!/usr/bin/env node
import { readFileSync, realpathSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { formatResult, HINT, runQuery } from './query-api.mjs';
export * from './query-api.mjs';

function dataPath(arg) {
  const p = resolve(arg);
  if (!statSync(p, { throwIfNoEntry: false })?.isDirectory()) return p;
  const final = join(p, 'data.json');
  const checkpoint = join(p, 'stitched.json');
  return !statSync(final, { throwIfNoEntry: false }) && statSync(checkpoint, { throwIfNoEntry: false }) ? checkpoint : final;
}

function main(argv) {
  const json = argv.includes('--json');
  const rest = argv.filter((a) => a !== '--json');
  if (rest.length < 2 || argv.includes('--help')) {
    process.stderr.write(`usage: query.mjs <result-dir|data.json> "<query>" [--json]\n  ${HINT}\n  Offline; with a running viewer, reply.mjs query <out> "<query>" also highlights the answer.\n`);
    process.exitCode = 2;
    return;
  }
  const D = JSON.parse(readFileSync(dataPath(rest[0]), 'utf8'));
  const r = runQuery(D, rest.slice(1).join(' '));
  process.stdout.write(`${json ? JSON.stringify(r) : formatResult(r)}\n`);
  if (!r.ok) process.exitCode = 1;
}

const isMain = () => {
  try { return Boolean(process.argv[1]) && realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url)); } catch { return false; }
};
if (isMain()) main(process.argv.slice(2));
