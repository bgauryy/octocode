#!/usr/bin/env node

import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const defaultSkill = resolve(here, '..');

function usage() {
  console.log(`Usage:
  node scripts/eval-architect.mjs [--skill <dir>] [--json]
  node scripts/eval-architect.mjs --self-test [--json]`);
}

function parseArgs(argv) {
  const out = { skill: defaultSkill, json: false, selfTest: false };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--json') out.json = true;
    else if (arg === '--self-test') out.selfTest = true;
    else if (arg === '--skill' && argv[i + 1]) out.skill = resolve(argv[++i]);
    else if (arg === '--help' || arg === '-h') return { help: true };
    else throw new Error(`Unknown or incomplete argument: ${arg}`);
  }
  return out;
}

function loadCases(skill) {
  const parsed = JSON.parse(readFileSync(join(skill, 'evals', 'cases.json'), 'utf8'));
  if (!Array.isArray(parsed.cases) || parsed.cases.length === 0) {
    throw new Error('evals/cases.json must contain a non-empty cases array');
  }
  return parsed.cases;
}

function corpus(skill) {
  const refs = join(skill, 'references');
  const files = [
    join(skill, 'SKILL.md'),
    ...readdirSync(refs)
      .filter(name => name.endsWith('.md') && name !== 'references.md')
      .sort()
      .map(name => join(refs, name)),
  ];
  return files.map(file => readFileSync(file, 'utf8')).join('\n');
}

function grade(testCase, text) {
  const required = testCase.required.map(check => ({
    name: check.name,
    passed: new RegExp(check.pattern, 'is').test(text),
  }));
  const forbidden = (testCase.forbidden ?? []).map(check => ({
    name: check.name,
    passed: !new RegExp(check.pattern, 'is').test(text),
  }));
  const checks = [...required, ...forbidden];
  const score = checks.filter(check => check.passed).length / checks.length;
  return { id: testCase.id, score, passed: score === 1, required, forbidden };
}

function evaluate(cases, text) {
  const results = cases.map(testCase => grade(testCase, text));
  const passed = results.filter(result => result.passed).length;
  return { score: passed / results.length, passed, total: results.length, results };
}

function selfTest(cases) {
  for (const testCase of cases) {
    if (!testCase.referenceSolution || !grade(testCase, testCase.referenceSolution).passed) {
      throw new Error(`${testCase.id}: referenceSolution does not pass its grader`);
    }
  }
  return { passed: true, cases: cases.length };
}

try {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    usage();
    process.exit(0);
  }
  const cases = loadCases(args.skill);
  const result = args.selfTest ? selfTest(cases) : evaluate(cases, corpus(args.skill));
  console.log(args.json ? JSON.stringify(result, null, 2) : args.selfTest
    ? `self-test passed (${result.cases} cases)`
    : `score ${result.score} (${result.passed}/${result.total})`);
  if (!args.selfTest && result.score < 1) process.exitCode = 1;
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 2;
}
