import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { discoverSkillTests, skillTestCommand } from './lib/skill-tests.mjs';

test('discovers nested skill tests without absorbing workspace-owned or hidden tests', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-skill-tests-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const paths = [
    'skills/octocode-agentic-prompts/scripts/interview-grade.test.mjs',
    'skills/octocode-scraping/scripts/tests/provider-usage.test.mjs',
    'skills-beta/example/scripts/test.test.mjs',
    'skills-dev/octocode-context-audit/scripts/tests/collectors.test.mjs',
    'skills/common/tests/report-flow.test.mjs',
    'skills-dev/octocode-dev/scripts/skill-tests.test.mjs',
    'skills/example/.cache/ignored.test.mjs',
  ];
  for (const relativePath of paths) {
    const file = path.join(root, relativePath);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, '');
  }
  assert.deepEqual(discoverSkillTests(root), paths.slice(0, 4).map(relative => path.join(...relative.split('/'))).toSorted((a, b) => a.localeCompare(b)));
});

test('a missing optional skill root does not create an unmatched test pattern', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-skill-tests-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  assert.deepEqual(discoverSkillTests(root), []);
  assert.throws(() => skillTestCommand(root), /No standalone skill tests found/);
});

test('malformed roots fail instead of silently skipping tests', t => {
  assert.throws(() => discoverSkillTests(null), TypeError);
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-skill-tests-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.writeFileSync(path.join(root, 'skills'), 'not a directory');
  assert.throws(() => discoverSkillTests(root), { code: 'ENOTDIR' });
});

test('the constructed command executes only the discovered standalone suite', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-skill-tests-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const [relative, source] of [
    ['skills/example/scripts/selected.test.mjs', "import { test } from 'node:test'; test('selected', () => {});"],
    ['skills/common/tests/excluded.test.mjs', "import { test } from 'node:test'; test('excluded', () => { throw Error('should not run'); });"],
  ]) {
    const file = path.join(root, relative);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, source);
  }
  const [command, args] = skillTestCommand(root);
  assert.equal(command, 'node');
  assert.deepEqual(args, ['--test', path.join('skills', 'example', 'scripts', 'selected.test.mjs')]);
  const env = { ...process.env };
  delete env.NODE_TEST_CONTEXT;
  const result = spawnSync(process.execPath, args, { cwd: root, env, encoding: 'utf8', timeout: 10_000 });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, /selected/);
  assert.doesNotMatch(result.stdout, /excluded/);
});

test('an empty suite fails the skill task without blocking help', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-skill-tests-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const scripts = path.join(root, 'skills-dev', 'octocode-dev', 'scripts');
  fs.mkdirSync(path.join(scripts, 'lib'), { recursive: true });
  for (const relative of ['dev.mjs', 'lib/workspaces.mjs', 'lib/skill-tests.mjs']) {
    fs.copyFileSync(fileURLToPath(new URL(`./${relative}`, import.meta.url)), path.join(scripts, relative));
  }
  const run = task => spawnSync(process.execPath, [path.join(scripts, 'dev.mjs'), task], { cwd: root, encoding: 'utf8', timeout: 10_000 });
  assert.equal(run('--help').status, 0);
  for (const task of ['skills:check', 'test:ci']) {
    const result = run(task);
    assert.equal(result.status, 1, result.stdout + result.stderr);
    assert.match(result.stderr, /No standalone skill tests found; refusing bare node --test discovery/);
    assert.doesNotMatch(result.stdout, /TAP version/);
  }
});
