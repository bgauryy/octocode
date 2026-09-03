/**
 * F9 ai-watch tests — marker extraction, loop guards (own-write / bash /
 * hash dedupe), deliverAs selection, and command registration. Detection is
 * driven through the __test__.scanFile seam on tmpdir files — no real
 * fs.watch events are relied upon.
 */
import assert from 'node:assert/strict';
import { beforeEach, test } from 'vitest';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {
  __test__,
  extractAiMarkers,
  markBashActivity,
  markOwnWrite,
  registerAiWatch,
  stopWatch,
} from '../src/tools/ai-watch.js';
import type { CommandDefinition, PiCommandContext, PiInstance } from '../src/types.js';

// ─── Harness ──────────────────────────────────────────────────────────────────

interface Sent {
  text: string;
  opts?: { deliverAs?: string };
}

function makePi() {
  const sends: Sent[] = [];
  const commands = new Map<string, CommandDefinition>();
  const pi = {
    on: () => undefined,
    registerCommand: (name: string, def: CommandDefinition) => {
      commands.set(name, def);
    },
    sendUserMessage: (text: string, opts?: { deliverAs?: string }) => {
      sends.push({ text, opts });
    },
  } as unknown as PiInstance;
  return { pi, sends, commands };
}

function makeCommandCtx() {
  const notes: Array<{ msg: string; level?: string }> = [];
  const ctx = {
    ui: { notify: (msg: string, level?: string) => notes.push({ msg, level }) },
  } as unknown as PiCommandContext;
  return { ctx, notes };
}

function tmpProject(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'ai-watch-'));
}

function writeFile(dir: string, name: string, content: string): string {
  const p = path.join(dir, name);
  fs.writeFileSync(p, content, 'utf8');
  return p;
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

beforeEach(() => {
  __test__.reset();
});

// ─── extractAiMarkers ─────────────────────────────────────────────────────────

test('extractAiMarkers matches //, #, block, and html comments ending with AI!', () => {
  const content = [
    'const a = 1; // rename this variable AI!',
    '# tighten this loop AI!',
    '/* add error handling here AI! */',
    '<!-- translate this section AI! -->',
    ' * document the return value AI!',
    'const noise = 2;',
  ].join('\n');
  const markers = extractAiMarkers(content, 'sample.ts');
  assert.deepEqual(markers, [
    { line: 1, text: 'rename this variable' },
    { line: 2, text: 'tighten this loop' },
    { line: 3, text: 'add error handling here' },
    { line: 4, text: 'translate this section' },
    { line: 5, text: 'document the return value' },
  ]);
});

test('extractAiMarkers captures bare markers and //AI! with no space', () => {
  const markers = extractAiMarkers('// AI!\nx();\n//AI!', 'a.js');
  assert.deepEqual(markers, [
    { line: 1, text: '' },
    { line: 3, text: '' },
  ]);
});

test('extractAiMarkers does NOT match OPENAI! or markers not at comment end', () => {
  const content = [
    "const key = 'OPENAI!';", // string, word char before AI!
    '// OPENAI!', // comment but no word boundary
    '// AI! then trailing text', // marker not at end of comment
    '# plain comment without marker',
    'const AI = 1; // uses AI but no marker',
  ].join('\n');
  assert.deepEqual(extractAiMarkers(content, 'x.py'), []);
});

// ─── scanFile detection + delivery ────────────────────────────────────────────

test('scanFile fires sendUserMessage as followUp when no turn is active', async () => {
  const dir = tmpProject();
  const { pi, sends } = makePi();
  registerAiWatch(pi, { cwd: dir, isTurnActive: () => false, env: {} });
  const file = writeFile(dir, 'app.ts', 'export const x = 1; // extract to helper AI!\n');

  const outcome = await __test__.scanFile(file);
  assert.equal(outcome.fired, true);
  assert.equal(outcome.reason, 'fired');
  assert.equal(sends.length, 1);
  assert.equal(sends[0]!.opts?.deliverAs, 'followUp');
  assert.match(sends[0]!.text, /AI! markers found in app\.ts:/);
  assert.match(sends[0]!.text, /line 1: extract to helper/);
});

test('scanFile bounds AI marker prompts and points to the source for omitted instructions', async () => {
  const dir = tmpProject();
  const { pi, sends } = makePi();
  registerAiWatch(pi, { cwd: dir, isTurnActive: () => false, env: {} });
  const markers = Array.from(
    { length: 200 },
    (_, index) => `// ${'instruction '.repeat(80)}${index} AI!`,
  ).join('\n');
  const file = writeFile(dir, 'many.ts', `${markers}\n`);

  const outcome = await __test__.scanFile(file);
  assert.equal(outcome.fired, true);
  assert.equal(outcome.markers.length, 200);
  assert.equal(sends.length, 1);
  assert.ok(sends[0]!.text.length <= 16_000);
  assert.match(sends[0]!.text, /150 more AI! markers omitted/);
  assert.match(sends[0]!.text, /inspect many\.ts directly/);
});

test('scanFile delivers as steer when a turn is active', async () => {
  const dir = tmpProject();
  const { pi, sends } = makePi();
  let active = true;
  registerAiWatch(pi, { cwd: dir, isTurnActive: () => active, env: {} });
  const file = writeFile(dir, 'app.ts', '// do the thing AI!\n');

  await __test__.scanFile(file);
  assert.equal(sends[0]!.opts?.deliverAs, 'steer');

  // Turn ends + content changes → next fire is a followUp.
  active = false;
  fs.writeFileSync(file, '// do the OTHER thing AI!\n');
  await __test__.scanFile(file);
  assert.equal(sends.length, 2);
  assert.equal(sends[1]!.opts?.deliverAs, 'followUp');
});

test('scanFile reports no-markers without firing', async () => {
  const dir = tmpProject();
  const { pi, sends } = makePi();
  registerAiWatch(pi, { cwd: dir, isTurnActive: () => false, env: {} });
  const file = writeFile(dir, 'plain.ts', 'export const y = 2; // ordinary comment\n');

  const outcome = await __test__.scanFile(file);
  assert.equal(outcome.fired, false);
  assert.equal(outcome.reason, 'no-markers');
  assert.equal(sends.length, 0);
});

// ─── Hash dedupe ──────────────────────────────────────────────────────────────

test('content-hash dedupe: same marker set twice fires exactly once', async () => {
  const dir = tmpProject();
  const { pi, sends } = makePi();
  registerAiWatch(pi, { cwd: dir, isTurnActive: () => false, env: {} });
  const file = writeFile(dir, 'dedupe.ts', '// simplify this AI!\n');

  const first = await __test__.scanFile(file);
  const second = await __test__.scanFile(file);
  assert.equal(first.reason, 'fired');
  assert.equal(second.fired, false);
  assert.equal(second.reason, 'deduped');
  assert.equal(sends.length, 1);

  // A different marker set on the same path fires again.
  fs.writeFileSync(file, '// simplify this differently AI!\n');
  const third = await __test__.scanFile(file);
  assert.equal(third.reason, 'fired');
  assert.equal(sends.length, 2);
});

test('dedupe is per path: identical markers in two files both fire', async () => {
  const dir = tmpProject();
  const { pi, sends } = makePi();
  registerAiWatch(pi, { cwd: dir, isTurnActive: () => false, env: {} });
  const a = writeFile(dir, 'a.ts', '// fix me AI!\n');
  const b = writeFile(dir, 'b.ts', '// fix me AI!\n');

  await __test__.scanFile(a);
  await __test__.scanFile(b);
  assert.equal(sends.length, 2);
});

// ─── Own-write suppression ────────────────────────────────────────────────────

test('markOwnWrite suppresses that path until the window expires', async () => {
  const dir = tmpProject();
  const { pi, sends } = makePi();
  registerAiWatch(pi, { cwd: dir, isTurnActive: () => false, env: {} });
  __test__.setWindowsForTests({ ownWriteMs: 40 });
  const file = writeFile(dir, 'own.ts', '// clean this up AI!\n');
  const other = writeFile(dir, 'other.ts', '// untouched file AI!\n');

  markOwnWrite(file);
  assert.equal(__test__.isOwnWriteSuppressed(file), true);

  const suppressed = await __test__.scanFile(file);
  assert.equal(suppressed.fired, false);
  assert.equal(suppressed.reason, 'own-write-suppressed');

  // Own-write suppression is per path — other files still fire.
  const otherOutcome = await __test__.scanFile(other);
  assert.equal(otherOutcome.reason, 'fired');

  await sleep(60);
  assert.equal(__test__.isOwnWriteSuppressed(file), false);
  const after = await __test__.scanFile(file);
  assert.equal(after.reason, 'fired');
  assert.equal(sends.length, 2);
});

// ─── Bash suppression ─────────────────────────────────────────────────────────

test('markBashActivity suppresses ALL paths until the window expires', async () => {
  const dir = tmpProject();
  const { pi, sends } = makePi();
  registerAiWatch(pi, { cwd: dir, isTurnActive: () => false, env: {} });
  __test__.setWindowsForTests({ bashMs: 40 });
  const a = writeFile(dir, 'a.ts', '// alpha AI!\n');
  const b = writeFile(dir, 'b.ts', '// beta AI!\n');

  markBashActivity();
  assert.equal(__test__.isBashSuppressed(), true);
  assert.equal((await __test__.scanFile(a)).reason, 'bash-suppressed');
  assert.equal((await __test__.scanFile(b)).reason, 'bash-suppressed');
  assert.equal(sends.length, 0);

  await sleep(60);
  assert.equal(__test__.isBashSuppressed(), false);
  assert.equal((await __test__.scanFile(a)).reason, 'fired');
  assert.equal((await __test__.scanFile(b)).reason, 'fired');
  assert.equal(sends.length, 2);
});

// ─── Path filtering ───────────────────────────────────────────────────────────

test('isWatchablePath filters ignored dirs, temp files, and unknown extensions', () => {
  assert.equal(__test__.isWatchablePath('src/tools/ai-watch.ts'), true);
  assert.equal(__test__.isWatchablePath('README.md'), true);
  assert.equal(__test__.isWatchablePath('Dockerfile'), true);
  assert.equal(__test__.isWatchablePath('node_modules/pkg/index.js'), false);
  assert.equal(__test__.isWatchablePath('.git/HEAD'), false);
  assert.equal(__test__.isWatchablePath('dist/bundle.js'), false);
  assert.equal(__test__.isWatchablePath('src/a.octocode-1-abc.tmp'), false);
  assert.equal(__test__.isWatchablePath('image.png'), false);
});

test('parsePorcelainPaths handles plain, quoted, and renamed entries', () => {
  const stdout = ' M src/a.ts\n?? new.ts\nR  old.ts -> renamed.ts\n?? "sp ace.ts"\n';
  assert.deepEqual(__test__.parsePorcelainPaths(stdout), [
    'src/a.ts',
    'new.ts',
    'renamed.ts',
    'sp ace.ts',
  ]);
});

// ─── Command registration / lifecycle ─────────────────────────────────────────

test('registerAiWatch registers /octocode-watch with on|off|status handling', async () => {
  const dir = tmpProject();
  const { pi, commands } = makePi();
  registerAiWatch(pi, { cwd: dir, isTurnActive: () => false, env: {} });

  const command = commands.get('octocode-watch');
  assert.ok(command, 'octocode-watch command registered');
  const completions = await command!.getArgumentCompletions?.('o');
  assert.deepEqual(completions?.map((item) => item.value), ['on', 'off']);

  const { ctx, notes } = makeCommandCtx();
  await command!.handler('status', ctx);
  assert.match(notes[0]!.msg, /octocode-watch: off/);

  await command!.handler('on', ctx);
  assert.equal(__test__.getWatchMode() !== undefined, true);
  assert.match(notes[1]!.msg, /octocode-watch: on/);

  await command!.handler('status', ctx);
  assert.match(notes[2]!.msg, /octocode-watch: on/);

  await command!.handler('off', ctx);
  assert.equal(__test__.getWatchMode(), undefined);
  assert.match(notes[3]!.msg, /octocode-watch: off/);
});

test('OCTOCODE_WATCH=1 auto-starts the watcher; stopWatch is idempotent', () => {
  const dir = tmpProject();
  const { pi } = makePi();
  registerAiWatch(pi, { cwd: dir, isTurnActive: () => false, env: { OCTOCODE_WATCH: '1' } });
  assert.equal(__test__.getWatchMode() !== undefined, true);
  stopWatch();
  assert.equal(__test__.getWatchMode(), undefined);
  stopWatch(); // second call must not throw
});
