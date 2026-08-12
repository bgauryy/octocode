#!/usr/bin/env node
/** Description + trigger-corpus self-check for octocode-research. */
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const d = (readFileSync(join(root, 'SKILL.md'), 'utf8').match(/^description:\s*"(.*)"\s*$/m) || [])[1] || '';
const triggers = JSON.parse(readFileSync(join(root, 'evals/trigger-cases.json'), 'utf8'));
const checks = [
  { name: 'Use when', pass: /^Use when\b/.test(d) },
  { name: 'length', pass: d.length <= 1024 && d.length > 40 },
  { name: 'evidence-before-assert', pass: /\bevidence\b/i.test(d) && /\bclaim\b|\bassert\b|\bchange it\b/i.test(d) },
  { name: 'skip known fix', pass: /\bSkip when the fix is already known\b|\balready known\b/i.test(d) },
  { name: 'anti roast/rfc/brainstorm', pass: /octocode-roast/i.test(d) && /octocode-rfc-generator/i.test(d) && /octocode-brainstorming/i.test(d) },
  { name: 'trigger corpus', pass: (triggers.should_trigger?.length || 0) >= 6 && (triggers.should_not_trigger?.length || 0) >= 4 },
];
const pass = checks.every((c) => c.pass);
console.log(`${pass ? 'PASS' : 'FAIL'} research-description`);
for (const c of checks) console.log(`  ${c.pass ? '✓' : '✗'} ${c.name}`);
process.exit(pass ? 0 : 1);
