import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'vitest';
import {
  DEFAULT_EFFORT_LEVEL,
  DIAL_MAX_ACTIVE_ENV,
  DIAL_PRESETS,
  EFFORT_LEVELS,
  applyDialLevel,
  getActiveDialLevel,
  getDialLevel,
  loadDialLevel,
  parseDialLevel,
  resetDialStateForTests,
  restoreDialOnStartup,
  type EffortLevel,
} from '../src/tools/effort-dial.js';
import type { CommandDefinition, PiInstance } from '../src/types.js';

// ─── Fakes ────────────────────────────────────────────────────────────────────

interface FakePi {
  pi: PiInstance;
  thinkingCalls: string[];
  commands: Map<string, CommandDefinition>;
}

function makeFakePi(): FakePi {
  const fake: FakePi = {
    pi: undefined as unknown as PiInstance,
    thinkingCalls: [],
    commands: new Map<string, CommandDefinition>(),
  };
  const raw: Record<string, unknown> = {
    setThinkingLevel: (level: string) => { fake.thinkingCalls.push(level); },
    registerCommand: (name: string, def: CommandDefinition) => { fake.commands.set(name, def); },
  };
  fake.pi = raw as unknown as PiInstance;
  return fake;
}

function tmpHome(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-dial-'));
}

// ─── Mapping table ────────────────────────────────────────────────────────────

test('each dial level maps to the spec thinking level and worker cap', async () => {
  resetDialStateForTests();
  const expected: Record<EffortLevel, { thinking: string; workers: number }> = {
    low: { thinking: 'low', workers: 1 },
    medium: { thinking: 'medium', workers: 2 },
    high: { thinking: 'high', workers: 4 },
    ultra: { thinking: 'xhigh', workers: 4 },
  };

  for (const level of EFFORT_LEVELS) {
    const fake = makeFakePi();
    const env: NodeJS.ProcessEnv = {};
    const home = tmpHome();
    const result = await applyDialLevel(fake.pi, undefined, level, { home, env });

    assert.deepEqual(fake.thinkingCalls, [expected[level].thinking], `thinking for ${level}`);
    assert.equal(env[DIAL_MAX_ACTIVE_ENV], String(expected[level].workers), `worker cap for ${level}`);
    assert.equal(result.thinking, expected[level].thinking);
    assert.equal(result.maxActiveWorkers, expected[level].workers);
    assert.deepEqual(result.warnings, []);
  }
});

test('DIAL_MAX_ACTIVE_ENV matches the exact env var resolveSpawnPolicy reads', () => {
  // Locked to SPAWN_POLICY_MAX_ACTIVE_ENV in src/tools/agent-tools.ts.
  assert.equal(DIAL_MAX_ACTIVE_ENV, 'OCTOCODE_AGENT_MAX_ACTIVE');
});

// ─── Persistence round-trip ───────────────────────────────────────────────────

test('applyDialLevel persists { level } and loadDialLevel round-trips it', async () => {
  resetDialStateForTests();
  const home = tmpHome();
  fs.chmodSync(home, 0o755);
  const fake = makeFakePi();

  await applyDialLevel(fake.pi, undefined, 'ultra', { home, env: {} });

  const onDisk = JSON.parse(fs.readFileSync(path.join(home, 'extension', 'dial.json'), 'utf8')) as { level: string };
  assert.deepEqual(onDisk, { level: 'ultra' });
  assert.equal(loadDialLevel(home), 'ultra');
  if (process.platform !== 'win32') {
    assert.equal(fs.statSync(path.join(home, 'extension')).mode & 0o777, 0o700);
    assert.equal(fs.statSync(path.join(home, 'extension', 'dial.json')).mode & 0o777, 0o600);
  }
});

test('loadDialLevel falls back to medium for missing, garbage, and unknown-level files', () => {
  const home = tmpHome();
  assert.equal(loadDialLevel(home), DEFAULT_EFFORT_LEVEL); // missing file

  fs.mkdirSync(path.join(home, 'extension'), { recursive: true });
  fs.writeFileSync(path.join(home, 'extension', 'dial.json'), 'not json at all');
  assert.equal(loadDialLevel(home), DEFAULT_EFFORT_LEVEL);

  fs.writeFileSync(path.join(home, 'extension', 'dial.json'), JSON.stringify({ level: 'turbo' }));
  assert.equal(loadDialLevel(home), DEFAULT_EFFORT_LEVEL);
});

test('restoreDialOnStartup applies the persisted level without re-persisting', async () => {
  resetDialStateForTests();
  const home = tmpHome();
  const first = makeFakePi();
  await applyDialLevel(first.pi, undefined, 'high', { home, env: {} });

  // Simulate a fresh session: delete the file after loading would prove no rewrite,
  // so instead capture mtime and assert restore does not rewrite the file.
  const filePath = path.join(home, 'extension', 'dial.json');
  const before = fs.statSync(filePath).mtimeMs;

  const fake = makeFakePi();
  const env: NodeJS.ProcessEnv = {};
  const result = await restoreDialOnStartup(fake.pi, undefined, { home, env });

  assert.equal(result?.level, 'high');
  assert.deepEqual(fake.thinkingCalls, ['high']);
  assert.equal(env[DIAL_MAX_ACTIVE_ENV], '4');
  assert.equal(getDialLevel(), 'high');
  assert.equal(fs.statSync(filePath).mtimeMs, before, 'restore must not rewrite dial.json');
});

test('restoreDialOnStartup is a no-op when the user never dialed', async () => {
  resetDialStateForTests();
  const home = tmpHome();
  const fake = makeFakePi();
  const env: NodeJS.ProcessEnv = { [DIAL_MAX_ACTIVE_ENV]: '6' };
  const result = await restoreDialOnStartup(fake.pi, undefined, { home, env });

  assert.equal(result, undefined);
  assert.deepEqual(fake.thinkingCalls, [], 'thinking level untouched without a persisted dial');
  assert.equal(env[DIAL_MAX_ACTIVE_ENV], '6', 'user-set worker cap must not be clobbered');
  assert.equal(getActiveDialLevel(), undefined, 'footer shows no dial segment');
});
// ─── parseDialLevel / getDialLevel ────────────────────────────────────────────

test('parseDialLevel accepts the four levels case-insensitively and rejects the rest', () => {
  assert.equal(parseDialLevel('low'), 'low');
  assert.equal(parseDialLevel('  HIGH '), 'high');
  assert.equal(parseDialLevel('Ultra'), 'ultra');
  assert.equal(parseDialLevel('turbo'), undefined);
  assert.equal(parseDialLevel(''), undefined);
  assert.equal(parseDialLevel(undefined), undefined);
});

test('getDialLevel reflects the last applied level (default medium)', async () => {
  resetDialStateForTests();
  assert.equal(getDialLevel(), 'medium');

  const fake = makeFakePi();
  await applyDialLevel(fake.pi, undefined, 'low', { home: tmpHome(), env: {} });
  assert.equal(getDialLevel(), 'low');
  await applyDialLevel(fake.pi, undefined, 'ultra', { home: tmpHome(), env: {} });
  assert.equal(getDialLevel(), 'ultra');
});

test('DIAL_PRESETS covers exactly the four levels', () => {
  assert.deepEqual(Object.keys(DIAL_PRESETS).sort(), [...EFFORT_LEVELS].sort());
  assert.ok(
    Object.values(DIAL_PRESETS).every(({ maxActiveWorkers }) => maxActiveWorkers <= 4),
    'no effort preset may exceed the cross-host root fan-out ceiling',
  );
});
