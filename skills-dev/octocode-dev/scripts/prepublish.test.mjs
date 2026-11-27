import assert from 'node:assert/strict';
import { test } from 'node:test';
import { unpublishedWorkspaceDependencies } from './prepublish.mjs';

const manifests = new Map([
  ['octocode-mcp', { name: 'octocode-mcp', version: '20.2.0' }],
  ['@octocodeai/config', { name: '@octocodeai/config', version: '1.0.0', private: true }],
]);
const extension = {
  dependencies: { 'octocode-mcp': '^20.2.0', '@octocodeai/config': '*', zod: '^4' },
};

test('a public workspace dependency missing from npm at its workspace version blocks the publish', () => {
  const asked = [];
  const view = spec => (asked.push(spec), spec === 'octocode-mcp@20.2.0' ? null : '9.9.9');
  assert.deepEqual(unpublishedWorkspaceDependencies(extension, manifests, view), ['octocode-mcp@20.2.0']);
  // Private workspace packages and registry-only dependencies are never asked about.
  assert.deepEqual(asked, ['octocode-mcp@20.2.0']);
});

test('the publish passes once npm serves the exact workspace version', () => {
  const view = spec => (spec === 'octocode-mcp@20.2.0' ? '20.2.0' : null);
  assert.deepEqual(unpublishedWorkspaceDependencies(extension, manifests, view), []);
});
