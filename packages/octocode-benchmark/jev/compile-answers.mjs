#!/usr/bin/env node
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(fileURLToPath(import.meta.url));
const suite = JSON.parse(readFileSync(join(root, 'questions.json'), 'utf8'));
const run = join(root, 'runs', suite.suite);
let text = '# Terra answers with and without Jev\n\nRaw completed arm answers, compiled without rewriting their claims. Consult RESULTS.md for independent grades, runtime corrections and limitations. Both arms used gpt-5.6-terra with high reasoning. These answers are research artifacts; an unverified proposed patch is not a validated fix.\n';
for (const question of suite.questions) {
  text += `\n## ${question.id}\n\n${question.prompt}\n`;
  for (const arm of ['baseline', 'treatment']) {
    const answer = readFileSync(join(run, arm, question.id, 'answer.md'), 'utf8');
    text += `\n### ${arm === 'baseline' ? 'Without Jev' : 'With Jev'}\n\n${answer.replace(/^#{1,6} /gm, '#### ')}\n`;
  }
}
writeFileSync(join(root, 'ANSWERS.md'), text);
console.log(`Compiled ${suite.questions.length * 2} answers into ANSWERS.md.`);
