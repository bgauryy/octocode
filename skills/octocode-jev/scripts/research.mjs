#!/usr/bin/env node
import { readFileSync, statSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { checkResearch, hashResearchRequest } from './check-research.mjs';

const launcher = fileURLToPath(new URL('./jev.mjs', import.meta.url));
const args = process.argv.slice(2);
if (args.length === 1 && ['--help', '-h'].includes(args[0])) {
  console.log('Usage: node scripts/research.mjs --input request.json [--timeout-ms N] [--retries N] [--project-env]\nEvaluates and checks one bound research request. JSON envelope on stdout; diagnostics on stderr.');
  process.exit(0);
}
const inputAt = args.indexOf('--input');
if (inputAt < 0 || !args[inputAt + 1]) {
  console.error(JSON.stringify({ error: 'Supply --input request.json; use --help.' }));
  process.exit(2);
}
try {
  if (statSync(args[inputAt + 1]).size > 4 * 1024 * 1024) throw new Error('size');
  const request = JSON.parse(readFileSync(args[inputAt + 1], 'utf8'));
  const child = spawnSync(process.execPath, [launcher, 'evaluate', ...args], { encoding: 'utf8', timeout: 305000, maxBuffer: 5 * 1024 * 1024 });
  if (child.status !== 0) {
    process.stderr.write(child.stderr || JSON.stringify({ error: 'Jev evaluation failed; no research advice is available.' }) + '\n');
    process.exit(child.status ?? 3);
  }
  const response = JSON.parse(child.stdout);
  const check = checkResearch(request, response);
  const envelope = { protocol: 'octocode-jev-research/v2', requestSha256: hashResearchRequest(request), response, check };
  console.log(JSON.stringify(envelope));
  process.exit(check.usable ? 0 : 4);
} catch {
  console.error(JSON.stringify({ error: 'Cannot evaluate research request. Supply readable valid JSON up to 4 MiB; use --help.' }));
  process.exit(2);
}
