import assert from 'node:assert/strict';
import test from 'node:test';
import octocodeDefault, {
  createOctocodePiExtension,
  resolvePromptMode,
  composeSystemPrompt,
} from '../src/index.js';

test('default export preserves the single-arg Pi contract (default(pi))', () => {
  assert.equal(typeof octocodeDefault, 'function');
  assert.equal(octocodeDefault.length, 1, 'Pi calls default(pi) with exactly one arg');
});

test('createOctocodePiExtension returns a single-arg wiring function', () => {
  const wiring = createOctocodePiExtension({ promptMode: 'replace' });
  assert.equal(typeof wiring, 'function');
  assert.equal(wiring.length, 1);
});

test('resolvePromptMode: explicit option wins, then env, then append default', () => {
  const previous = process.env.OCTOCODE_PROMPT_MODE;
  try {
    delete process.env.OCTOCODE_PROMPT_MODE;
    assert.equal(resolvePromptMode(), 'append');
    assert.equal(resolvePromptMode('replace'), 'replace');
    assert.equal(resolvePromptMode('append'), 'append');

    process.env.OCTOCODE_PROMPT_MODE = 'replace';
    assert.equal(resolvePromptMode(), 'replace', 'env selects replace when no option given');
    assert.equal(resolvePromptMode('append'), 'append', 'explicit option overrides env');

    process.env.OCTOCODE_PROMPT_MODE = 'garbage';
    assert.equal(resolvePromptMode(), 'append', 'unknown env falls back to append');
  } finally {
    if (previous === undefined) delete process.env.OCTOCODE_PROMPT_MODE;
    else process.env.OCTOCODE_PROMPT_MODE = previous;
  }
});

test('composeSystemPrompt: append keeps Pi prompt first, replace leads with harness', () => {
  const appended = composeSystemPrompt({
    piSystemPrompt: 'PI_BASE',
    octocodePrompt: 'OCTO_HARNESS',
    promptMode: 'append',
  });
  assert.ok(appended.startsWith('PI_BASE'), 'append: Pi prompt leads');
  assert.ok(appended.includes('OCTO_HARNESS'), 'append: harness present');

  const replaced = composeSystemPrompt({
    piSystemPrompt: 'PI_BASE',
    octocodePrompt: 'OCTO_HARNESS',
    promptMode: 'replace',
  });
  assert.ok(replaced.indexOf('OCTO_HARNESS') < replaced.indexOf('PI_BASE'), 'replace: harness leads');
  assert.ok(replaced.includes('PI_BASE'), 'replace: Pi prompt preserved, never dropped');
});
