import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { checkPlan } from '../plan-check.mjs';

const script = fileURLToPath(new URL('../plan-check.mjs', import.meta.url));

const fields = (priority, extra = '') => `Covers: login claim, src/login.ts:10
Priority/level: ${priority} unit
Given: a fresh user fixture
When: run \`npm test -- login\`
Then: exit code 0 and one session row
Pass if / Fail if: pass on exit 0 and one row; fail otherwise
Evidence to capture: test output and exit code
Cleanup: none, the fixture is in memory
${extra}`;

const plan = ({ features, cases, intro = '' }) => `---
status: Ready
question: "Test plan: login"
---
# Test plan: login

## Features

| Feature or claim | Source | Risk | Priority | Cases |
|---|---|---|---|---|
${features.join('\n')}

## Cases

${intro}
${cases.join('\n\n')}

## Results format

| ID | Result | Evidence | Notes |
|---|---|---|---|
| \`<ID>\` | \`pass/fail/blocked/skipped\` | \`<captured output/file/log>\` | \`<actual vs expected, or blocker>\` |
`;

const cleanPlan = plan({
  features: [
    '| Login succeeds | `src/login.ts:10` | Session missing | P0 | T1 |',
    '| Lockout | `docs/auth.md:4` | Lockout bypassed | P1 | T2, T3 |',
    '| SSO | `docs/sso.md:1` | Out of scope | P2 | Not tested: owned by the SSO team |',
  ],
  cases: [
    `### T1 — login succeeds\n\n${fields('P0', 'Discriminates: fails on the base revision before the fix')}`,
    `### T2 — lockout after five tries\n\n${fields('P1', 'Skip if: no lockout fixture is available')}`,
    `### T3: lockout clears\n\n| Field | Content |\n|---|---|\n| Covers | lockout claim |\n| Priority/level | P2 integration |\n| Given | a locked user |\n| When | wait for the window |\n| Then | login works |\n| Pass if / Fail if | pass on success |\n| Evidence to capture | log |\n| Cleanup | none |`,
  ],
});

const empty = (report) => Object.entries(report).filter(([, value]) => Array.isArray(value) && value.length);

test('a plan in the output.md format has no findings', () => {
  const report = checkPlan(cleanPlan);
  assert.deepEqual(empty(report), []);
  assert.equal(report.ok, true);
  assert.deepEqual(report.counts, { features: 3, cases: 3, p0: 1 });
});

test('each mechanical failure is reported against its case or feature', () => {
  const report = checkPlan(plan({
    features: [
      '| Login | `src/login.ts:10` | Session missing | P0 | T1 |',
      '| Logout | <path:line or spec> | Session kept | P1 | T9 |',
      '| Audit | `docs/audit.md:2` | Missing row | P1 | Root-owned gates |',
    ],
    cases: [
      `### T1 — login\n\n${fields('P0')}`,
      `### T1 — login again\n\n${fields('P1')}`,
      '### T2 — partial\n\nPriority/level: unit\nGiven: user\nWhen:\nThen: <result>',
    ],
  }));
  assert.equal(report.ok, false);
  assert.deepEqual(report.duplicateIds, ['T1']);
  assert.deepEqual(report.p0WithoutDiscriminates, ['T1']);
  assert.deepEqual(report.featuresWithoutSource, ['Logout']);
  assert.deepEqual(report.featuresWithoutCases, ['Logout', 'Audit']);
  assert.deepEqual(report.unknownCaseRefs, [{ feature: 'Logout', ref: 'T9' }]);
  assert.deepEqual(report.orphanCases, ['T2']);
  const t2 = report.missingFields.filter((item) => item.id === 'T2').map((item) => item.field);
  // Every required field except Given is absent, blank, a placeholder, or (priority) names no P0-P2.
  assert.deepEqual(t2, ['Covers', 'Priority/level', 'When', 'Then', 'Pass if / Fail if', 'Evidence to capture', 'Cleanup']);
});

test('ranges, shared intro fields, notes and fenced headings', () => {
  const report = checkPlan(plan({
    features: ['| All cases | `spec.md:1` | Any | P1 | C1–C3 |'],
    intro: 'Cleanup: none for read-only calls\nEvidence to capture: full output\n\n### Shared setup for C1–C3\n\nGiven: nothing here applies to cases\n',
    cases: [1, 2, 3].map((n) => `### C${n} — case ${n}\n\nCovers: x\nPriority/level: P1 unit\nGiven: g\nWhen: w\n\n\`\`\`sh\n# not a heading\nThen: not a field inside a fence\n\`\`\`\n\nThen: t\nPass if / Fail if: p`),
  }));
  assert.deepEqual(empty(report), []);
  assert.equal(report.counts.cases, 3);
});

test('results must give every case one allowed result, with evidence and permitted skips', () => {
  const results = `# Results\n\n| ID | Result | Evidence | Notes |\n|---|---|---|---|\n| T1 | pass | | |\n| T2 | skipped | none | no fixture |\n| T2 | pass | log | |\n| T4 | pass | log | |\n| Other | done | x | |\n`;
  const report = checkPlan(cleanPlan.replace('Skip if: no lockout fixture is available', ''), results).results;
  assert.deepEqual(report.missingResults, ['T3']);
  assert.deepEqual(report.duplicateResults, ['T2']);
  assert.deepEqual(report.skippedNotPermitted, ['T2']);
  assert.deepEqual(report.withoutEvidence, ['T1']);
  assert.deepEqual(report.unknownIds, ['T4', 'Other']);
  assert.deepEqual(report.invalidResults, []);
  const permitted = checkPlan(cleanPlan, '| ID | Result | Evidence |\n|---|---|---|\n| T1 | **PASS** | out |\n| T2 | skipped | |\n| T3 | pending | |\n').results;
  assert.deepEqual(permitted.skippedNotPermitted, []);
  assert.deepEqual(permitted.invalidResults, [{ id: 'T3', result: 'pending' }]);
  assert.deepEqual(checkPlan(cleanPlan, 'no table').results.structureErrors.length, 1);
});

test('a plan without the sections it reads reports structure errors', () => {
  const report = checkPlan('# Test plan: x\n\n## Answer\n\nNone.\n');
  assert.equal(report.ok, false);
  assert.equal(report.structureErrors.length, 2);
});

test('CLI: exit 0 clean, 1 findings, 2 invalid input, --help', () => {
  const dir = mkdtempSync(join(tmpdir(), 'plan-check-'));
  try {
    const clean = join(dir, 'clean.md');
    const broken = join(dir, 'broken.md');
    writeFileSync(clean, cleanPlan);
    writeFileSync(broken, cleanPlan.replace('Discriminates: fails on the base revision before the fix', ''));
    const run = (...args) => spawnSync(process.execPath, [script, ...args], { encoding: 'utf8' });
    const ok = run(clean);
    assert.equal(ok.status, 0);
    assert.equal(JSON.parse(ok.stdout).ok, true);
    const findings = run(broken);
    assert.equal(findings.status, 1);
    assert.deepEqual(JSON.parse(findings.stdout).p0WithoutDiscriminates, ['T1']);
    const results = join(dir, 'results.md');
    writeFileSync(results, '| ID | Result | Evidence |\n|---|---|---|\n| T1 | pass | out |\n| T2 | fail | out |\n| T3 | blocked | |\n');
    const withResults = run(clean, '--results', results);
    assert.equal(withResults.status, 0);
    assert.equal(JSON.parse(withResults.stdout).results.file, results);
    for (const args of [[], [join(dir, 'absent.md')], [clean, '--results'], [clean, '--bogus'], [clean, broken]]) {
      const bad = run(...args);
      assert.equal(bad.status, 2, args.join(' '));
      assert.equal(JSON.parse(bad.stdout).ok, false);
    }
    const help = run('--help');
    assert.equal(help.status, 0);
    assert.match(help.stdout, /Exit codes/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
