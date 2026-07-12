#!/usr/bin/env node
// Integrity audit for an rtk-gh (agent toolchain comparison) benchmark run.
// usage: node check-run-integrity.mjs <runDir>
// Verifies, per agent: commands.ndjson parses line-by-line; every step has a
// raw evidence file; logged byte counts reconcile with raw file sizes; no
// zero-byte successful steps; answers.md exists, covers all questions, and
// ends with a Totals section; and flags provider-truncation markers inside
// raw outputs (matchTruncated / incompleteResults) so scored answers can be
// audited for truncated evidence. Also flags missing/zero token counts (a
// run using the current run-step.mjs should have a `tokens` field on every
// research step) and mixed tokenizer methods within one run.
//
// Answer-section headers are matched at H2 OR H3 (`## Q<N>` or `### Q<N>`):
// prompt-template.md asks for H3, but this was flagged as a false failure
// against 3/6 real agents in run rtk-gh-vs-octocode-flows-20260712T070819Z
// who used H2 with fully complete, correctly-ordered content — the header
// level itself carries no signal about answer quality, so treat both as
// valid rather than blocking a run on solver Markdown-heading style.
import { readdirSync, readFileSync, existsSync, statSync } from 'node:fs';
import { join } from 'node:path';

const runDir = process.argv[2];
if (!runDir) {
  console.error('usage: node check-run-integrity.mjs <runDir> [--questions N]');
  process.exit(2);
}
const qFlagIdx = process.argv.indexOf('--questions');
const N_QUESTIONS =
  qFlagIdx >= 0 && process.argv[qFlagIdx + 1]
    ? Number(process.argv[qFlagIdx + 1])
    : 10;

const problems = [];
const notes = [];
const tokenizerMethodsSeen = new Set();
const agentsDir = join(runDir, 'agents');
const agents = readdirSync(agentsDir, { withFileTypes: true })
  .filter(d => d.isDirectory())
  .map(d => d.name)
  .sort();

// The wrapper writes stdout + optional "\n--- STDERR ---\n" + stderr to the
// raw file but logs bytes = stdout+stderr only, so the file may be up to the
// marker length larger than the logged bytes.
const STDERR_MARKER_SLACK = '\n--- STDERR ---\n'.length;

for (const agent of agents) {
  const dir = join(agentsDir, agent);
  const ndjsonPath = join(dir, 'commands.ndjson');
  if (!existsSync(ndjsonPath)) {
    problems.push(`${agent}: commands.ndjson MISSING`);
    continue;
  }
  const lines = readFileSync(ndjsonPath, 'utf8').trim().split('\n');
  const steps = [];
  lines.forEach((line, i) => {
    try {
      steps.push(JSON.parse(line));
    } catch {
      problems.push(`${agent}: commands.ndjson line ${i + 1} does not parse (truncated write?)`);
    }
  });

  const qSeen = new Set();
  let truncationMarkers = 0;
  let stepsWithoutTokens = 0;
  let stepsTotal = 0;
  // Steps sharing an id are retries. Legacy runs (pre suffix-preserving
  // wrapper) kept only the LAST attempt's raw file — reconcile bytes against
  // the last entry and note that earlier attempts' evidence was overwritten.
  const byId = new Map();
  for (const s of steps) {
    const list = byId.get(s.id) ?? [];
    list.push(s);
    byId.set(s.id, list);
  }
  for (const [id, attempts] of byId) {
    const q = (id.match(/^(q\d+)/) || [])[1];
    if (q) qSeen.add(q);
    const suffixed = attempts.map((s, i) => ({
      step: s,
      file:
        i === 0
          ? join(dir, 'raw', `${id}.txt`)
          : join(dir, 'raw', `${id}.${i + 1}.txt`),
    }));
    const legacyOverwrite =
      attempts.length > 1 && !existsSync(suffixed[1].file);
    if (legacyOverwrite) {
      notes.push(
        `${agent}/${id}: ${attempts.length} attempts logged under one id; legacy wrapper kept only the last attempt's raw output (metrics counted ALL attempts — nothing lost from measurements)`
      );
    }
    const checks = legacyOverwrite
      ? [{ step: attempts[attempts.length - 1], file: suffixed[0].file }]
      : suffixed;
    for (const { step: s, file: rawFile } of checks) {
      if (!existsSync(rawFile)) {
        problems.push(`${agent}/${s.id}: raw evidence file MISSING`);
        continue;
      }
      const size = statSync(rawFile).size;
      if (size < s.bytes || size > s.bytes + STDERR_MARKER_SLACK) {
        problems.push(
          `${agent}/${s.id}: raw size ${size} does not reconcile with logged bytes ${s.bytes} (possible truncation)`
        );
      }
      if (s.exit === 0 && s.bytes === 0) {
        notes.push(
          `${agent}/${s.id}: successful step with EMPTY output (verify intentional, e.g. --quiet zero rows)`
        );
      }
      stepsTotal += 1;
      if (s.tokens === undefined) {
        stepsWithoutTokens += 1;
      } else if (s.bytes > 0 && s.tokens === 0) {
        problems.push(`${agent}/${s.id}: non-empty output logged 0 tokens (tokenizer bug?)`);
      }
      if (s.tokenizer) tokenizerMethodsSeen.add(s.tokenizer);
      const raw = readFileSync(rawFile, 'utf8');
      if (
        /"?matchTruncated"?\s*[:=]\s*true|"?incompleteResults"?\s*[:=]\s*true|incomplete_results/i.test(
          raw
        )
      ) {
        truncationMarkers += 1;
        notes.push(
          `${agent}/${s.id}: provider reported TRUNCATED/INCOMPLETE results — check the answer did not rely on the missing tail`
        );
      }
    }
  }

  const answersPath = join(dir, 'answers.md');
  if (!existsSync(answersPath)) {
    problems.push(`${agent}: answers.md MISSING`);
  } else {
    const answers = readFileSync(answersPath, 'utf8');
    for (let n = 1; n <= N_QUESTIONS; n++) {
      if (!new RegExp(`^#{2,3}\\s*Q${n}\\b`, 'm').test(answers)) {
        problems.push(`${agent}: answers.md missing section Q${n}`);
      }
    }
    if (!/## Totals/.test(answers)) {
      problems.push(`${agent}: answers.md missing Totals section (truncated sheet?)`);
    }
    if (qSeen.size < N_QUESTIONS) {
      const missing = Array.from({ length: N_QUESTIONS }, (_, i) => `q${i + 1}`).filter(q => !qSeen.has(q));
      notes.push(`${agent}: no logged steps for ${missing.join(', ')} (answered from other questions' evidence?)`);
    }
  }
  if (stepsWithoutTokens > 0) {
    notes.push(
      `${agent}: ${stepsWithoutTokens}/${stepsTotal} step(s) have no 'tokens' field (pre-token-measurement run-step.mjs; aggregate.mjs backfills from raw/ at report time — re-run with the current wrapper for live token logging)`
    );
  }
  console.log(
    `${agent}: ${steps.length} steps parsed, ${qSeen.size}/${N_QUESTIONS} questions with logged evidence, ${truncationMarkers} provider-truncation markers`
  );
}

if (tokenizerMethodsSeen.size > 1) {
  problems.push(
    `mixed tokenizer methods logged within one run: ${[...tokenizerMethodsSeen].join(', ')} — token totals are not comparable across agents; re-run with one consistent environment`
  );
}

console.log('');
if (notes.length) {
  console.log('NOTES (review, not necessarily defects):');
  for (const n of notes) console.log(`  · ${n}`);
  console.log('');
}
if (problems.length) {
  console.log('PROBLEMS:');
  for (const p of problems) console.log(`  ✗ ${p}`);
  process.exit(1);
}
console.log('INTEGRITY OK — no truncated logs, missing evidence files, or incomplete answer sheets.');
