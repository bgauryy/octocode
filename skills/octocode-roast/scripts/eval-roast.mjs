#!/usr/bin/env node
/**
 * Smoke eval for octocode-roast answers + description trigger self-check.
 *   node scripts/eval-roast.mjs --list | --self-test | --triggers
 *   node scripts/eval-roast.mjs --case <id> --input <answer.md> [--json]
 */
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const cases = JSON.parse(readFileSync(join(root, 'evals/cases.json'), 'utf8'));
const triggers = JSON.parse(readFileSync(join(root, 'evals/trigger-cases.json'), 'utf8'));
const skillMd = readFileSync(join(root, 'SKILL.md'), 'utf8');
const args = process.argv.slice(2);
const has = (f) => args.includes(f);
const arg = (f) => { const i = args.indexOf(f); return i >= 0 ? args[i + 1] : undefined; };

function desc() {
  const m = skillMd.match(/^description:\s*"(.*)"\s*$/m);
  return m ? m[1] : '';
}

function gradeTriggers() {
  const d = desc();
  const checks = [
    { name: 'Use when prefix', pass: /^Use when\b/.test(d) },
    { name: 'length <= 1024', pass: d.length <= 1024 && d.length > 40 },
    { name: 'roast/critique intent', pass: /\broast\b|\bcritique\b|\bsmell\b|\bdebt\b/i.test(d) },
    { name: 'anti research', pass: /octocode-research/i.test(d) },
    { name: 'trigger corpus', pass: (triggers.should_trigger?.length || 0) >= 8 },
    { name: 'anti-triggers', pass: (triggers.should_not_trigger?.length || 0) >= 5 },
    { name: 'answer cases', pass: (cases.cases?.length || 0) >= 4 },
  ];
  return { id: 'description-triggers', pass: checks.every((c) => c.pass), checks, description: d };
}

function test(text, item) { return new RegExp(item.pattern, 'im').test(text); }
function grade(c, text) {
  const checks = [];
  for (const item of c.required || []) checks.push({ name: item.name, pass: test(text, item) });
  for (const item of c.forbidden || []) checks.push({ name: `forbidden: ${item.name}`, pass: !test(text, item) });
  const score = checks.length ? checks.filter((x) => x.pass).length / checks.length : 1;
  return { id: c.id, pass: score >= (c.minScore ?? 0.8) && checks.every((x) => x.pass), score, checks };
}

if (has('--list')) {
  for (const c of cases.cases) console.log(`${c.id}\t${c.mode || '-'}\t${c.prompt}`);
  process.exit(0);
}
if (has('--triggers') || has('--self-test')) {
  const r = gradeTriggers();
  if (has('--json')) console.log(JSON.stringify(r, null, 2));
  else {
    console.log(`${r.pass ? 'PASS' : 'FAIL'} ${r.id}`);
    for (const ch of r.checks) console.log(`  ${ch.pass ? '✓' : '✗'} ${ch.name}`);
  }
  if (has('--self-test')) console.log(`eval-roast: ${cases.cases.length} cases`);
  process.exit(r.pass ? 0 : 1);
}
const id = arg('--case') || cases.cases[0]?.id;
const input = arg('--input');
if (!input) throw new Error('--input is required unless --list, --self-test, or --triggers');
const c = cases.cases.find((x) => x.id === id);
if (!c) throw new Error(`Unknown case: ${id}`);
const result = grade(c, readFileSync(resolve(process.cwd(), input), 'utf8'));
if (has('--json')) console.log(JSON.stringify(result, null, 2));
else {
  console.log(`${result.pass ? 'PASS' : 'FAIL'} ${result.id} score=${result.score.toFixed(2)}`);
  for (const ch of result.checks) console.log(`  ${ch.pass ? '✓' : '✗'} ${ch.name}`);
}
process.exit(result.pass ? 0 : 1);
