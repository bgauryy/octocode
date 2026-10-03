#!/usr/bin/env node
// Joins tasks.json with runs.jsonl -> results.json (+ prints markdown tables). No estimates: every number is a sum of recorded runs.
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
const here = dirname(fileURLToPath(import.meta.url));
const runs = readFileSync(join(here, 'runs.jsonl'), 'utf8').trim().split('\n').map((l) => JSON.parse(l));
const { tasks, edges } = JSON.parse(readFileSync(join(here, 'tasks.json'), 'utf8'));
const pick = (id, side, steps) => steps.map((s) => runs.find((r) => r.taskId === id && r.side === side && r.step === s)).filter(Boolean);
const sum = (rs) => ({ calls: rs.length, chars: rs.reduce((a, r) => a + r.readChars, 0), ms: rs.reduce((a, r) => a + r.ms, 0), exits: rs.map((r) => r.exit) });
const out = { generatedAt: new Date().toISOString(), tasks: [], edges: [], allSteps: {} };
for (const t of tasks) {
  const ghId = t.gh.ref ?? t.id;
  const g = sum(pick(ghId, 'gh', t.gh.steps));
  const o = sum(pick(t.oc.ref ?? t.id, 'oc', t.oc.steps));
  out.tasks.push({ ...t, ghMeasured: g, ocMeasured: o });
}
for (const e of edges) {
  const g = runs.filter((r) => r.taskId === e.id && r.side === 'gh');
  const o = runs.filter((r) => r.taskId === e.id && r.side === 'oc');
  out.edges.push({ ...e, gh: sum(g), oc: sum(o), ghRuns: g, ocRuns: o });
}
// naive-crawl total for g5full (every emitted continuation followed)
const all = runs.filter((r) => r.taskId === 'g5full' && r.side === 'oc');
out.allSteps.g5fullNaive = sum(all);
out.allSteps.st3 = sum(runs.filter((r) => r.taskId === 'st3' && r.side === 'oc'));
writeFileSync(join(here, 'results.json'), JSON.stringify({ ...out, runs }, null, 2));
const byTool = {};
for (const t of out.tasks) (byTool[t.tool] ??= []).push(t);
for (const [tool, ts] of Object.entries(byTool)) {
  console.log(`\n### ${tool}\n`);
  console.log('| id | task | gh chars/calls/ms | gh correct | oc chars/calls/ms | oc correct |');
  console.log('|---|---|---|---|---|---|');
  for (const t of ts) {
    const g = t.ghMeasured, o = t.ocMeasured;
    console.log(`| ${t.id} | ${t.task} | ${g.chars}/${g.calls}/${g.ms} | ${t.gh.correct ?? '(see ' + t.gh.ref + ')'} | ${o.chars}/${o.calls}/${o.ms} | ${t.oc.correct} |`);
  }
}
console.log('\n### edges\n');
for (const e of out.edges) console.log(`| ${e.id} | ${e.case} | gh ${e.gh.chars}/${e.gh.calls}/${e.gh.ms} exit ${e.gh.exits.join(',')} | oc ${e.oc.chars}/${e.oc.calls}/${e.oc.ms} exit ${e.oc.exits.join(',')} |`);
console.log('\nnaive g5full', out.allSteps.g5fullNaive, '\nst3 all', out.allSteps.st3);
const tot = (side) => out.tasks.filter((t) => !t.gh.ref || side === 'oc').reduce((a, t) => { const m = side === 'gh' ? t.ghMeasured : t.ocMeasured; a.chars += m.chars; a.calls += m.calls; a.ms += m.ms; return a; }, { chars: 0, calls: 0, ms: 0 });
console.log('\ntotals gh(excluding ref tasks)', tot('gh'), 'oc', tot('oc'));
