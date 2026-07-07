import assert from 'node:assert/strict';
import { test } from 'vitest';
import octocodeDefault, {
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
    'before_agent_start',
    'agent_end',
    'session_before_compact',
    'session_shutdown',
  ]);
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
