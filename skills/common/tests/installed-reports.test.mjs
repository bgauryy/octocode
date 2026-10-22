import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readdirSync, readFileSync, writeFileSync, mkdirSync, rmSync, existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { stageSkills } from '../../../packages/octocode/scripts/stage-skills.mjs';

test('every installed report skill renders, discovers and rejects invalid reports from an unrelated working directory', t => {
  const root = mkdtempSync(join(tmpdir(), 'installed-reports-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const installed = join(root, 'installed');
  stageSkills(resolve(import.meta.dirname, '../..'), installed);
  assert.equal(existsSync(join(installed, 'common')), false);
  const consumers = readdirSync(installed).filter(name => existsSync(join(installed, name, 'scripts/render-report.mjs')));
  assert.ok(consumers.length > 0);
  for (const name of consumers) {
    const contract = JSON.parse(readFileSync(join(installed, name, 'output.md'), 'utf8').match(/<!-- report-contract (.*?) -->/)[1]);
    const folder = join(root, '.octocode', name);
    mkdirSync(folder, { recursive: true });
    const smokeFile = new Date().toLocaleDateString('en-CA') + '-smoke.md';
    const input = join(folder, smokeFile), out = join(folder, 'smoke.html');
    const sections = [...contract.required, ...contract.anyOf.map(group => group[0])];
    const content = `---\nstatus: ${contract.statuses[0]}\nquestion: Does the installed output pipeline work?\n---\n# ${name}\n\nSynthetic pipeline fixture; not a domain-workflow result.\n\n` + sections.map(h => `## ${h}\n\nFixture evidence.\n`).join('\n') + '\n```mermaid\nflowchart LR\nA[Markdown] --> B[HTML]\n```\n';
    const invoke = args => spawnSync(process.execPath, [join(installed, name, 'scripts/render-report.mjs'), ...args], { cwd: root, encoding: 'utf8', timeout: 15000 });
    writeFileSync(input, content);
    const render = invoke([input, '--out', out, '--json', '--strict']);
    assert.equal(render.status, 0, `${name}: ${render.stderr || render.stdout}`);
    assert.equal(JSON.parse(render.stdout).render, 'unverified');
    const html = readFileSync(out, 'utf8');
    assert.ok(html.includes('id="verify-render"'), name);
    const listing = invoke([folder, '--manifest', '--all']);
    assert.equal(listing.status, 0, name);
    assert.ok(listing.stdout.includes(smokeFile), name);
    writeFileSync(input, content.replace(/^status: .*$/m, 'status: __outside_contract__'));
    assert.equal(invoke([input, '--out', out, '--json', '--strict']).status, 2, name);
    assert.equal(readFileSync(out, 'utf8'), html, `${name}: invalid input overwrote valid HTML`);
  }
});
