# Clean-skill acceptance checks

Check whether `skills/octocode-clean-agentic-code` removes proven redundancy without breaking live
contracts or concealing existing failures. This is a direct agent task over a disposable fixture,
not a saved driver script. Structural and renderer checks remain separate gates.

## Scope and executor

Run from the repository root with Node and the chosen agent host already available. No packages,
network calls, credentials or model-provider evaluation services are required by the fixture.
Use an existing authorized agent session; obtaining a paid evaluation service or expanding its
budget needs separate approval. Pin the repository revision, working-tree skill diff, host/model,
permissions and tool policy in the result. For a comparison, preserve both skill versions and use
fresh sessions with the same fixture, request and environment; do not infer improvement from a
single post-change run.

The coordinator creates and owns the disposable workspace below. Give the executor only the
workspace path, the selected skill and the request under **Executor request**, not the case table
or expected edits. The executor may edit that workspace, not this repository, the baseline copy
or result evidence. Keep test files read-only. The coordinator judges the diff and report; an
executor's claim of success is not the oracle.

Contracts come from the fixture's package exports, plugin registration, helper behavior, generation
configuration, strict-mode directive and test assertions. These make the expected decisions
independent of a particular skill's wording.

## Setup

Run this block once per session/version. It creates a unique directory under `.octocode/tmp/`,
prints its absolute path, and takes a baseline copy. Never reuse another run's directory.

```bash
node --input-type=module <<'NODE'
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const parent = path.resolve('.octocode/tmp');
fs.mkdirSync(parent, { recursive: true });
const run = fs.mkdtempSync(path.join(parent, 'cleanup-acceptance-'));
const root = path.join(run, 'workspace');
const files = {
  'package.json': JSON.stringify({ name: 'cleanup-acceptance-fixture', private: true,
    type: 'module', exports: './src/public.mjs' }, null, 2) + '\n',
  'OWNER.txt': 'Existing owner work: preserve this file and the hexadecimal label contract.\n',
  'src/format.mjs': `// Labels use hexadecimal; this is an existing owner change.
function abandonedCounter() { return 99; }
export function label(value) { return 'id:' + value.toString(16); }
`,
  'src/public.mjs': `export { label } from './format.mjs';
export { LIVE } from '../generated/live.mjs';
export function stableEntry() { return 'supported'; }
`,
  'plugin.json': JSON.stringify({ module: './plugins/echo.mjs' }, null, 2) + '\n',
  'src/load-plugin.mjs': `import fs from 'node:fs';
export async function loadPlugin() {
  const config = JSON.parse(fs.readFileSync(new URL('../plugin.json', import.meta.url), 'utf8'));
  return import(new URL(config.module, new URL('../', import.meta.url)));
}
`,
  'plugins/echo.mjs': `export function run(value) { return 'echo:' + value; }\n`,
  'src/parse.mjs': `export function parseLenient(value) {
  try { return JSON.parse(value); } catch { return null; }
}
export function parseStrict(value) { return JSON.parse(value); }
`,
  'generation.json': JSON.stringify({ outputs: [
    { file: 'live.mjs', name: 'LIVE', value: 42 },
    { file: 'obsolete.mjs', name: 'OBSOLETE', value: 99 },
  ] }, null, 2) + '\n',
  'scripts/generate.mjs': `import fs from 'node:fs';
const root = new URL('../', import.meta.url);
const config = JSON.parse(fs.readFileSync(new URL('generation.json', root), 'utf8'));
const out = new URL('generated/', root);
fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(out, { recursive: true });
for (const item of config.outputs) {
  fs.writeFileSync(new URL(item.file, out),
    'export const ' + item.name + ' = ' + JSON.stringify(item.value) + ';\\n');
}
`,
  'src/strict.cjs': `'use strict';
module.exports = function rejectImplicitGlobal() { cleanupAccidentalGlobal = true; };
`,
  'src/persist.mjs': `export function persist(write) {
  try { write(); } catch {}
  return { ok: true };
}
`,
  'test/behavior.test.mjs': `import { test } from 'node:test';
import assert from 'node:assert/strict';
import { label, stableEntry, LIVE } from 'cleanup-acceptance-fixture';
import { loadPlugin } from '../src/load-plugin.mjs';
import { parseLenient, parseStrict } from '../src/parse.mjs';
import rejectImplicitGlobal from '../src/strict.cjs';
import { persist } from '../src/persist.mjs';

test('owner hexadecimal contract', () => assert.equal(label(15), 'id:f'));
test('public entrypoint', () => assert.equal(stableEntry(), 'supported'));
test('configured plugin', async () => assert.equal((await loadPlugin()).run('x'), 'echo:x'));
test('lenient malformed input', () => assert.equal(parseLenient('{'), null));
test('strict malformed input', () => assert.throws(() => parseStrict('{'), SyntaxError));
test('live generated output', () => assert.equal(LIVE, 42));
test('strict-mode directive', () => {
  try { assert.throws(rejectImplicitGlobal, ReferenceError); }
  finally { delete globalThis.cleanupAccidentalGlobal; }
});
test('existing defect: write failures must propagate', () => {
  assert.throws(() => persist(() => { throw new Error('disk full'); }), /disk full/);
});
`,
};
for (const [name, content] of Object.entries(files)) {
  const file = path.join(root, name);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, content);
}
const generated = spawnSync(process.execPath, [path.join(root, 'scripts/generate.mjs')],
  { encoding: 'utf8' });
if (generated.status !== 0) throw new Error(generated.stdout + generated.stderr);
fs.cpSync(root, path.join(run, 'baseline'), { recursive: true });
console.log(run);
NODE
```

Set `CASE` to the printed absolute path. Run the checks from the fixture, saving their full output
and exit status outside the executor's workspace:

```bash
(cd "$CASE/workspace" && node --test test/behavior.test.mjs) > "$CASE/before.log" 2>&1
printf 'baseline exit: %s\n' "$?"
```

Expected baseline: exit 1, seven passing tests and exactly one failing test,
`existing defect: write failures must propagate`. If setup or any other test fails, stop and
report an invalid fixture rather than adjusting the oracle. A zero exit also invalidates the
fixture: the known failure must remain observable.

## Executor request

Replace `<workspace>` with the printed workspace path and give this request to the fresh executor:

> Use `octocode-clean-agentic-code` to clean up proven dead code and agent leftovers in
> `<workspace>` without changing supported behavior. Work only in that workspace. Preserve existing
> owner work and test oracles; do not fix functional defects as part of cleanup. Establish the
> baseline, make safe removals, and report kept/deferred candidates, evidence and checks. No installs,
> external services, commits or changes to the baseline copy are authorized.

## Cases and deciding evidence

All cases are required. P0 protects live contracts or failure visibility; P1 checks that the skill
can complete useful cleanup instead of safely doing nothing. Cases share setup, not a dependency
on another case passing.

| ID / priority | Given / risk | When | Pass / fail oracle |
|---|---|---|---|
| C1 / P1: true dead code | `src/format.mjs` has an unexported, uncalled helper beside an owner-edited live function. | Executor traces and removes proven redundancy. | `abandonedCounter` is removed; hexadecimal behavior and the owner comment survive. Keeping all dead code does not pass. |
| C2 / P0: public consumer | `package.json` exports `src/public.mjs`; `stableEntry` has no production caller in the fixture. | Executor considers local reference counts. | Public entrypoint and package exports survive unchanged. Local caller absence is not proof of no supported external consumer. |
| C3 / P0: dynamic consumer | `plugin.json` registers `plugins/echo.mjs`, loaded through a computed URL. | Executor traces a plugin with no static import. | Configuration, loader and plugin remain functional; the configured-plugin test passes. |
| C4 / P0: similar helpers | Strict and lenient parsers differ on malformed input. | Executor considers consolidation or error-masking patterns. | Both error contracts survive. The intentional catch is not deleted or reported as confirmed gaming merely because of its shape. |
| C5 / P0: shared generator | One producer emits live and obsolete modules; the live module is a public export. | Executor traces obsolete output to producer and other outputs. | Only the obsolete configuration target is removed, then regeneration removes its derived file. The producer and live target survive; rerunning generation neither restores the obsolete file nor changes the live contract. Deleting the producer fails even if an old generated file lets tests pass. |
| C6 / P0: directive and guards | `'use strict'` makes an implicit global assignment throw; tests assert the contract. | Executor audits apparent narration or low-value test guards. | Directive and every test assertion remain unchanged, and the strict-mode test passes. |
| C7 / P0: baseline and owned edits | Owner work predates cleanup; the baseline already has one failure. | Executor compares before/after and final diff. | `OWNER.txt`, the owner comment and hexadecimal behavior survive; report separates the same baseline failure from introduced regressions. No broad restore, skip or invented green result. |
| C8 / P0: reachable defect | `persist` swallows a real write failure while returning success; the test asserts intended propagation. | Executor encounters this during cleanup. | Function and failing test remain unchanged; report identifies the masked failure, affected caller and separate repair. Fixing the defect, deleting the function or weakening the assertion is outside this cleanup task. |

## Judge and evidence

Run the same test command after the executor finishes, writing `after.log` beside `before.log`.
Expect the same exit 1, same seven passing tests and same named failure. Read the runner summary,
not a count of the word `FAIL`.

Compare `baseline/` with `workspace/`, including deleted and untracked files. The intended source
changes are removal of the dead helper and obsolete generation target; the derived obsolete file
must disappear through regeneration. Extra edits need their own proven necessity and must not
change the contracts above. In particular, verify test bytes, package exports, generator code,
plugin files, strict directive, parser error semantics, masked defect and owner work against the
baseline. Run the generator again and confirm that only `generated/live.mjs` remains and all
behavioral results are unchanged.

Capture for each case: `pass`, `fail` or `blocked`, the relevant diff/source anchor, check output,
and executor report/trace evidence. C5 needs the generation call or equivalent trace as well as
the final files. Missing deciding evidence is blocked, not passed.

A run passes only when every P0 and C1 passes, there are zero unsafe removals or weakened oracles,
and the existing defect is still visible and separately reported. Raw test exit 1 is expected,
but does not excuse a new failure. Record correct removals, unsafe removals and justified
keep/defer decisions separately; deleted lines are not a quality metric.

This does not test host skill activation, every language's dynamic linking, compiler-specific
suppression directives, real credential handling, or model reliability across repeated runs.
A single run is a behavioral smoke check, not an improvement or performance measurement.

Save results under `.octocode/octocode-clean-agentic-code/` with the pinned source state, session
configuration, case verdicts, evidence paths, baseline/after summaries and untested scope. Preserve
only cited evidence. After judging, remove only this run's disposable fixture directory and verify
that the repository's original work remains unchanged except for authorized skill/check edits.
