import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { idsIn } from '../rfc-trace.mjs';

const SCRIPT = fileURLToPath(new URL('../rfc-trace.mjs', import.meta.url));
const FINDINGS = ['undefinedIds', 'duplicateIds', 'featuresWithoutTask', 'featuresWithoutCheck', 'tasksWithoutServes', 'forwardDeps', 'cycles', 'citationsNotInSources', 'brokenLocalLinks'];

const FEATURES = `| F | Consumer | Before → after | Example | Accepted by |
|---|---|---|---|---|
| F1 | Agent on a Swift package | Root falls back to the file folder → the \`Package.swift\` folder | \`lspSearch\` on a nested file | V1 |
| F2 | Every other server | No change | Same call, same root | V2 |`;
const TASKS = `| T | Make | Where | Serves | Depends on | Done when | State |
|---|---|---|---|---|---|---|
| T1 | read rootMarkers | \`src/config.rs\` | F1 | none | unit test passes | done |
| T2 | root lookup uses markers | \`src/root.rs\` | F1, F2 | T1 | V2 passes | doing |
| T3 | docs | \`docs/LSP.md\` | F1 | T1 | docs check passes | blocked (Q1) |
| T4 | live Swift check | benchmark | F1 | T2, T3 | V1 passes | todo |`;
const VALIDATION = `- **V1 (F1):** given a Swift package, when \`lspSearch\` runs on a nested file, then the root is the \`Package.swift\` folder.
- **V2 (F2):** given a Rust crate, when the same call runs, then the root equals today's.`;

function rfc({ lead = '', features = FEATURES, tasks = TASKS, validation = VALIDATION, questions = '- Q1 Non-blocking: which docs page owns server settings?', sources = '- [root.rs](src/root.rs)\n- [LSP roots](https://example.com/lsp)', extra = '' } = {}) {
  return `---
status: Proposed
question: Should lspSearch read rootMarkers?
---

# Root markers

Read \`rootMarkers\` per server ([root.rs](src/root.rs#L2)); see [Tasks](#tasks-and-rollout) and [roots](https://example.com/lsp#roots).
${lead}
## Summary

F1 finds a Swift root; F2 keeps other servers unchanged.

## Features

${features}

## Proposed design

The lookup reads \`rootMarkers\` first.
${extra}
## Tasks and rollout

${tasks}

## Validation and success

${validation}

## Open questions

${questions}

## Sources

${sources}
`;
}

function withRfc(text, fn) {
  const dir = mkdtempSync(join(tmpdir(), 'rfc-trace-'));
  try {
    mkdirSync(join(dir, 'src'));
    writeFileSync(join(dir, 'src', 'root.rs'), 'one\ntwo\nthree\n');
    writeFileSync(join(dir, 'RFC.md'), text);
    return fn(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function run(args, script = SCRIPT) {
  const result = spawnSync(process.execPath, [script, ...args], { encoding: 'utf8' });
  let json = null;
  try { json = JSON.parse(result.stdout); } catch { /* text output */ }
  return { status: result.status, stdout: result.stdout, json };
}
const trace = (dir, ...args) => run([join(dir, 'RFC.md'), '--workspace', dir, ...args]);

test('a traced RFC exits 0 with no findings, a dependency chain as critical path and a graph of every task', () => {
  withRfc(rfc(), (dir) => {
    const { status, json } = trace(dir);
    assert.equal(status, 0, JSON.stringify(json));
    assert.equal(json.ok, true);
    for (const key of FINDINGS) assert.deepEqual(json[key], [], key);
    const taskIds = [...TASKS.matchAll(/^\| (T\d+) /gm)].map(([, id]) => id);
    const edges = new Set([...json.taskGraphMermaid.matchAll(/^ {2}(T\d+) (?:==>\|critical\||-->) (T\d+)$/gm)].map(([, from, to]) => `${from}>${to}`));
    assert.ok(json.criticalPath.length > 1);
    for (let i = 1; i < json.criticalPath.length; i++) assert.ok(edges.has(`${json.criticalPath[i - 1]}>${json.criticalPath[i]}`), 'each critical step is a Depends on edge');
    assert.equal((json.taskGraphMermaid.match(/==>\|critical\|/g) ?? []).length, json.criticalPath.length - 1);
    for (const id of taskIds) assert.match(json.taskGraphMermaid, new RegExp(`^ {2}${id}\\["`, 'm'), `node ${id}`);
    assert.equal(json.taskGraphBranches, json.criticalPath.length !== taskIds.length);
    assert.match(json.taskGraphMermaid, /T3\["blocked: T3 docs, Q1"\]/, 'a blocked task names its question');
    assert.match(json.taskGraphMermaid, /T4\["T4 /, 'todo carries no state label');
  });
});

test('each trace gap is reported in its own list and exits 1', () => {
  const tasks = `${TASKS}
| T5 | cache | \`src/cache.rs\` | none | T6 | bench | todo |
| T6 | warm cache | \`src/cache.rs\` | F2 | T5 | bench | todo |
| T7 | early step | \`src/early.rs\` | F2 | T8 | test | todo |
| T8 | late step | \`src/late.rs\` | F2 | none | test | todo |`;
  const features = `${FEATURES}
| F3 | Operator | No metric → a counter | \`/metrics\` | V3 |
| F2 | Duplicate | row | — | V2 |`;
  const lead = `
Q9 is open; cited [elsewhere](https://example.org/missing), [gone](missing.md), [anchor](#nowhere), [line](src/root.rs#L99), [ref][nope] and a note[^gone].
`;
  withRfc(rfc({ tasks, features, lead }), (dir) => {
    const { status, json } = trace(dir);
    assert.equal(status, 1);
    assert.equal(json.ok, false);
    assert.ok(json.undefinedIds.some((item) => item.id === 'Q9'));
    assert.ok(json.undefinedIds.some((item) => item.id === 'V3'), 'an Accepted by V with no V item');
    assert.deepEqual(json.duplicateIds.map((item) => item.id), ['F2']);
    assert.deepEqual(json.featuresWithoutTask, ['F3']);
    assert.deepEqual(json.featuresWithoutCheck, ['F3']);
    assert.deepEqual(json.tasksWithoutServes, ['T5']);
    assert.ok(json.forwardDeps.some((dep) => dep.task === 'T7' && dep.dependsOn === 'T8'));
    assert.ok(json.forwardDeps.some((dep) => dep.task === 'T5' && dep.dependsOn === 'T6'));
    assert.deepEqual(json.cycles, [['T5', 'T6']]);
    assert.deepEqual(json.criticalPath, [], 'no critical path through a cycle');
    assert.deepEqual(json.citationsNotInSources.map((item) => item.target).toSorted(), ['https://example.org/missing', 'missing.md']);
    const reasons = Object.fromEntries(json.brokenLocalLinks.map((link) => [link.target, link.reason]));
    assert.equal(reasons['missing.md'], 'notFound');
    assert.equal(reasons['#nowhere'], 'anchorNotFound');
    assert.equal(reasons['src/root.rs#L99'], 'lineBeyondEnd');
    assert.equal(reasons['[nope]'], 'referenceUndefined');
    assert.equal(reasons.gone, 'footnoteUndefined');
    for (const link of json.brokenLocalLinks) assert.ok(Number.isInteger(link.line));
  });
});

test("ranges, Won't features, --not-id and fenced code follow the format rules", () => {
  const features = `| F | Consumer | Before → after | Example | Priority | Accepted by |
|---|---|---|---|---|---|
| F1 | Agent | old → new | call | Must | V1 |
| F2 | Agent | old → new | call | Must | V2 |
| F3 | Agent | old → new | call | Won't | none |`;
  const tasks = `| T | Make | Where | Serves | Depends on | Done when |
|---|---|---|---|---|---|
| T1 | build | \`src/a.rs\` | F1–F2 | none | V1 and V2 pass |`;
  const validation = '| V | Given → When → Then | Accepts |\n|---|---|---|\n| V1 | given a, when b, then c | F1 to F2 |\n| V2 | given d, when e, then f | F2 |';
  const extra = '\nLatency P99 stays flat.\n\n```js\nconst T42 = 1;\n```\n\n```mermaid\nflowchart LR\n  T1 --> T9\n```\n';
  withRfc(rfc({ features, tasks, validation, extra }), (dir) => {
    const { json } = trace(dir);
    assert.ok(!json.error, JSON.stringify(json.error));
    assert.deepEqual(json.featuresWithoutTask, [], "a range serves every feature in it; Won't needs no task");
    assert.deepEqual(json.featuresWithoutCheck, []);
    const undefinedIds = json.undefinedIds.map((item) => item.id);
    assert.ok(undefinedIds.includes('P99'));
    assert.ok(undefinedIds.includes('T9'), 'mermaid blocks are scanned');
    assert.ok(!undefinedIds.includes('T42'), 'other fenced code is not');
    assert.equal(json.taskGraphBranches, false, 'one task is one chain');
    const ignored = trace(dir, '--not-id', 'P99,T9');
    assert.deepEqual(ignored.json.undefinedIds, []);
    assert.equal(ignored.status, 0);
  });
});

test('input the trace cannot read exits 2 with a stable error code', () => {
  const cases = [
    { text: rfc().replace('## Sources', '## References'), args: [], code: 'missingSection' },
    { text: rfc({ tasks: TASKS.replace('Depends on', 'After') }), args: [], code: 'tasksTableColumns' },
    { text: rfc({ tasks: 'Tasks follow the design.' }), args: [], code: 'noTasks' },
    { text: rfc(), args: ['--not-id', 'latency'], code: 'invalidArguments' },
    { text: rfc(), args: ['--bogus'], code: 'invalidArguments' },
  ];
  for (const { text, args, code } of cases) {
    withRfc(text, (dir) => {
      const { status, json } = trace(dir, ...args);
      assert.equal(status, 2, code);
      assert.equal(json.ok, false);
      assert.equal(json.error.code, code);
      assert.ok(json.error.message);
    });
  }
  const missing = run([join(tmpdir(), 'no-such-rfc-trace', 'RFC.md')]);
  assert.equal(missing.status, 2);
  assert.equal(missing.json.error.code, 'fileNotFound');
});

test('--mermaid prints the JSON graph as a fenced block; --help names every output field', () => {
  withRfc(rfc(), (dir) => {
    const { json } = trace(dir);
    const block = trace(dir, '--mermaid');
    assert.equal(block.status, 0);
    assert.equal(block.stdout.trim(), `\`\`\`mermaid\n${json.taskGraphMermaid}\n\`\`\``);
    const help = run(['--help']);
    assert.equal(help.status, 0);
    for (const key of Object.keys(json)) assert.match(help.stdout, new RegExp(`\\b${key}\\b`), key);
  });
});

test('runs through a symlinked entry point', () => {
  const dir = mkdtempSync(join(tmpdir(), 'rfc-trace-link-'));
  try {
    const link = join(dir, 'rfc-trace.mjs');
    symlinkSync(SCRIPT, link);
    const help = run(['--help'], link);
    assert.equal(help.status, 0);
    assert.match(help.stdout, /Usage:/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('idsIn expands ranges in order and keeps leading zeros', () => {
  assert.deepEqual(idsIn('F1–F3, T2 to T4 and V1-V2'), ['F1', 'F2', 'F3', 'T2', 'T3', 'T4', 'V1', 'V2']);
  assert.deepEqual(idsIn('T08-T10'), ['T08', 'T09', 'T10']);
  assert.deepEqual(idsIn('F1a, XT1, T3–T1'), ['T3', 'T1']);
  assert.deepEqual(idsIn('P99 and F1', new Set(['P99'])), ['F1']);
});
