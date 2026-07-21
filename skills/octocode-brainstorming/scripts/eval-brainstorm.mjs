#!/usr/bin/env node
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const spec = JSON.parse(readFileSync(join(root, 'evals/cases.json'), 'utf8'));
const args = process.argv.slice(2);
const has = (flag) => args.includes(flag);
const arg = (flag) => { const i = args.indexOf(flag); return i >= 0 ? args[i + 1] : undefined; };

function stripAnsi(s) { return s.replace(/\u001b\[[0-9;]*[a-zA-Z]/g, ''); }
function countCitations(text) {
  const urls = text.match(/https?:\/\/[^\s)]+/g) || [];
  const files = text.match(/(?:^|\s)([A-Za-z0-9_.\/-]+\.[A-Za-z0-9]+):(\d+)\b/g) || [];
  return urls.length + files.length;
}
function resolvesFileLines(text) {
  const failures = [];
  const rx = /(?:^|\s)([A-Za-z0-9_.\/-]+\.[A-Za-z0-9]+):(\d+)\b/g;
  let m;
  while ((m = rx.exec(text))) {
    const rel = m[1];
    const line = Number(m[2]);
    const p = resolve(process.cwd(), rel);
    if (!existsSync(p)) { failures.push(`${rel}:${line} missing file`); continue; }
    const total = readFileSync(p, 'utf8').split(/\r?\n/).length;
    if (line < 1 || line > total) failures.push(`${rel}:${line} out of bounds (${total})`);
  }
  return failures;
}
function checkPattern(text, item) {
  const rx = new RegExp(item.pattern, 'im');
  return rx.test(text);
}
function gradeCase(c, text) {
  const checks = [];
  for (const item of c.required || []) checks.push({ name: item.name, pass: checkPattern(text, item), required: true });
  for (const item of c.forbidden || []) checks.push({ name: `forbidden: ${item.name}`, pass: !checkPattern(text, item), required: true });
  for (const q of c.binaryQuestions || []) {
    const pass = new RegExp(q.passPattern, 'im').test(text) && !(q.failPattern && new RegExp(q.failPattern, 'im').test(text));
    checks.push({ name: q.id, pass, required: false });
  }
  const citationCount = countCitations(text);
  if (c.minCitationCount) checks.push({ name: `min citations ${c.minCitationCount}`, pass: citationCount >= c.minCitationCount, required: true });
  const fileLineFailures = resolvesFileLines(text);
  checks.push({ name: 'cited file:line references resolve', pass: fileLineFailures.length === 0, detail: fileLineFailures.join('; '), required: true });
  checks.push({ name: 'closes with Sources/Resources section', pass: /##\s*(Sources|Resources)\b[\s\S]*$/i.test(text), required: true });
  const passed = checks.filter((x) => x.pass).length;
  const score = checks.length ? passed / checks.length : 1;
  const pass = score >= (c.minScore ?? 0.8) && checks.filter((x) => x.required).every((x) => x.pass);
  return { id: c.id, score, pass, citationCount, checks };
}

if (has('--list')) {
  for (const c of spec.cases) console.log(`${c.id}\t${c.mode}\t${c.prompt}`);
  process.exit(0);
}
if (has('--self-test')) {
  console.log(`eval-brainstorm: ${spec.cases.length} cases loaded`);
  process.exit(spec.cases.length ? 0 : 1);
}
const id = arg('--case') || spec.cases[0]?.id;
const input = arg('--input');
const c = spec.cases.find((x) => x.id === id);
if (!c) throw new Error(`Unknown case: ${id}`);
if (!input) throw new Error('--input is required unless --list or --self-test');
const text = stripAnsi(readFileSync(resolve(process.cwd(), input), 'utf8'));
const result = gradeCase(c, text);
if (has('--json')) console.log(JSON.stringify(result, null, 2));
else {
  console.log(`${result.pass ? 'PASS' : 'FAIL'} ${result.id} score=${result.score.toFixed(2)} citations=${result.citationCount}`);
  for (const ch of result.checks) console.log(`  ${ch.pass ? '✓' : '✗'} ${ch.name}${ch.detail ? ` — ${ch.detail}` : ''}`);
}
process.exit(result.pass ? 0 : 1);
