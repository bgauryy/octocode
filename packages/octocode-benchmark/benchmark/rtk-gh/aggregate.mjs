#!/usr/bin/env node
// Aggregate an rtk-gh (agent toolchain comparison) benchmark run into markdown tables.
// usage: node aggregate.mjs <runDir>
// Reads  <runDir>/agents/*/commands.ndjson  and optional
//   <runDir>/scores.json   ({ "<agent>": { "q1": 1, "q2": 0.5, ... } })      — correctness, 0-1 per sub-question
//   <runDir>/quality.json  ({ "<agent>": { "q1": 5, "q2": 4.5, ... } })      — depth-of-quality, 1-5 per question
// Prints markdown to stdout.
//
// Token usage is the PRIMARY cost metric (bytes is kept as a secondary/audit
// column). Commands logged by the current run-step.mjs already carry a
// `tokens` field; older runs logged before token-measurement was added are
// backfilled here by re-tokenizing the saved `raw/<id>.txt` evidence file —
// this makes the metric available retroactively without re-running agents,
// but it means bytes and tokens are the ONLY correctness-independent
// artifacts that can be recomputed after the fact; never edit answers.md or
// scores after backfilling.
import { readdirSync, readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { estimateTokens, tokenizerMethod, isRealTokenizer } from './token-estimate.mjs';

const runDir = process.argv[2];
if (!runDir) {
  console.error('usage: node aggregate.mjs <runDir>');
  process.exit(2);
}

const agentsDir = join(runDir, 'agents');
const scoresFile = join(runDir, 'scores.json');
const qualityFile = join(runDir, 'quality.json');
const scores = existsSync(scoresFile)
  ? JSON.parse(readFileSync(scoresFile, 'utf8'))
  : null;
const quality = existsSync(qualityFile)
  ? JSON.parse(readFileSync(qualityFile, 'utf8'))
  : null;

const agents = readdirSync(agentsDir, { withFileTypes: true })
  .filter(d => d.isDirectory())
  .map(d => d.name)
  .sort();

const armOf = name => name.replace(/-\d+$/, '');
let backfilledTokens = 0;
let loggedTokens = 0;

function rawFileFor(dir, cmd) {
  // run-step.mjs preserves retried attempts as <id>.2.txt, <id>.3.txt, ...
  // Aggregate reads whichever file exists for this exact ndjson line by
  // position, so retries are handled the same way check-run-integrity.mjs
  // handles them (see that script's `byId` grouping for the full rule).
  const plain = join(dir, 'raw', `${cmd.id}.txt`);
  return existsSync(plain) ? plain : null;
}

const rows = [];
for (const agent of agents) {
  const dir = join(agentsDir, agent);
  const ndjson = join(dir, 'commands.ndjson');
  const cmds = existsSync(ndjson)
    ? readFileSync(ndjson, 'utf8').trim().split('\n').filter(Boolean).map(l => JSON.parse(l))
    : [];
  const research = cmds.filter(c => !c.id.startsWith('smoke'));
  const perQ = {};
  for (const c of research) {
    const q = (c.id.match(/^(q\d+)/) || [])[1] || 'other';
    perQ[q] = perQ[q] || { steps: 0, bytes: 0, ms: 0, tokens: 0 };
    perQ[q].steps += 1;
    perQ[q].bytes += c.bytes;
    perQ[q].ms += c.ms;
    let tokens = c.tokens;
    if (tokens === undefined) {
      const rawFile = rawFileFor(dir, c);
      tokens = rawFile ? estimateTokens(readFileSync(rawFile, 'utf8')) : 0;
      backfilledTokens += 1;
    } else {
      loggedTokens += 1;
    }
    perQ[q].tokens += tokens;
    c._tokens = tokens;
  }
  const agentScores = scores ? scores[agent] || {} : null;
  const agentQuality = quality ? quality[agent] || {} : null;
  const qualityVals = agentQuality ? Object.values(agentQuality) : [];
  rows.push({
    agent,
    arm: armOf(agent),
    steps: research.length,
    bytes: research.reduce((a, c) => a + c.bytes, 0),
    tokens: research.reduce((a, c) => a + (c._tokens ?? 0), 0),
    ms: research.reduce((a, c) => a + c.ms, 0),
    correct: agentScores
      ? Object.values(agentScores).reduce((a, v) => a + v, 0)
      : null,
    meanQuality: qualityVals.length
      ? qualityVals.reduce((a, v) => a + v, 0) / qualityVals.length
      : null,
    perQ,
    agentScores,
    agentQuality,
  });
}

const kb = b => (b / 1024).toFixed(1);
const sec = ms => (ms / 1000).toFixed(1);
const tok = t => t.toFixed(0);

console.log(
  `_Token counts use \`${tokenizerMethod()}\`${isRealTokenizer() ? '' : ' (no real BPE tokenizer installed in this environment — approximation, see token-estimate.mjs)'}. ${loggedTokens} step(s) read tokens logged by run-step.mjs; ${backfilledTokens} step(s) backfilled from raw/ evidence at aggregate time._\n`
);

console.log('## Per-agent totals\n');
console.log('| Agent | Arm | Correct | Quality (1-5) | Steps | Tokens (est.) | KB consumed | Tool time (s) |');
console.log('|---|---|---:|---:|---:|---:|---:|---:|');
for (const r of rows) {
  console.log(
    `| ${r.agent} | ${r.arm} | ${r.correct ?? '—'} | ${r.meanQuality?.toFixed(1) ?? '—'} | ${r.steps} | ${tok(r.tokens)} | ${kb(r.bytes)} | ${sec(r.ms)} |`
  );
}

console.log('\n## Per-arm means\n');
console.log('| Arm | Agents | Correct | Quality (1-5) | Steps | Tokens (est.) | KB consumed | Tool time (s) | Tokens per correct |');
console.log('|---|---:|---:|---:|---:|---:|---:|---:|---:|');
const arms = [...new Set(rows.map(r => r.arm))];
for (const arm of arms) {
  const g = rows.filter(r => r.arm === arm);
  const mean = f => g.reduce((a, r) => a + f(r), 0) / g.length;
  const mc = scores ? mean(r => r.correct) : null;
  const mq = quality ? mean(r => r.meanQuality ?? 0) : null;
  const mt = mean(r => r.tokens);
  const mb = mean(r => r.bytes);
  console.log(
    `| ${arm} | ${g.length} | ${mc === null ? '—' : mc.toFixed(2)} | ${mq === null ? '—' : mq.toFixed(2)} | ${mean(r => r.steps).toFixed(1)} | ${tok(mt)} | ${kb(mb)} | ${sec(mean(r => r.ms))} | ${mc ? tok(mt / mc) : '—'} |`
  );
}

if (scores) {
  const qids = [...new Set(rows.flatMap(r => Object.keys(r.agentScores || {})))].sort(
    (a, b) => Number(a.slice(1)) - Number(b.slice(1))
  );
  console.log('\n## Per-question correctness matrix\n');
  console.log(`| Q | ${rows.map(r => r.agent).join(' | ')} |`);
  console.log(`|---|${rows.map(() => '---:').join('|')}|`);
  for (const q of qids) {
    console.log(`| ${q} | ${rows.map(r => r.agentScores?.[q] ?? '—').join(' | ')} |`);
  }
  console.log('\n## Per-question cost matrix (steps / tokens)\n');
  console.log(`| Q | ${rows.map(r => r.agent).join(' | ')} |`);
  console.log(`|---|${rows.map(() => '---:').join('|')}|`);
  for (const q of qids) {
    console.log(
      `| ${q} | ${rows.map(r => (r.perQ[q] ? `${r.perQ[q].steps} / ${tok(r.perQ[q].tokens)}` : '—')).join(' | ')} |`
    );
  }
}

if (quality) {
  const qids = [...new Set(rows.flatMap(r => Object.keys(r.agentQuality || {})))].sort(
    (a, b) => Number(a.slice(1)) - Number(b.slice(1))
  );
  console.log('\n## Per-question depth-of-quality matrix (1-5, independent of correctness)\n');
  console.log(`| Q | ${rows.map(r => r.agent).join(' | ')} |`);
  console.log(`|---|${rows.map(() => '---:').join('|')}|`);
  for (const q of qids) {
    console.log(`| ${q} | ${rows.map(r => r.agentQuality?.[q] ?? '—').join(' | ')} |`);
  }
}
