import assert from 'node:assert/strict';
import { test } from 'node:test';
import { configDrift, ruleView } from './core-lint-sync.mjs';

const canonical = {
  $schema: './node_modules/oxlint/configuration_schema.json',
  plugins: ['typescript'],
  ignorePatterns: ['packages/octocode-native/target/**'],
  rules: { 'max-lines': ['error', { max: 400 }] },
  overrides: [
    { files: ['**/tests/**'], rules: { 'no-console': 'off' } },
    { files: ['packages/octocode-chrome-devtools/**'], rules: { 'typescript/no-useless-default-assignment': 'off' } },
  ],
};

test('layout keys and repo-scoped overrides are not rules', () => {
  const view = ruleView(canonical);
  assert.deepEqual(Object.keys(view).toSorted(), ['overrides', 'plugins', 'rules']);
  assert.deepEqual(view.overrides, [canonical.overrides[0]]);
});

test('a copy with its own layout matches; a changed rule, plugin or generic override drifts', () => {
  const copy = { ...canonical, $schema: '../../x.json', ignorePatterns: ['dist/**'], overrides: [canonical.overrides[0]] };
  assert.deepEqual(configDrift(canonical, copy), []);
  assert.deepEqual(configDrift(canonical, { ...copy, rules: { 'max-lines': ['error', { max: 500 }] } }), ['rules']);
  assert.deepEqual(configDrift(canonical, { ...copy, plugins: [], env: { node: true } }), ['env', 'plugins']);
  assert.deepEqual(configDrift(canonical, { ...copy, overrides: [] }), ['overrides']);
});
