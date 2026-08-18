#!/usr/bin/env node
/** Description contract self-check for octocode-research. */
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const args = process.argv.slice(2);
if (args.includes('--help')) {
  console.log(`check-description — description contract gate for octocode-research

  node scripts/check-description.mjs [--json]

  --json   machine-readable results
  --help   this text

Asserts the SKILL.md description leads with "Use when", stays inside the length window,
promises evidence, keeps the skip clause, keeps the explicit trigger phrases, and hands off
to at least one named sibling skill. Exit 1 on any failed check.`);
  process.exit(0);
}
const json = args.includes('--json');

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const d = (readFileSync(join(root, 'SKILL.md'), 'utf8').match(/^description:\s*"(.*)"\s*$/m) || [])[1] || '';
const checks = [
  { name: 'Use when', pass: /^Use when\b/.test(d) },
  { name: 'length', pass: d.length <= 1024 && d.length > 40 },
  { name: 'evidence-before-assert', pass: /\bevidence\b/i.test(d) && /\bclaim\b|\bassert\b|\bchange it\b/i.test(d) },
  { name: 'skip known fix', pass: /\bSkip when the fix is already known\b|\balready known\b/i.test(d) },
  { name: 'explicit triggers', pass: /research this/i.test(d) && /use octocode/i.test(d) },
  { name: 'boundary handoff', pass: /\bnot for\b/i.test(d) && /→\s*octocode-[a-z-]+/.test(d) },
];
const pass = checks.every((c) => c.pass);
if (json) {
  console.log(JSON.stringify({ pass, length: d.length, checks }, null, 2));
} else {
  console.log(`${pass ? 'PASS' : 'FAIL'} research-description`);
  for (const c of checks) console.log(`  ${c.pass ? '✓' : '✗'} ${c.name}`);
}
process.exit(pass ? 0 : 1);
