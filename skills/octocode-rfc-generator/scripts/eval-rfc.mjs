#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const spec = JSON.parse(readFileSync(join(root, 'evals/cases.json'), 'utf8'));
const args = process.argv.slice(2);
const has = (flag) => args.includes(flag);
const arg = (flag) => { const i = args.indexOf(flag); return i >= 0 ? args[i + 1] : undefined; };
function countCitations(text) { return (text.match(/https?:\/\/[^\s)]+|(?:^|\s)[A-Za-z0-9_.\/-]+\.[A-Za-z0-9]+:\d+\b/g) || []).length; }
function test(text, item) { return new RegExp(item.pattern, 'im').test(text); }
function grade(c, text) {
  const checks = [];
  for (const item of c.required || []) checks.push({ name: item.name, pass: test(text, item) });
  for (const item of c.forbidden || []) checks.push({ name: `forbidden: ${item.name}`, pass: !test(text, item) });
  if (c.minCitationCount) checks.push({ name: `min citations ${c.minCitationCount}`, pass: countCitations(text) >= c.minCitationCount });
  const score = checks.length ? checks.filter((x) => x.pass).length / checks.length : 1;
  return { id: c.id, pass: score >= (c.minScore ?? 0.8) && checks.every((x) => x.pass), score, checks };
}
if (has('--list')) { for (const c of spec.cases) console.log(`${c.id}\t${c.mode}\t${c.prompt}`); process.exit(0); }
if (has('--self-test')) { console.log(`eval-rfc: ${spec.cases.length} cases loaded`); process.exit(spec.cases.length ? 0 : 1); }
const id = arg('--case') || spec.cases[0]?.id;
const input = arg('--input');
if (!input) throw new Error('--input is required unless --list or --self-test');
const c = spec.cases.find((x) => x.id === id);
if (!c) throw new Error(`Unknown case: ${id}`);
const result = grade(c, readFileSync(resolve(process.cwd(), input), 'utf8'));
if (has('--json')) console.log(JSON.stringify(result, null, 2));
else {
  console.log(`${result.pass ? 'PASS' : 'FAIL'} ${result.id} score=${result.score.toFixed(2)}`);
  for (const ch of result.checks) console.log(`  ${ch.pass ? '✓' : '✗'} ${ch.name}`);
}
process.exit(result.pass ? 0 : 1);
