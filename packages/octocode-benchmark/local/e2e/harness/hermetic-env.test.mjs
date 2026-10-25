import assert from 'node:assert/strict';
import fs from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import test from 'node:test';
import { hermeticEnv } from '../../../../../test-utils/hermetic-env.ts';

test('hermetic suites disable inherited LSP startup and installation', t => {
  t.mock.method(fs, 'mkdtempSync', () => '/mock/octocode-home');
  syncBuiltinESMExports();
  t.after(() => { t.mock.restoreAll(); syncBuiltinESMExports(); });
  const enabled = {
    OCTOCODE_LSP_PREWARM: 'all', OCTOCODE_LSP_AUTO_INSTALL: 'on',
    OCTOCODE_TRUST_PROJECT_LSP_CONFIG: 'true', OCTOCODE_LIVE: '1',
  };
  for (const base of [{}, enabled]) {
    const env = hermeticEnv(base);
    assert.equal(env.OCTOCODE_LSP_PREWARM, 'off');
    assert.equal(env.OCTOCODE_LSP_AUTO_INSTALL, 'off');
    assert.equal(env.OCTOCODE_TRUST_PROJECT_LSP_CONFIG, 'false');
    assert.equal(env.OCTOCODE_LIVE, undefined);
  }
  assert.equal(enabled.OCTOCODE_LSP_PREWARM, 'all', 'caller environment remains unchanged');
});
