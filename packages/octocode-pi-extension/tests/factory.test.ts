import assert from 'node:assert/strict';
import fs from 'node:fs';
import { test } from 'vitest';
import octocodeDefault, { createOctocodePiExtension } from '../src/index.js';
import * as runtimeEntrypoint from '../src/index.js';
import * as testingEntrypoint from './helpers/production-pi.js';
import { resolvePromptMode, composeSystemPrompt } from '../src/prompt.js';
import { DEFAULT_OCTOCODE_PROMPT_MODE } from '../src/contracts/protocols.js';

test('default export preserves the single-arg Pi contract (default(pi))', () => {
  assert.equal(typeof octocodeDefault, 'function');
  assert.equal(octocodeDefault.length, 1, 'Pi calls default(pi) with exactly one arg');
});

test('createOctocodePiExtension returns a single-arg wiring function', () => {
  const wiring = createOctocodePiExtension({ promptMode: 'octocode-first' });
  assert.equal(typeof wiring, 'function');
  assert.equal(wiring.length, 1);
});

test('production conformance helpers stay in the test corpus and are never published', () => {
  assert.equal('createProductionPiScenarioSuite' in runtimeEntrypoint, false);
  assert.equal('captureProductionPiLifecycle' in runtimeEntrypoint, false);
  // The probe corpus stays usable from tests/, just not from the package surface.
  assert.equal(typeof testingEntrypoint.createProductionPiScenarioSuite, 'function');
  assert.equal(typeof testingEntrypoint.captureProductionPiLifecycle, 'function');

  const manifest = JSON.parse(
    fs.readFileSync(new URL('../package.json', import.meta.url), 'utf8'),
  ) as { exports?: Record<string, unknown> };
  assert.equal(manifest.exports?.['./testing'], undefined, 'no published conformance subpath');

  // `dist` is built from src/** only, so keeping the probe out of src/ is what
  // keeps it out of the published tarball. Asserted on source (not on dist)
  // because the default build does not clean stale dist artifacts.
  const srcRoot = new URL('../src/', import.meta.url);
  assert.equal(fs.existsSync(new URL('testing.ts', srcRoot)), false);
  assert.equal(fs.existsSync(new URL('adapters/pi-production-probe.ts', srcRoot)), false);
});

test('resolvePromptMode: explicit option wins, then env, then append default', () => {
  const previous = process.env['OCTOCODE_PROMPT_MODE'];
  try {
    delete process.env['OCTOCODE_PROMPT_MODE'];
    assert.equal(resolvePromptMode(), DEFAULT_OCTOCODE_PROMPT_MODE);
    assert.equal(resolvePromptMode('octocode-first'), 'octocode-first');
    assert.equal(resolvePromptMode('append'), 'append');

    process.env['OCTOCODE_PROMPT_MODE'] = 'octocode-first';
    assert.equal(resolvePromptMode(), 'octocode-first', 'env selects octocode-first when no option given');
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
});
