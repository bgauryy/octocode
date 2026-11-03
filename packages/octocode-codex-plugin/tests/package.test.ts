import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { test } from 'node:test';
import { Ajv2020 } from 'ajv/dist/2020.js';
import { definePluginPackageTests } from '../../../test-utils/plugin-package-tests.ts';
import { readJson } from '../../octocode/scripts/build-plugin.mjs';
import { build, packageRoot, repoRoot } from '../scripts/build.ts';

definePluginPackageTests({
  packageRoot,
  repoRoot,
  build,
  manifest: 'plugin.json',
  mcp: 'mcp.json',
  marketplace: '.agents/plugins/marketplace.json',
  archiveRequired: ['plugin.json', 'mcp.json', 'assets/logo.png'],
});

test('manifests conform to upstream Agent Plugins schemas', () => {
  const ajv = new Ajv2020({ allErrors: true, strict: false });
  for (const name of ['plugin', 'mcp']) {
    const validate = ajv.compile(
      readJson(join(packageRoot, `tests/schemas/${name}.schema.json`))
    );
    const manifest: unknown = readJson(join(packageRoot, `${name}.json`));
    assert.ok(
      typeof manifest === 'object' && manifest !== null,
      `${name}.json is an object`
    );
    assert.ok(validate(manifest), ajv.errorsText(validate.errors));
    assert.equal(validate({ ...manifest, unrecognizedField: true }), false);
  }
});

test('the Codex extension points at shipped assets within the host limits', () => {
  const extension = readJson(join(packageRoot, 'plugin.json')).extensions[
    'com.openai'
  ];
  assert.ok(existsSync(resolve(packageRoot, extension.onboardingSkill)));
  for (const field of ['logo', 'composerIcon'])
    assert.ok(existsSync(resolve(packageRoot, extension.interface[field])));
  assert.ok(extension.interface.shortDescription.length <= 30);
  assert.ok(extension.interface.displayName.length <= 30);
});
