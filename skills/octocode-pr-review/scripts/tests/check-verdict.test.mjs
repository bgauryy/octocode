import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { checkVerdict } from '../check-verdict.mjs';

const SCRIPT = fileURLToPath(new URL('../check-verdict.mjs', import.meta.url));

const report = ({ status = 'APPROVE', findings, checks }) => `---
status: ${status}
question: Review PR 7
---

# PR 7: ${status}

## Answer

Verdict.

## Findings

${findings ?? '| ID | Location | Finding and evidence | Impact | Severity | Confidence | Fix |\n|---|---|---|---|---|---|---|\n| `Q-1` | `a.ts:3` | `suggestion (non-blocking):` rename | none | `Low` | `high` | rename |'}

\`\`\`md
## Checks
| Check | Result |
|---|---|
| ignored | failed |
\`\`\`

## Checks

${checks ?? '| Check | Result | Evidence |\n|---|---|---|\n| unit tests | `passed` | 120 of 120 |\n| e2e | skipped (not applicable) | no UI change |'}
`;

const blocker = '| ID | Location | Finding and evidence | Impact | Severity | Confidence | Fix |\n|---|---|---|---|---|---|---|\n| `SEC-1` | `src/auth.ts:42` | `issue (blocking, security):` token skips expiry | sessions never end | `High` | `high` | check exp |';
const failedCheck = '| Check | Result |\n|---|---|\n| unit tests | **failed** (2 of 120) |';

test('an approval with only non-blocking findings and passing checks is consistent', () => {
  const result = checkVerdict(report({}));
  assert.equal(result.ok, true);
  assert.deepEqual(result.blocking, []);
  assert.deepEqual(result.checks, { passed: 1, failed: 0, skipped: 1, unavailable: 0 });
});

test('APPROVE is rejected with a blocking finding or a failed check; other verdicts are not', () => {
  assert.deepEqual(checkVerdict(report({ findings: blocker })).conflicts, ['APPROVE with blocking finding SEC-1']);
  assert.deepEqual(checkVerdict(report({ checks: failedCheck })).conflicts, ['APPROVE with failed check unit tests']);
  for (const status of ['COMMENT', 'REQUEST_CHANGES']) {
    const result = checkVerdict(report({ status, findings: blocker, checks: failedCheck }));
    assert.equal(result.ok, true);
    assert.deepEqual(result.blocking, ['SEC-1']);
  }
});

test('"No findings" text needs no table', () => {
  assert.equal(checkVerdict(report({ findings: 'No findings in the files read.' })).ok, true);
});

test('reports the checker cannot read are invalid, not consistent', () => {
  for (const text of [
    report({}).replace('status: APPROVE\n', ''),
    report({ status: 'LGTM' }),
    report({}).replace('## Findings', '## Notes'),
    report({ checks: 'Ran the unit tests: all passed.' }),
    report({ checks: '| Check | Outcome |\n|---|---|\n| unit | passed |' }),
    report({ checks: '| Check | Result |\n|---|---|\n| unit | green |' }),
  ]) assert.throws(() => checkVerdict(text), { code: 'invalidReport' });
});

test('the CLI prints JSON and exits 0 consistent, 1 conflict, 2 invalid', t => {
  const dir = mkdtempSync(join(tmpdir(), 'check-verdict-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const run = text => {
    const file = join(dir, 'report.md');
    writeFileSync(file, text);
    const result = spawnSync(process.execPath, [SCRIPT, file], { encoding: 'utf8' });
    return { status: result.status, json: JSON.parse(result.stdout) };
  };
  assert.equal(run(report({})).status, 0);
  const conflict = run(report({ findings: blocker }));
  assert.deepEqual([conflict.status, conflict.json.ok, conflict.json.error.code], [1, false, 'verdictConflict']);
  assert.deepEqual(conflict.json.conflicts, ['APPROVE with blocking finding SEC-1']);
  const invalid = run(report({ status: 'LGTM' }));
  assert.deepEqual([invalid.status, invalid.json.error.code], [2, 'invalidReport']);
  const missing = spawnSync(process.execPath, [SCRIPT, join(dir, 'absent.md')], { encoding: 'utf8' });
  assert.deepEqual([missing.status, JSON.parse(missing.stdout).error.code], [2, 'unreadableReport']);
  const help = spawnSync(process.execPath, [SCRIPT, '--help'], { encoding: 'utf8' });
  assert.equal(help.status, 0);
  assert.match(help.stdout, /Exit codes/);
});
