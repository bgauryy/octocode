import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { passPath, questions, runPath } from './lib.mjs';
import { FACT_STATES, LETTERS, factsOf, jobs, judgePrompt, prepare, redact, verdictOf } from './judge.mjs';
import { roundIn, scratch, storeRun } from './testing.mjs';

test('factsOf reads the Fact table, else the Facts list, else nothing', () => {
  const table = '# key\n\n| Fact | Value |\n|---|---|\n| Declaration | `a.rs:220` |\n| Delegates to | `b()` |\n\nVerified: rg\n';
  assert.deepEqual(factsOf(table), ['Declaration: `a.rs:220`', 'Delegates to: `b()`']);
  assert.deepEqual(factsOf(table.replace('| Delegates to |', '| Not the answer |')), ['Declaration: `a.rs:220`'], 'a distractor row is not a fact');
  const numbered = '| # | Fact | Value | Line |\n|---|---|---|---|\n| 1 | Defining file | `a.ts` | 149 |\n| 2 | Helper | `b()` | 157 |\n| Not the answer | `c` | elsewhere | 378 |\n';
  assert.deepEqual(factsOf(numbered), ['Defining file: `a.ts`: 149', 'Helper: `b()`: 157']);
  assert.deepEqual(factsOf('# key\n\n## Facts\n\n- one\n  continued\n- two\n\n## Notes\n\n- not a fact\n'), ['one continued', 'two']);
  assert.deepEqual(factsOf('# key\n\nprose with | File:line | tables |\n|---|---|\n| a | b |\n'), []);
});

test('a verdict needs a score for every answer and, with facts, a known state for every fact of every answer', () => {
  const letters = LETTERS.slice(0, 2);
  const line = 'VERDICT Q1: X c=10 d=5 | Y c=7 d=3 | winner=X | confident=yes';
  assert.deepEqual(verdictOf(`reasoning\n${line}`, letters, 0).scores, { X: 10, Y: 7 });
  assert.deepEqual(verdictOf(`**${line}**`, letters, 0).depth, { X: 5, Y: 3 });
  assert.equal(verdictOf('VERDICT Q1: X c=10 d=5 | winner=X', letters, 0), undefined);
  const facts = `FACTS X: F1=right F2=${FACT_STATES[3]}\nFACTS Y: F1=missing F2=wrong\n${line}`;
  assert.deepEqual(verdictOf(facts, letters, 2).facts, { X: ['right', FACT_STATES[3]], Y: ['missing', 'wrong'] });
  assert.equal(verdictOf(`FACTS X: F1=right F2=right\n${line}`, letters, 2), undefined, 'an answer without its FACTS line');
  assert.equal(verdictOf(`FACTS X: F1=right\nFACTS Y: F1=right F2=right\n${line}`, letters, 2), undefined, 'a fact without a state');
  assert.equal(verdictOf(`FACTS X: F1=right F2=maybe\nFACTS Y: F1=right F2=right\n${line}`, letters, 2), undefined, 'an unknown state');
});

test('the prompt asks for fact states only when the key lists facts, and shows one section per answer', () => {
  const base = { brief: 'BRIEF', question: '## Question\n\nWhat?', id: 'Q1', clones: '/clones', answers: ['a1', 'a2', ''] };
  const plain = judgePrompt({ ...base, key: undefined, facts: [] });
  assert.doesNotMatch(plain, /FACTS|Rubric/);
  assert.equal(plain.match(/^## Answer [A-Z]$/gm).length, base.answers.length);
  assert.match(plain, /\(no answer was produced\)/);
  const facts = ['first', 'second'];
  const keyed = judgePrompt({ ...base, key: 'KEY TEXT', facts });
  assert.match(keyed, /KEY TEXT/);
  for (const letter of LETTERS.slice(0, base.answers.length)) assert.ok(keyed.includes(`FACTS ${letter}: F1=<${FACT_STATES.join('|')}> F2=`));
  assert.ok(keyed.trimEnd().split('\n').at(-1).startsWith('End with exactly one line: VERDICT Q1: X c=<0-10> d=<1-5> | Y '));
});

test('redaction removes the product, its tool names, the clone path and gh commands', () => {
  const out = redact('Octocode localFetch read /c/repos/x; `gh api` said so; mylocalFetcher stays', { clones: '/c/repos', toolNames: ['localFetch'] });
  assert.equal(out, '[tool] [tool] read <clones>/x; `[tool] api` said so; mylocalFetcher stays');
});

test('first grading uses a seeded order; only a gap of two or more points is regraded, in reverse order', () => {
  const { dir, cleanup } = scratch('judge');
  try {
    const manifest = roundIn(dir, { ids: ['Q1', 'Q2', 'Q3'] });
    for (const question of questions(manifest)) for (const arm of question.arms) storeRun(runPath(manifest, 1, question.set, question.id, arm), { steps: [{}], answer: `Octocode answer of ${arm}` });
    fs.writeFileSync(path.join(manifest.sets.s.dir, 'answers', 'Q1.md'), '| Fact | Value |\n|---|---|\n| F | v |\n');
    const first = jobs(manifest, 'first');
    assert.equal(first.length, 3);
    assert.deepEqual(first.map(job => job.order), jobs(manifest, 'first').map(job => job.order));
    for (const job of first) assert.deepEqual(job.order.toSorted(), ['gh', 'tool']);
    const prepared = prepare(manifest, 1, first[0].question, first[0].order);
    assert.equal(prepared.facts.length, 1);
    assert.doesNotMatch(prepared.prompt.split('## Answer X')[1], /octocode/i);
    const gaps = { Q1: 1, Q2: 2, Q3: 5 };
    for (const job of first) {
      fs.mkdirSync(job.out, { recursive: true });
      fs.writeFileSync(path.join(job.out, 'verdict.json'), JSON.stringify({ scores: { [job.order[0]]: 10, [job.order[1]]: 10 - gaps[job.question.id] }, order: job.order }));
    }
    const swap = jobs(manifest, 'swap');
    assert.deepEqual(swap.map(job => job.question.id), Object.keys(gaps).filter(id => gaps[id] >= 2));
    for (const job of swap) {
      assert.deepEqual(job.order, first.find(one => one.question.id === job.question.id).order.toReversed());
      assert.equal(job.out, path.join(passPath(manifest, 1), 'judge', `${job.question.name}-swap`));
    }
  } finally {
    cleanup();
  }
});
