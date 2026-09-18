#!/usr/bin/env node
/**
 * bug-triage-v1 integrity inspector + result aggregator
 *
 * Usage:
 *   node inspect.mjs --check           verify completeness of all run artifacts (no write)
 *   node inspect.mjs --save            verify + write metrics.json
 *   node inspect.mjs --aggregate       de-blind grades + write RESULTS.md  (requires --save passing first)
 *   node inspect.mjs --summary         print current completion status
 */

import { readFileSync, writeFileSync, existsSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import { createHash } from 'crypto';

const __dir = dirname(fileURLToPath(import.meta.url));
const rel = (...p) => resolve(__dir, ...p);

const CASES = JSON.parse(readFileSync(rel('cases.json'), 'utf8')).cases;

// A/B assignment matches judge.mjs
function assignment(c) {
  const n = parseInt(c.id.replace('BUG-', ''), 10);
  return n % 2 === 1 ? { A: 'baseline', B: 'treatment' } : { A: 'treatment', B: 'baseline' };
}

function sha256file(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function tryReadJson(path) {
  if (!existsSync(path)) return null;
  try { return JSON.parse(readFileSync(path, 'utf8')); } catch { return null; }
}

// ── check one case arm ────────────────────────────────────────────────────────

function checkArm(c, arm) {
  const base = rel('runs', arm, c.id);
  const issues = [];

  const answerPath = `${base}/answer.md`;
  const resultPath = `${base}/result.json`;

  if (!existsSync(answerPath)) issues.push('answer.md missing');
  if (!existsSync(resultPath)) issues.push('result.json missing');

  const result = tryReadJson(resultPath);
  if (result) {
    if (result.arm !== arm) issues.push(`result.json arm mismatch: got ${result.arm}`);
    if (result.caseId !== c.id) issues.push(`result.json caseId mismatch`);
    if (typeof result.octocodeCalls !== 'number') issues.push('result.json missing octocodeCalls');
    if (result.octocodeCalls > 8) issues.push(`result.json octocodeCalls=${result.octocodeCalls} exceeds cap of 8`);

    if (arm === 'treatment') {
      if (!existsSync(`${base}/decision-before.json`)) issues.push('decision-before.json missing');
      if (!existsSync(`${base}/decision-after.json`)) issues.push('decision-after.json missing');
      if (!existsSync(`${base}/jev-calls.json`)) issues.push('jev-calls.json missing');

      const db = tryReadJson(`${base}/decision-before.json`);
      if (db && !db.gateClassification) issues.push('decision-before.json missing gateClassification');

      const da = tryReadJson(`${base}/decision-after.json`);
      if (da && typeof da.decisionChangedByJev !== 'boolean') issues.push('decision-after.json missing decisionChangedByJev');
    }
  }

  return { arm, caseId: c.id, complete: issues.length === 0, issues, result };
}

// ── check grade ───────────────────────────────────────────────────────────────

function checkGrade(c) {
  const p = rel('grades', c.id, 'grade.json');
  if (!existsSync(p)) return { caseId: c.id, graded: false };
  const g = tryReadJson(p);
  if (!g) return { caseId: c.id, graded: false, issues: ['grade.json parse error'] };
  const issues = [];
  for (const f of ['caseId','scoreA','scoreB','winner','majorFalseClaimA','majorFalseClaimB','jevChangeEvident']) {
    if (!(f in g)) issues.push(`missing ${f}`);
  }
  return { caseId: c.id, graded: issues.length === 0, grade: g, issues };
}

// ── aggregate metrics ─────────────────────────────────────────────────────────

function aggregateMetrics(rows) {
  const baseline = rows.map(r => r.baseline).filter(Boolean);
  const treatment = rows.map(r => r.treatment).filter(Boolean);

  const sum = (arr, key) => arr.reduce((a, x) => a + (x[key] ?? 0), 0);

  return {
    baseline: {
      completeCases: baseline.filter(x => x.complete).length,
      totalOctocodeCalls: sum(baseline.map(x => x.result).filter(Boolean), 'octocodeCalls'),
      totalOctocodeQueryRows: sum(baseline.map(x => x.result).filter(Boolean), 'octocodeQueryRows'),
      totalToolErrors: sum(baseline.map(x => x.result).filter(Boolean), 'toolErrorRows'),
    },
    treatment: {
      completeCases: treatment.filter(x => x.complete).length,
      totalOctocodeCalls: sum(treatment.map(x => x.result).filter(Boolean), 'octocodeCalls'),
      totalOctocodeQueryRows: sum(treatment.map(x => x.result).filter(Boolean), 'octocodeQueryRows'),
      totalToolErrors: sum(treatment.map(x => x.result).filter(Boolean), 'toolErrorRows'),
      totalJevCalls: sum(treatment.map(x => x.result).filter(Boolean), 'jevCalls'),
      totalJevInputTokens: sum(treatment.map(x => x.result).filter(Boolean), 'jevInputTokens'),
      totalJevOutputTokens: sum(treatment.map(x => x.result).filter(Boolean), 'jevOutputTokens'),
      totalJevLatencyMs: sum(treatment.map(x => x.result).filter(Boolean), 'jevLatencyMs'),
      jevCalledCases: treatment.map(x => x.result).filter(r => r?.jevCalled).length,
      decisionChangedCases: treatment.map(x => x.result).filter(r => r?.decisionChangedByJev).length,
      gateClassifications: {
        disputed_inference: treatment.map(x => x.result).filter(r => r?.gateClassification === 'disputed_inference').length,
        deterministic: treatment.map(x => x.result).filter(r => r?.gateClassification === 'deterministic').length,
        missing_fact: treatment.map(x => x.result).filter(r => r?.gateClassification === 'missing_fact').length,
      },
    },
  };
}

// ── de-blind and score ────────────────────────────────────────────────────────

function deblind(grades) {
  return grades.map(g => {
    if (!g.graded) return g;
    const c = CASES.find(x => x.id === g.caseId);
    const { A, B } = assignment(c);
    const baselineScore = A === 'baseline' ? g.grade.scoreA : g.grade.scoreB;
    const treatmentScore = A === 'treatment' ? g.grade.scoreA : g.grade.scoreB;
    const baselineFalseClaim = A === 'baseline' ? g.grade.majorFalseClaimA : g.grade.majorFalseClaimB;
    const treatmentFalseClaim = A === 'treatment' ? g.grade.majorFalseClaimA : g.grade.majorFalseClaimB;
    const winner = g.grade.winner === 'A' ? A : g.grade.winner === 'B' ? B : 'tie';
    return {
      ...g,
      baselineScore,
      treatmentScore,
      delta: treatmentScore - baselineScore,
      winner,
      baselineFalseClaim,
      treatmentFalseClaim,
      jevChangeEvident: g.grade.jevChangeEvident,
    };
  });
}

// ── RESULTS.md writer ─────────────────────────────────────────────────────────

function writeResults(rows, grades, metrics) {
  const scored = grades.filter(g => g.graded);
  const totalCases = CASES.length;
  const gradedCases = scored.length;

  const meanBaseline = scored.length ? (scored.reduce((a, g) => a + g.baselineScore, 0) / scored.length).toFixed(2) : 'n/a';
  const meanTreatment = scored.length ? (scored.reduce((a, g) => a + g.treatmentScore, 0) / scored.length).toFixed(2) : 'n/a';
  const meanDelta = scored.length ? (scored.reduce((a, g) => a + g.delta, 0) / scored.length).toFixed(2) : 'n/a';
  const treatmentWins = scored.filter(g => g.winner === 'treatment').length;
  const baselineWins = scored.filter(g => g.winner === 'baseline').length;
  const ties = scored.filter(g => g.winner === 'tie').length;
  const jevChanges = scored.filter(g => g.jevChangeEvident).length;
  const baselineFalseClaims = scored.filter(g => g.baselineFalseClaim).length;
  const treatmentFalseClaims = scored.filter(g => g.treatmentFalseClaim).length;

  const verdictLine = () => {
    if (gradedCases < totalCases) return '**INCOMPLETE — grading not finished**';
    if (treatmentWins >= 6 && treatmentFalseClaims === 0) return '**ACCEPT — replicate**';
    if (treatmentWins >= 3) return '**CONTINUE — adjust**';
    return '**REJECT — do not route by default**';
  };

  const perCase = scored.map(g => {
    const c = CASES.find(x => x.id === g.caseId);
    return `| ${g.caseId} | ${c?.repo.split('/')[1]} | ${g.baselineScore} | ${g.treatmentScore} | ${g.delta >= 0 ? '+' : ''}${g.delta} | ${g.winner} | ${g.jevChangeEvident ? 'yes' : 'no'} | ${g.baselineFalseClaim ? '⚠' : '—'} | ${g.treatmentFalseClaim ? '⚠' : '—'} |`;
  }).join('\n');

  const m = metrics;
  const t = m.treatment;
  const b = m.baseline;

  const md = `# Bug-Triage Benchmark — Results
**Suite:** bug-triage-v1  
**Generated:** ${new Date().toISOString()}  
**Cases graded:** ${gradedCases} / ${totalCases}

## Verdict
${verdictLine()}

## Aggregate quality

| Metric | Baseline | Treatment | Delta |
|---|---:|---:|---:|
| Mean score /10 | ${meanBaseline} | ${meanTreatment} | ${meanDelta} |
| Cases won | ${baselineWins} | ${treatmentWins} | — |
| Ties | — | ${ties} | — |
| Major false claims | ${baselineFalseClaims} | ${treatmentFalseClaims} | — |
| Jev-change evident (judge) | — | ${jevChanges} | — |

## Per-case scores

| Case | Repo | Baseline | Treatment | Δ | Winner | Jev changed? | Baseline FC? | Treatment FC? |
|---|---|---:|---:|---:|---|---|---|---|
${perCase}

## Flow and token metrics

| Metric | Baseline | Treatment |
|---|---:|---:|
| Complete cases | ${b.completeCases} | ${t.completeCases} |
| Octocode calls | ${b.totalOctocodeCalls} | ${t.totalOctocodeCalls} |
| Octocode query rows | ${b.totalOctocodeQueryRows} | ${t.totalOctocodeQueryRows} |
| Tool error rows | ${b.totalToolErrors} | ${t.totalToolErrors} |
| Jev calls | — | ${t.totalJevCalls ?? 0} |
| Cases where Jev called | — | ${t.jevCalledCases ?? 0} |
| Cases where Jev changed decision | — | ${t.decisionChangedCases ?? 0} |
| Jev input tokens | — | ${t.totalJevInputTokens ?? 0} |
| Jev output tokens | — | ${t.totalJevOutputTokens ?? 0} |
| Summed Jev latency (ms) | — | ${t.totalJevLatencyMs ?? 0} |

## Gate classification (treatment arm)

| Classification | Cases |
|---|---:|
| \`disputed_inference\` (Jev eligible) | ${t.gateClassifications?.disputed_inference ?? 0} |
| \`deterministic\` (no Jev) | ${t.gateClassifications?.deterministic ?? 0} |
| \`missing_fact\` (no Jev) | ${t.gateClassifications?.missing_fact ?? 0} |

## Limitations
- One run per arm; no statistical significance.
- Judge model shares training with worker models; possible correlated bias.
- Public issues may be partially in model training data.
- Host LLM tokens not captured (Octocode calls and Jev tokens measured only).
- A single positive pilot does not justify default Jev routing — only optional gate-routed calls.
`;

  writeFileSync(rel('RESULTS.md'), md);
  console.log('RESULTS.md written');
}

// ── CLI ──────────────────────────────────────────────────────────────────────

const args = process.argv.slice(2);

if (args[0] === '--summary') {
  console.log('Bug-triage-v1 completion status\n');
  for (const c of CASES) {
    const b = checkArm(c, 'baseline');
    const t = checkArm(c, 'treatment');
    const g = checkGrade(c);
    const bIcon = b.complete ? '✓' : '✗';
    const tIcon = t.complete ? '✓' : '✗';
    const gIcon = g.graded ? '✓' : '○';
    console.log(`${c.id}  baseline:${bIcon}  treatment:${tIcon}  grade:${gIcon}`);
    if (!b.complete) b.issues.forEach(i => console.log(`       baseline: ${i}`));
    if (!t.complete) t.issues.forEach(i => console.log(`       treatment: ${i}`));
  }
} else if (args[0] === '--check' || args[0] === '--save') {
  const rows = CASES.map(c => ({
    caseId: c.id,
    baseline: checkArm(c, 'baseline'),
    treatment: checkArm(c, 'treatment'),
  }));

  let allOk = true;
  for (const row of rows) {
    const b = row.baseline;
    const t = row.treatment;
    const ok = b.complete && t.complete;
    console.log(`${row.caseId}  ${ok ? '✓' : '✗'}`);
    if (!b.complete) b.issues.forEach(i => console.log(`  baseline: ${i}`));
    if (!t.complete) t.issues.forEach(i => console.log(`  treatment: ${i}`));
    if (!ok) allOk = false;
  }

  if (args[0] === '--save') {
    const metrics = {
      suiteId: 'bug-triage-v1',
      generatedAt: new Date().toISOString(),
      integrityValid: existsSync(rel('frozen.json')),
      allComplete: allOk,
      ...aggregateMetrics(rows),
      rows: rows.map(r => ({
        caseId: r.caseId,
        baselineComplete: r.baseline.complete,
        treatmentComplete: r.treatment.complete,
        baselineIssues: r.baseline.issues,
        treatmentIssues: r.treatment.issues,
        baselineResult: r.baseline.result,
        treatmentResult: r.treatment.result,
      })),
    };
    writeFileSync(rel('metrics.json'), JSON.stringify(metrics, null, 2));
    console.log('\nmetrics.json written');
  }

  if (!allOk) process.exit(1);
} else if (args[0] === '--aggregate') {
  const metricsPath = rel('metrics.json');
  if (!existsSync(metricsPath)) {
    console.error('Run --save first to generate metrics.json');
    process.exit(1);
  }
  const metrics = JSON.parse(readFileSync(metricsPath, 'utf8'));
  const rows = CASES.map(c => ({
    caseId: c.id,
    baseline: checkArm(c, 'baseline'),
    treatment: checkArm(c, 'treatment'),
  }));
  const rawGrades = CASES.map(checkGrade);
  const grades = deblind(rawGrades);
  writeResults(rows, grades, metrics);
} else {
  console.log(`bug-triage-v1 inspector

Commands:
  --summary        Show completion status for all cases (no write)
  --check          Validate all run artifacts (no write, exits 1 on failure)
  --save           Validate + write metrics.json
  --aggregate      De-blind grades + write RESULTS.md (requires metrics.json)
`);
}
