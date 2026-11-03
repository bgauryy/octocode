import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readdirSync, readFileSync, writeFileSync, existsSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';
import { build, COLLECTIONS, routeGraph, sharedAssets, sharedTargets } from '../build.mjs';
import { stageSkills } from '../../../packages/octocode/scripts/stage-skills.mjs';

const byText = (a, b) => a.localeCompare(b);

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'skills-common-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const collection of COLLECTIONS) {
    const dir = join(root, collection, 'example');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'SKILL.md'), '# Example\n[Render](scripts/render-report.mjs)\n[Output](output.md)\n');
    writeFileSync(join(dir, 'output.md'), '# Output\n[Shared](references/report-output.md)\n');
  }
  return root;
}

test('discovers consumers in every collection, checks without writing, and repairs missing or drifted assets', t => {
  const root = fixture(t);
  const first = build({ root, check: true });
  assert.equal(first.skills, COLLECTIONS.length);
  // A fresh install is missing every shared file it selects, identically in each collection.
  assert.equal(first.stale.length, first.copies);
  const perSkill = COLLECTIONS.map(collection =>
    first.stale.filter(file => file.startsWith(`${collection}/example/`)).map(file => file.slice(collection.length + 9)).toSorted(byText));
  assert.equal(perSkill.flat().length, first.stale.length);
  for (const files of perSkill) assert.deepEqual(files, perSkill[0]);
  for (const file of ['scripts/render-report.mjs', 'scripts/lib/entrypoint.mjs', 'references/report-output.md']) assert.ok(perSkill[0].includes(file), file);
  const renderer = join(root, 'skills/example/scripts/render-report.mjs');
  assert.equal(existsSync(renderer), false);
  build({ root });
  assert.deepEqual(build({ root, check: true }).stale, []);
  writeFileSync(renderer, '// drift\n');
  rmSync(join(root, 'skills-dev/example/references/report-output.md'));
  assert.equal(build({ root, check: true }).stale.length, 2);
  assert.equal(readFileSync(renderer, 'utf8'), '// drift\n');
  build({ root });
  assert.deepEqual(build({ root }).stale, []);
});

test('staged skills render independently and exclude the private source package', t => {
  const root = fixture(t);
  const source = join(root, 'skills');
  mkdirSync(join(source, 'common'));
  writeFileSync(join(source, 'common/package.json'), JSON.stringify({ private: true }));
  const target = join(root, 'installed');
  stageSkills(source, target);
  assert.equal(existsSync(join(target, 'common')), false);
  rmSync(source, { recursive: true });
  const markdown = join(root, 'report.md');
  const html = join(root, 'report.html');
  writeFileSync(markdown, '# Standalone\n\nEvidence survives installation.\n\n```mermaid\nflowchart LR\nA --> B\n```\n');
  const run = spawnSync(process.execPath, [join(target, 'example/scripts/render-report.mjs'), markdown, '--out', html], { encoding: 'utf8', cwd: root });
  assert.equal(run.status, 0, run.stderr);
  const page = readFileSync(html, 'utf8');
  assert.match(page, /Evidence survives installation/);
  assert.match(page, /<dialog id="modal"/);
  assert.match(page, /modal.showModal\(\)/);
  assert.match(page, /modalTrigger\?\.focus\(\)/);
  assert.match(page, /\.skip:focus/);
  assert.equal(existsSync(join(target, 'example/references/report-output.md')), true);
  assert.equal(existsSync(join(target, 'example/references/evidence.md')), true);
  assert.equal(existsSync(join(target, 'example/references/report-widgets.md')), true);
  assert.equal(existsSync(join(target, 'example/references/report-mermaid.md')), true);
  assert.equal(existsSync(join(target, 'example/references/protocol-catalog.md')), true);
  const link = join(root, 'render-link.mjs');
  symlinkSync(join(target, 'example/scripts/render-report.mjs'), link);
  const linked = spawnSync(process.execPath, [link, '--help'], { encoding: 'utf8', cwd: root });
  assert.equal(linked.status, 0, linked.stderr);
  assert.match(linked.stdout, /Usage:/);
});

test('a nested reference selects only its shared guide and its transitive dependencies on a fresh build', t => {
  const root = mkdtempSync(join(tmpdir(), 'skills-guidance-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const dir = join(root, 'skills-dev/example');
  mkdirSync(join(dir, 'references/nested'), { recursive: true });
  writeFileSync(join(dir, 'SKILL.md'), '# Example\n[Task](references/nested/task.md)\n');
  writeFileSync(join(dir, 'references/nested/task.md'), '[Tools](../octocode-usage.md#discover-and-invoke)\n');
  const expected = ['octocode-usage.md', 'configuration.md', 'evidence.md', 'protocol-catalog.md'].map(name => `skills-dev/example/references/${name}`);
  expected.push('skills-dev/example/scripts/lib/config-env.mjs');
  const first = build({ root, check: true });
  assert.deepEqual(first.stale.toSorted(byText), expected.toSorted(byText));
  assert.equal(existsSync(join(dir, 'references/octocode-usage.md')), false);
  build({ root });
  for (const path of expected) assert.equal(existsSync(join(root, path)), true);
  const canonical = sharedAssets(dir);
  assert.deepEqual([...canonical.keys()].toSorted(byText), expected.map(file => file.replace('skills-dev/example/', '')).toSorted(byText));
  for (const [target, content] of canonical) assert.deepEqual(readFileSync(join(dir, target)), content);

  assert.equal(existsSync(join(dir, 'scripts/render-report.mjs')), false);
  assert.deepEqual(build({ root, check: true }).stale, []);
  rmSync(join(dir, 'references/configuration.md'));
  assert.deepEqual(build({ root, check: true }).stale, ['skills-dev/example/references/configuration.md']);
});

test('architecture consumers get the canonical graph template without receiving an unused report renderer', t => {
  const root = mkdtempSync(join(tmpdir(), 'skills-graph-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const dir = join(root, 'skills-dev/graph');
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'SKILL.md'), '# Graph\nUses assets/template.html.\n');
  assert.equal(build({ root }).copies, 1);
  assert.match(readFileSync(join(dir, 'assets/template.html'), 'utf8'), /<!doctype html>/i);
  assert.equal(existsSync(join(dir, 'scripts/render-report.mjs')), false);
});

test('a shared file no route reaches is not copied, and a stale unreached copy is removed', t => {
  const root = fixture(t);
  const skill = join(root, 'skills/example');
  build({ root });
  const copy = join(skill, 'references/report-output.md');
  assert.equal(existsSync(copy), true);
  // Cut the only route to the page: the renderer stays, the unreached page goes.
  writeFileSync(join(skill, 'output.md'), '# Output\n');
  assert.ok(build({ root, check: true }).stale.includes('skills/example/references/report-output.md'));
  assert.equal(existsSync(copy), true, 'check never deletes');
  build({ root });
  assert.equal(existsSync(copy), false);
  assert.equal(existsSync(join(skill, 'references/evidence.md')), false, 'its dependencies go with it');
  assert.equal(existsSync(join(skill, 'scripts/render-report.mjs')), true);
  assert.deepEqual(build({ root, check: true }).stale.filter(file => file.startsWith('skills/')), []);
});

test('an unreached file at a shared path that is not a byte-identical copy is authored and kept', t => {
  const root = fixture(t);
  const skill = join(root, 'skills/example');
  writeFileSync(join(skill, 'output.md'), '# Output\n');
  mkdirSync(join(skill, 'references'), { recursive: true });
  writeFileSync(join(skill, 'references/report-output.md'), '# My own notes\n');
  build({ root, check: true });
  build({ root });
  assert.equal(readFileSync(join(skill, 'references/report-output.md'), 'utf8'), '# My own notes\n');
});

test('every installed skill carries exactly the shared files its routes reach', () => {
  const root = new URL('../../..', import.meta.url).pathname;
  const targets = sharedTargets();
  for (const collection of COLLECTIONS) {
    for (const name of readdirSync(join(root, collection))) {
      const dir = join(root, collection, name);
      if (!existsSync(join(dir, 'SKILL.md'))) continue;
      const wanted = sharedAssets(dir);
      const label = `${collection}/${name}`;
      for (const target of targets) {
        const installed = existsSync(join(dir, target));
        if (installed && !wanted.has(target)) assert.fail(`${label} ships ${target} but no route reaches it`);
        if (wanted.has(target)) assert.ok(installed, `${label} is missing ${target}`);
      }
      for (const link of routeGraph(dir).links) assert.ok(existsSync(join(dir, link)), `${label} routes to ${link} but it was not copied`);
    }
  }
});

test('network consumers restore a missing guard from its documented resource route', t => {
  const root = fixture(t);
  const skill = join(root, 'skills/example');
  writeFileSync(join(skill, 'SKILL.md'), '# Example\n[Network guard](scripts/lib/net-guard.mjs)\n');
  build({ root });
  const guard = join(skill, 'scripts/lib/net-guard.mjs');
  assert.match(readFileSync(guard, 'utf8'), /export function privateAddress/);
  rmSync(guard);
  assert.ok(build({ root, check: true }).stale.includes('skills/example/scripts/lib/net-guard.mjs'));
  build({ root });
  assert.equal(existsSync(guard), true);
});

test('provider configuration bundles the config owner into a standalone skill with process > workspace > home precedence', t => {
  const root = mkdtempSync(join(tmpdir(), 'skills-config-env-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const dir = join(root, 'skills/provider');
  mkdirSync(join(dir, 'references'), { recursive: true });
  writeFileSync(join(dir, 'SKILL.md'), '# Provider\n[Configuration](references/configuration.md)\n');
  writeFileSync(join(dir, 'references/configuration.md'), readFileSync(new URL('../references/configuration.md', import.meta.url)));

  build({ root });
  const loader = join(dir, 'scripts/lib/config-env.mjs');
  assert.equal(existsSync(loader), true);
  assert.doesNotMatch(readFileSync(loader, 'utf8'), /@octocodeai\/config/);

  const home = join(root, 'home');
  const workspace = join(root, 'workspace');
  mkdirSync(home, { recursive: true });
  mkdirSync(join(workspace, '.octocode'), { recursive: true });
  writeFileSync(join(home, '.env'), 'TAVILY_API_KEY=home-value\nEXA_API_KEY=home-exa\nSERPER_API_KEY=home-serper\n');
  writeFileSync(join(workspace, '.octocode/.env'), 'EXA_API_KEY=workspace-value\nSERPER_API_KEY=workspace-serper\n');
  const check = `import assert from 'node:assert/strict';\nconst { loadOctocodeEnv } = await import(${JSON.stringify(pathToFileURL(loader).href)});\nloadOctocodeEnv();\nassert.equal(process.env.TAVILY_API_KEY, 'home-value');\nassert.equal(process.env.EXA_API_KEY, 'workspace-value');\nassert.equal(process.env.SERPER_API_KEY, 'process-value');\n`;
  const result = spawnSync(process.execPath, ['--input-type=module', '--eval', check], {
    cwd: workspace,
    env: { PATH: process.env.PATH, OCTOCODE_HOME: home, SERPER_API_KEY: 'process-value' },
    encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, '');
  assert.equal(existsSync(join(root, 'node_modules')), false);
});
