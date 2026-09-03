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
  registerDialCommand,
  resetDialStateForTests,
  restoreDialOnStartup,
  type EffortLevel,
} from '../src/tools/effort-dial.js';
import type { CommandDefinition, PiContext, PiInstance } from '../src/types.js';

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

function makeCtx(opts?: {
  custom?: <T>(...args: unknown[]) => Promise<T | undefined>;
}): { ctx: PiContext; notifications: Array<{ message: string; level?: string }> } {
  const notifications: Array<{ message: string; level?: string }> = [];
  const ctx = {
    hasUI: opts?.custom !== undefined,
    // Real TUI contexts always carry mode:'tui'; the overlay helper now requires
    // it (custom() is TUI-only), so the mock must set it when providing custom.
    ...(opts?.custom ? { mode: 'tui' as const } : {}),
    ui: {
      notify: (message: string, level?: string) => { notifications.push({ message, level }); },
      ...(opts?.custom ? { custom: opts.custom } : {}),
    },
  } as unknown as PiContext;
  return { ctx, notifications };
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

// ─── /octocode-dial command ───────────────────────────────────────────────────

function registeredHandler(fake: FakePi, deps?: { home?: string; env?: NodeJS.ProcessEnv }): CommandDefinition {
  registerDialCommand(fake.pi, deps);
  const def = fake.commands.get('octocode-dial');
  assert.ok(def, 'octocode-dial command must be registered');
  return def!;
}

test('/octocode-dial rejects an unknown level with a helpful message', async () => {
  resetDialStateForTests();
  const fake = makeFakePi();
  const env: NodeJS.ProcessEnv = {};
  const def = registeredHandler(fake, { home: tmpHome(), env });
  const { ctx, notifications } = makeCtx();

  await def.handler('turbo', ctx);

  assert.equal(notifications.length, 1);
  assert.equal(notifications[0]!.level, 'error');
  assert.match(notifications[0]!.message, /Unknown effort level 'turbo'/);
  assert.match(notifications[0]!.message, /low, medium, high, ultra/);
  // Nothing applied.
  assert.deepEqual(fake.thinkingCalls, []);
  assert.equal(env[DIAL_MAX_ACTIVE_ENV], undefined);
  assert.equal(getDialLevel(), 'medium');
});

test('/octocode-dial high applies the level and reports the settings', async () => {
  resetDialStateForTests();
  const fake = makeFakePi();
  const env: NodeJS.ProcessEnv = {};
  const home = tmpHome();
  const def = registeredHandler(fake, { home, env });
  const { ctx, notifications } = makeCtx();

  await def.handler(' high ', ctx);

  assert.deepEqual(fake.thinkingCalls, ['high']);
  assert.equal(env[DIAL_MAX_ACTIVE_ENV], '4');
  assert.equal(getDialLevel(), 'high');
  assert.equal(loadDialLevel(home), 'high');
  assert.equal(notifications[0]!.level, 'info');
  assert.match(notifications[0]!.message, /Effort dial: high/);
  assert.match(notifications[0]!.message, /thinking high/);
});

test('/octocode-dial with no args uses the picker and applies the choice', async () => {
  resetDialStateForTests();
  const fake = makeFakePi();
  const env: NodeJS.ProcessEnv = {};
  const home = tmpHome();
  const def = registeredHandler(fake, { home, env });
  const { ctx, notifications } = makeCtx({
    custom: (async () => 'ultra') as <T>(...args: unknown[]) => Promise<T | undefined>,
  });

  await def.handler('', ctx);

  assert.deepEqual(fake.thinkingCalls, ['xhigh']);
  assert.equal(env[DIAL_MAX_ACTIVE_ENV], '4');
  assert.equal(getDialLevel(), 'ultra');
  assert.match(notifications[0]!.message, /Effort dial: ultra/);
});

test('/octocode-dial with no args on a non-interactive host explains the arg form', async () => {
  resetDialStateForTests();
  const fake = makeFakePi();
  const def = registeredHandler(fake, { home: tmpHome(), env: {} });
  const { ctx, notifications } = makeCtx(); // no ui.custom / hasUI

  await def.handler('', ctx);

  assert.equal(notifications.length, 1);
  assert.equal(notifications[0]!.level, 'warning');
  assert.match(notifications[0]!.message, /octocode-dial <low\|medium\|high\|ultra>/);
  assert.deepEqual(fake.thinkingCalls, []);
  assert.equal(getDialLevel(), 'medium');
});

test('argument completions offer the levels with preset descriptions', async () => {
  const fake = makeFakePi();
  const def = registeredHandler(fake, { home: tmpHome(), env: {} });

  const all = await def.getArgumentCompletions?.('');
  assert.deepEqual(all?.map((item) => item.value), [...EFFORT_LEVELS]);
  assert.match(all?.[3]?.description ?? '', /xhigh/);

  const filtered = await def.getArgumentCompletions?.('ul');
  assert.deepEqual(filtered?.map((item) => item.value), ['ultra']);
  assert.equal(await def.getArgumentCompletions?.('zzz'), null);
});

test('DIAL_PRESETS covers exactly the four levels', () => {
  assert.deepEqual(Object.keys(DIAL_PRESETS).sort(), [...EFFORT_LEVELS].sort());
  assert.ok(
    Object.values(DIAL_PRESETS).every(({ maxActiveWorkers }) => maxActiveWorkers <= 4),
    'no effort preset may exceed the cross-host root fan-out ceiling',
  );
});
