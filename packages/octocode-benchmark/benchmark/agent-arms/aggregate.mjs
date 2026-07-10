#!/usr/bin/env node
// Aggregate an agent-arms benchmark run into markdown tables.
// usage: node aggregate.mjs <runDir>
// Reads  <runDir>/agents/*/commands.ndjson  and optional <runDir>/scores.json
// ({ "<agent>": { "q1": 1, "q2": 0.5, ... } }), prints markdown to stdout.
import { readdirSync, readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const runDir = process.argv[2];
if (!runDir) {
  console.error('usage: node aggregate.mjs <runDir>');
  process.exit(2);
}

const agentsDir = join(runDir, 'agents');
const scoresFile = join(runDir, 'scores.json');
const scores = existsSync(scoresFile)
  ? JSON.parse(readFileSync(scoresFile, 'utf8'))
  : null;

const agents = readdirSync(agentsDir, { withFileTypes: true })
  .filter(d => d.isDirectory())
  .map(d => d.name)
  .sort();

const armOf = name => name.replace(/-\d+$/, '');
const rows = [];
for (const agent of agents) {
  const ndjson = join(agentsDir, agent, 'commands.ndjson');
  const cmds = existsSync(ndjson)
    ? readFileSync(ndjson, 'utf8').trim().split('\n').filter(Boolean).map(l => JSON.parse(l))
    : [];
  const research = cmds.filter(c => !c.id.startsWith('smoke'));
  const perQ = {};
  for (const c of research) {
    const q = (c.id.match(/^(q\d+)/) || [])[1] || 'other';
    perQ[q] = perQ[q] || { steps: 0, bytes: 0, ms: 0 };
    perQ[q].steps += 1;
    perQ[q].bytes += c.bytes;
    perQ[q].ms += c.ms;
  }
  const agentScores = scores ? scores[agent] || {} : null;
  rows.push({
    agent,
    arm: armOf(agent),
    steps: research.length,
    bytes: research.reduce((a, c) => a + c.bytes, 0),
    ms: research.reduce((a, c) => a + c.ms, 0),
    correct: agentScores
      ? Object.values(agentScores).reduce((a, v) => a + v, 0)
      : null,
    perQ,
    agentScores,
  });
}

const kb = b => (b / 1024).toFixed(1);
const sec = ms => (ms / 1000).toFixed(1);

console.log('## Per-agent totals\n');
console.log('| Agent | Arm | Correct | Steps | KB consumed | Tool time (s) |');
console.log('|---|---|---:|---:|---:|---:|');
for (const r of rows) {
  console.log(
    `| ${r.agent} | ${r.arm} | ${r.correct ?? '—'} | ${r.steps} | ${kb(r.bytes)} | ${sec(r.ms)} |`
  );
}

console.log('\n## Per-arm means\n');
console.log('| Arm | Agents | Correct | Steps | KB consumed | Tool time (s) | KB per correct |');
console.log('|---|---:|---:|---:|---:|---:|---:|');
const arms = [...new Set(rows.map(r => r.arm))];
for (const arm of arms) {
  const g = rows.filter(r => r.arm === arm);
  const mean = f => g.reduce((a, r) => a + f(r), 0) / g.length;
  const mc = scores ? mean(r => r.correct) : null;
  const mb = mean(r => r.bytes);
  console.log(
    `| ${arm} | ${g.length} | ${mc === null ? '—' : mc.toFixed(2)} | ${mean(r => r.steps).toFixed(1)} | ${kb(mb)} | ${sec(mean(r => r.ms))} | ${mc ? kb(mb / mc) : '—'} |`
  );
}

if (scores) {
  const qids = [...new Set(rows.flatMap(r => Object.keys(r.agentScores || {})))].sort(
    (a, b) => Number(a.slice(1)) - Number(b.slice(1))
  );
  console.log('\n## Per-question score matrix\n');
  console.log(`| Q | ${rows.map(r => r.agent).join(' | ')} |`);
  console.log(`|---|${rows.map(() => '---:').join('|')}|`);
  for (const q of qids) {
    console.log(`| ${q} | ${rows.map(r => r.agentScores?.[q] ?? '—').join(' | ')} |`);
  }
  console.log('\n## Per-question cost matrix (steps / KB)\n');
  console.log(`| Q | ${rows.map(r => r.agent).join(' | ')} |`);
  console.log(`|---|${rows.map(() => '---:').join('|')}|`);
  for (const q of qids) {
    console.log(
      `| ${q} | ${rows.map(r => (r.perQ[q] ? `${r.perQ[q].steps} / ${kb(r.perQ[q].bytes)}` : '—')).join(' | ')} |`
    );
  }
}
