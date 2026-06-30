import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import {
  MANAGED_BLOCK_END,
  MANAGED_BLOCK_START,
  formatStatus,
  getAssetPaths,
  listBundledSkills,
  mergeManagedAppendSystem,
  parseSetupScope,
  shouldAppendSystemPrompt,
  splitArgs,
} from '../src/index.js';

const packageRoot = path.resolve(import.meta.dirname, '..');
const distDir = path.join(packageRoot, 'dist');

test('build copies the canonical system prompt', () => {
  const paths = getAssetPaths(distDir);
  const sourcePrompt = path.join(packageRoot, 'docs', 'PI', 'APPEND_SYSTEM.md');
  assert.equal(fs.existsSync(paths.systemPrompt), true);
  assert.equal(fs.readFileSync(paths.systemPrompt, 'utf8'), fs.readFileSync(sourcePrompt, 'utf8'));

  const prompt = fs.readFileSync(paths.systemPrompt, 'utf8');
  assert.match(prompt, /<context_management>/);
  assert.match(prompt, /<verification>/);
});

test('build copies bundled Octocode skills without secret env files', () => {
  const skills = listBundledSkills(distDir);
  const sourceSkills = listBundledSkills(packageRoot);
  const rootSkills = listBundledSkills(path.resolve(packageRoot, '../..'));
  assert.deepEqual(skills, sourceSkills);
  assert.deepEqual(sourceSkills, rootSkills);
  assert.deepEqual(
    skills,
    [
      'octocode',
      'octocode-awareness',
      'octocode-brainstorming',
      'octocode-research',
      'octocode-rfc-generator',
      'octocode-roast',
      'octocode-skills',
      'octocode-stats',
    ].sort()
  );

  const forbiddenEnv = path.join(distDir, 'skills', 'octocode-brainstorming', '.env');
  const allowedExample = path.join(distDir, 'skills', 'octocode-brainstorming', '.env.example');
  assert.equal(fs.existsSync(forbiddenEnv), false);
  assert.equal(fs.existsSync(allowedExample), true);
});

test('managed APPEND_SYSTEM block is inserted and replaced without duplication', () => {
  const first = mergeManagedAppendSystem('local rules\n', 'old octocode rules');
  assert.match(first, new RegExp(MANAGED_BLOCK_START));
  assert.match(first, new RegExp(MANAGED_BLOCK_END));

  const second = mergeManagedAppendSystem(first, 'new octocode rules');
  assert.equal(second.match(new RegExp(MANAGED_BLOCK_START, 'g'))?.length, 1);
  assert.match(second, /new octocode rules/);
  assert.doesNotMatch(second, /old octocode rules/);
});

test('argument parsing supports setup scopes and quoted installer args', () => {
  assert.equal(parseSetupScope('--global'), 'global');
  assert.equal(parseSetupScope('global'), 'global');
  assert.equal(parseSetupScope(''), 'project');
  assert.deepEqual(splitArgs('--ide "VS Code" --scope user'), ['--ide', 'VS Code', '--scope', 'user']);
});

test('system prompt append guard detects existing prompt', () => {
  const prompt = '<system_prompt>\nabc\n</system_prompt>';
  assert.equal(shouldAppendSystemPrompt('', prompt), true);
  assert.equal(shouldAppendSystemPrompt(prompt, prompt), false);
});

test('status reports the dist assets', () => {
  const status = formatStatus(distDir);
  assert.match(status, /system prompt: found/);
  assert.match(status, /octocode-research/);
});
