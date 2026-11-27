import test from 'node:test';
import assert from 'node:assert/strict';
import { cpSync, mkdtempSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';

// Paths from this file, so the test runs from the repo root or its package.
const skillsRoot = resolve(import.meta.dirname, '../..');
const renderer = resolve(skillsRoot, 'common/scripts/render-report.mjs');
const today = new Date().toLocaleDateString('en-CA');
const listing = (dir) => readdirSync(dir, { recursive: true }).sort();

function workspace(t) {
  const root = mkdtempSync(join(tmpdir(), 'report-check-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  return root;
}

const report = (status, sections) => `---\nstatus: ${status}\nquestion: Q?\n---\n# Report\n\nAnswer.\n\n${sections.map((h) => `## ${h}\n\nEvidence.\n`).join('\n')}`;
const check = (script, file, root, ...extra) => spawnSync(process.execPath, [script, file, '--check', '--workspace', root, ...extra], { encoding: 'utf8' });

for (const entry of readdirSync(skillsRoot)) {
  let source;
  try { source = readFileSync(join(skillsRoot, entry, 'output.md'), 'utf8'); } catch { continue; }
  const match = source.match(/<!-- report-contract ([\s\S]*?) -->/);
  if (!match) continue;
  const contract = JSON.parse(match[1]);
  test(`${entry}: --check passes a contract-shaped report and rejects status, missing and out-of-order sections, writing nothing`, (t) => {
    const root = workspace(t), dir = join(root, '.octocode', entry), file = join(dir, `${today}-report.md`);
    mkdirSync(dir, { recursive: true });
    const sections = [...contract.required, ...contract.anyOf.map((g) => g[0])];
    writeFileSync(file, report(contract.statuses[0], sections));
    const before = listing(root);
    const passed = check(renderer, file, root, '--json');
    assert.equal(passed.status, 0, passed.stdout);
    assert.deepEqual(JSON.parse(passed.stdout), { validation: 'passed', files: 1, skills: [entry] });
    assert.deepEqual(listing(root), before, '--check writes no file');

    writeFileSync(file, report('__outside_contract__', sections));
    const status = check(renderer, file, root, '--json');
    assert.equal(status.status, 2);
    assert.ok(JSON.parse(status.stdout).warnings.some((w) => w.includes('__outside_contract__')));

    for (const missing of contract.required) {
      writeFileSync(file, report(contract.statuses[0], sections.filter((h) => h !== missing)));
      const run = check(renderer, file, root);
      assert.equal(run.status, 2, missing);
      assert.match(run.stdout, new RegExp(`^violation: .*## ${missing.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`, 'm'));
    }
    if (contract.required.length > 1) {
      writeFileSync(file, report(contract.statuses[0], [...contract.required].reverse()));
      const run = check(renderer, file, root, '--json');
      assert.equal(run.status, 2);
      assert.ok(JSON.parse(run.stdout).warnings.some((w) => /out of order/.test(w)));
    }
    assert.deepEqual(listing(root), before, 'a failed --check writes no file');
  });
}

test('a report saved outside .octocode/ answers to the skill whose script runs; one with no owner fails', (t) => {
  const root = workspace(t);
  // An installed copy: the renderer beside its own skill's output.md.
  const skill = join(root, 'skills', 'octocode-research');
  mkdirSync(skill, { recursive: true });
  cpSync(resolve(skillsRoot, 'common/scripts'), join(skill, 'scripts'), { recursive: true });
  writeFileSync(join(skill, 'output.md'), '<!-- report-contract {"statuses":["Confirmed"],"required":["Answer","Gaps"],"anyOf":[]} -->\n');
  const installed = join(skill, 'scripts', 'render-report.mjs');
  mkdirSync(join(root, 'docs'));
  const file = join(root, 'docs', 'answer.md');
  writeFileSync(file, report('Confirmed', ['Answer', 'Gaps']));
  const passed = check(installed, file, root, '--json');
  assert.equal(passed.status, 0, passed.stdout);
  assert.deepEqual(JSON.parse(passed.stdout).skills, ['octocode-research']);
  writeFileSync(file, report('Confirmed', ['Gaps', 'Answer']));
  assert.match(check(installed, file, root).stdout, /^violation: docs\/answer\.md: required sections are out of order/m);
  // skills/common belongs to no report skill, so it cannot check a report saved outside .octocode/.
  const orphan = check(renderer, file, root, '--json');
  assert.equal(orphan.status, 2);
  assert.match(JSON.parse(orphan.stdout).warnings[0], /no report skill owns this file/);
});

test('--check refuses output options and missing files with exit 1', (t) => {
  const root = workspace(t), file = join(root, 'r.md');
  writeFileSync(file, '# R\n');
  for (const extra of [['--out', join(root, 'r.html')], ['--manifest'], ['--open']]) {
    const run = check(renderer, file, root, ...extra);
    assert.equal(run.status, 1, extra.join(' '));
    assert.match(run.stderr, /--check writes nothing/);
  }
  assert.equal(check(renderer, join(root, 'missing.md'), root).status, 1);
  assert.deepEqual(readdirSync(root), ['r.md']);
});
