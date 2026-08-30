#!/usr/bin/env node
/** Description + trigger-corpus self-check for octocode-eval-benchmark. */
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const skill = readFileSync(join(root, 'SKILL.md'), 'utf8');
const name = (skill.match(/^name:\s*(\S+)\s*$/m) || [])[1] || '';
const d = (skill.match(/^description:\s*"(.*)"\s*$/m) || [])[1] || '';
const triggers = JSON.parse(readFileSync(join(root, 'evals/trigger-cases.json'), 'utf8'));
const checks = [
  { name: 'Use when', pass: /^Use when\b/.test(d) },
  { name: 'eval-benchmark identity', pass: name === 'octocode-eval-benchmark' },
  { name: 'length', pass: d.length <= 1024 && d.length > 40 },
  { name: 'eval / benchmark intent', pass: /eval|benchmark/i.test(d) },
  { name: 'KPI / keep-discard', pass: /KPI|keep\/discard|held-out|sensor/i.test(d) },
  { name: 'anti ordinary ship', pass: /Not for ordinary ship|tests passed/i.test(d) },
  { name: 'anti code-graph collision', pass: !/dependency graph|import cycle|code graph/i.test(d) },
  { name: 'trigger corpus', pass: (triggers.should_trigger?.length || 0) >= 6 && (triggers.should_not_trigger?.length || 0) >= 4 },
];
const pass = checks.every((c) => c.pass);
console.log(`${pass ? 'PASS' : 'FAIL'} eval-benchmark-description`);
for (const c of checks) console.log(`  ${c.pass ? '✓' : '✗'} ${c.name}`);
process.exit(pass ? 0 : 1);
