import {test} from 'node:test';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {existsSync} from 'node:fs';
import {join} from 'node:path';
import {binary, root, tempWorkspace} from './helpers.mjs';

test('message hook previews report absent guard separately from consumable host configuration', t => {
  const workspace = tempWorkspace(t, 'guard-capabilities-'), database = join(workspace, 'db.sqlite');
  for (const vendor of ['grok', 'cursor']) {
    const result = spawnSync(binary, ['host-config', '--vendor', vendor, '--workspace', workspace, '--database', database], {encoding: 'utf8'});
    assert.equal(result.status, 0, result.stderr);
    const config = JSON.parse(result.stdout), guard = JSON.parse(result.stderr);
    assert.ok(config.hooks);
    assert.equal(config.leaseGuard, undefined, 'No unsupported keys leak into native host settings');
    assert.equal(guard.type, 'leaseGuard');
    assert.equal(guard.vendor, vendor);
    assert.equal(guard.configured, false);
    assert.equal(guard.advisory, true);
    assert.deepEqual(guard.supportedOperations, []);
    assert.match(guard.reason, /no bundled edit guard/);
  }
  const unsupported = spawnSync(binary, ['host-config', '--vendor', 'codex', '--workspace', workspace, '--database', database], {encoding: 'utf8'});
  assert.notEqual(unsupported.status, 0, 'Unsupported Codex guard setup must not appear installed');
  assert.equal(unsupported.stdout, '');
  assert.equal(existsSync(database), false);
});

test('Claude guard preview advertises only structured operations and does not claim installation', {skip: process.platform === 'win32'}, t => {
  const workspace = tempWorkspace(t, 'claude-capabilities-'), database = join(workspace, 'db.sqlite');
  const result = spawnSync(process.execPath, [join(root, 'scripts/hooks/claude-lease-guard.mjs'), '--config',
    '--binary', binary, '--workspace', workspace, '--database', database, '--session', 'db-id', '--host-session', 'host-id'], {encoding: 'utf8'});
  assert.equal(result.status, 0, result.stderr);
  const config = JSON.parse(result.stdout), guard = JSON.parse(result.stderr);
  assert.equal(config.hooks.PreToolUse[0].matcher, '^(Write|Edit)$');
  assert.deepEqual(guard.supportedOperations, ['Write', 'Edit']);
  assert.equal(guard.configured, false);
  assert.equal(guard.advisory, true);
  assert.match(guard.reason, /Preview only/);
  assert.equal(existsSync(database), false);
});
