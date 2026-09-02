#!/usr/bin/env node

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const cases = JSON.parse(readFileSync(join(root, 'evals', 'cases.json'), 'utf8')).cases;

function matches(answer, pattern) {
  return new RegExp(pattern, 'i').test(answer);
}

function grade(testCase, answer) {
  const required = testCase.required.filter(item => matches(answer, item.pattern));
  const forbidden = testCase.forbidden.filter(item => matches(answer, item.pattern));
  return {
    id: testCase.id,
    score: required.length,
    minScore: testCase.minScore,
    missing: testCase.required.filter(item => !matches(answer, item.pattern)).map(item => item.id),
    forbidden: forbidden.map(item => item.id),
    pass:
      required.length >= testCase.minScore &&
      required.length === testCase.required.length &&
      forbidden.length === 0,
  };
}

function fail(message, code = 1) {
  console.error(message);
  process.exit(code);
}

const args = process.argv.slice(2);
if (args.includes('--self-test')) {
  const failures = [];
  for (const testCase of cases) {
    const positive = grade(testCase, testCase.referenceAnswer);
    const negative = grade(testCase, testCase.failureAnswer);
    if (!positive.pass) failures.push(`${testCase.id}: reference answer failed`);
    if (negative.pass) failures.push(`${testCase.id}: failure answer passed`);
  }
  if (failures.length > 0) fail(failures.join('\n'));
  console.log(`PASS ${cases.length}/${cases.length} architect behavior cases`);
  process.exit(0);
}

const caseIndex = args.indexOf('--case');
const answerIndex = args.indexOf('--answer');
if (caseIndex < 0 || !args[caseIndex + 1] || answerIndex < 0 || !args[answerIndex + 1]) {
  fail('Usage: eval-skill.mjs --self-test | --case <id> --answer <response.md>', 2);
}
const testCase = cases.find(item => item.id === args[caseIndex + 1]);
if (!testCase) fail(`Unknown case: ${args[caseIndex + 1]}`, 2);
const receipt = grade(testCase, readFileSync(args[answerIndex + 1], 'utf8'));
console.log(JSON.stringify(receipt, null, 2));
process.exit(receipt.pass ? 0 : 1);
