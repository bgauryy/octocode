import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { test } from 'vitest';

test('built config consumers use the packaged implementation without forwarding modules', async () => {
  const dist = path.resolve(import.meta.dirname, '../dist');
  for (const relative of ['index.js', 'extension-paths.js', 'tools/storage-policy.js']) {
    const source = fs.readFileSync(path.join(dist, relative), 'utf8');
    assert.equal(/from\s+['"]@octocodeai\/config['"]/.test(source), false, `${relative} needs no separately installed config package`);
    assert.ok(/from\s+['"]\.{1,2}\/config\.js['"]/.test(source), `${relative} uses the packaged implementation`);
  }
  assert.equal(fs.existsSync(path.join(dist, 'env.js')), false);
  assert.equal(fs.existsSync(path.join(dist, 'prompts/subagent-shared.js')), false);
  assert.equal(fs.existsSync(path.join(dist, 'octocode/index.js')), false);
  const manifest = JSON.parse(fs.readFileSync(path.resolve(dist, '../package.json'), 'utf8'));
  assert.deepEqual(manifest.pi.extensions, ['./dist/index.js']);
  const config = await import(pathToFileURL(path.join(dist, 'config.js')).href);
  const configuredHome = path.join(dist, 'config-fixture-home');
  assert.equal(config.getOctocodeHome({ OCTOCODE_HOME: configuredHome }), configuredHome);
});
