import assert from 'node:assert/strict';
import { test } from 'vitest';
import fs from 'node:fs';
import octocodeDefault, {
  bundledAwarenessSkillRoot,
  createAwarenessHooksAddon,
  createOctocodePiExtension,
  resolvePromptMode,
  composeSystemPrompt,
} from '../src/index.js';

test('default export preserves the single-arg Pi contract (default(pi))', () => {
  assert.equal(typeof octocodeDefault, 'function');
  assert.equal(octocodeDefault.length, 1, 'Pi calls default(pi) with exactly one arg');
});

test('createOctocodePiExtension returns a single-arg wiring function', () => {
  const wiring = createOctocodePiExtension({ promptMode: 'octocode-first' });
  assert.equal(typeof wiring, 'function');
  assert.equal(wiring.length, 1);
});

test('createAwarenessHooksAddon returns a standalone Pi hooks addon', () => {
  const events: string[] = [];
  const addon = createAwarenessHooksAddon();

  assert.equal(typeof addon, 'function');
  assert.equal(addon.length, 1);

  const bridge = addon({
    on: (eventName: string) => {
      events.push(eventName);
    },
  } as unknown as Parameters<typeof addon>[0]);

  assert.ok(bridge);
  assert.deepEqual(events, [
    'tool_call',
    'tool_result',
    'tool_execution_start',
    'tool_execution_end',
    'input',
    'before_agent_start',
    'agent_end',
    'session_before_compact',
    'session_shutdown',
  ]);
});

test('bundledAwarenessSkillRoot resolves the bundled awareness skill dir', () => {
  const root = bundledAwarenessSkillRoot();
  // In a built package the bundle exists; in a bare src checkout it may not.
  // When present it must be a real dir carrying the skill's SKILL.md so the
  // harness self-edit gate has something to protect.
  if (root) {
    assert.ok(root.endsWith('/octocode-awareness'), `expected an octocode-awareness dir, got ${root}`);
    assert.ok(fs.existsSync(root), 'bundled skill root should exist when returned');
  }
});

test('createAwarenessHooksAddon wires the bundled skillRoot so the Pi harness gate is not a no-op', async () => {
  const root = bundledAwarenessSkillRoot();
  if (!root) return; // no bundle in this checkout — nothing to protect, gate correctly stays off
  const previousAllow = process.env['OCTOCODE_ALLOW_HARNESS_APPLY'];
  try {
    delete process.env['OCTOCODE_ALLOW_HARNESS_APPLY'];
    let toolCall: ((event: unknown, ctx: unknown) => Promise<unknown>) | undefined;
    const addon = createAwarenessHooksAddon();
    addon({
      on: (eventName: string, fn: (event: unknown, ctx: unknown) => Promise<unknown>) => {
        if (eventName === 'tool_call') toolCall = fn;
      },
    } as unknown as Parameters<typeof addon>[0]);

    assert.ok(toolCall, 'tool_call handler should be registered');
    const decision = await toolCall!(
      { toolName: 'edit', toolCallId: 'harness-gate-1', input: { path: `${root}/SKILL.md` } },
      { cwd: '/tmp' },
    );
    assert.ok(
      decision && (decision as { block?: boolean }).block === true,
      'editing a bundled skill file with OCTOCODE_ALLOW_HARNESS_APPLY unset must be blocked',
    );
    assert.match(
      String((decision as { reason?: string }).reason),
      /editing the skill itself is gated/,
    );
  } finally {
    if (previousAllow === undefined) delete process.env['OCTOCODE_ALLOW_HARNESS_APPLY'];
    else process.env['OCTOCODE_ALLOW_HARNESS_APPLY'] = previousAllow;
  }
});

test('resolvePromptMode: explicit option wins, then env, then append default', () => {
  const previous = process.env['OCTOCODE_PROMPT_MODE'];
  try {
    delete process.env['OCTOCODE_PROMPT_MODE'];
    assert.equal(resolvePromptMode(), 'append');
    assert.equal(resolvePromptMode('octocode-first'), 'octocode-first');
    assert.equal(resolvePromptMode('replace'), 'octocode-first', 'replace is a compatibility alias');
    assert.equal(resolvePromptMode('append'), 'append');

    process.env['OCTOCODE_PROMPT_MODE'] = 'octocode-first';
    assert.equal(resolvePromptMode(), 'octocode-first', 'env selects octocode-first when no option given');
    process.env['OCTOCODE_PROMPT_MODE'] = 'replace';
    assert.equal(resolvePromptMode(), 'octocode-first', 'legacy env replace aliases to octocode-first');
    assert.equal(resolvePromptMode('append'), 'append', 'explicit option overrides env');

    process.env['OCTOCODE_PROMPT_MODE'] = 'garbage';
    assert.equal(resolvePromptMode(), 'append', 'unknown env falls back to append');
  } finally {
    if (previous === undefined) delete process.env['OCTOCODE_PROMPT_MODE'];
    else process.env['OCTOCODE_PROMPT_MODE'] = previous;
  }
});

test('composeSystemPrompt: append keeps Pi prompt first, octocode-first leads with harness', () => {
  const appended = composeSystemPrompt({
    piSystemPrompt: 'PI_BASE',
    octocodePrompt: 'OCTO_HARNESS',
    promptMode: 'append',
  });
  assert.ok(appended.startsWith('PI_BASE'), 'append: Pi prompt leads');
  assert.ok(appended.includes('OCTO_HARNESS'), 'append: harness present');

  const octocodeFirst = composeSystemPrompt({
    piSystemPrompt: 'PI_BASE',
    octocodePrompt: 'OCTO_HARNESS',
    promptMode: 'octocode-first',
  });
  assert.ok(
    octocodeFirst.indexOf('OCTO_HARNESS') < octocodeFirst.indexOf('PI_BASE'),
    'octocode-first: harness leads',
  );
  assert.ok(octocodeFirst.includes('PI_BASE'), 'octocode-first: Pi prompt preserved, never dropped');

  const legacyReplace = composeSystemPrompt({
    piSystemPrompt: 'PI_BASE',
    octocodePrompt: 'OCTO_HARNESS',
    promptMode: 'replace',
  });
  assert.equal(legacyReplace, octocodeFirst, 'replace remains a compatibility alias');
});
