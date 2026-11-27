import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createRequire } from 'node:module';
import { chrome, call, closeChrome } from '../../dist/runtime.js';
import { chromeCommands } from '../../dist/engine/chrome-contract.mjs';

const root = join(import.meta.dirname, '../..');

test('every command is a native function, named exactly as in MCP and /cli', () => {
  assert.deepEqual(
    Object.keys(chrome).sort(),
    Object.keys(chromeCommands).sort()
  );
  for (const fn of Object.values(chrome)) assert.equal(typeof fn, 'function');
  assert.equal(typeof chrome.helper, 'function');
  assert.equal(typeof chrome.group, 'function');
  assert.equal(chrome['har-pager'], undefined);
});

test('a native call validates input with the command schema and runs the engine', async () => {
  const cwd = mkdtempSync(join(tmpdir(), 'octo-native-'));
  const previous = process.cwd();
  process.chdir(cwd);
  try {
    await assert.rejects(() => call('targets', { extra: true }));
    await assert.rejects(() => call('helper', { name: 'cookies' }));
    // A helper runs through the gateway: its usage is the reply.
    const usage = await chrome.helper({ name: 'prune', args: ['--help'] });
    assert.equal(usage.ok, true);
    const reply = await chrome.schema({});
    assert.equal(reply.ok, true);
  } finally {
    closeChrome();
    process.chdir(previous);
    rmSync(cwd, { recursive: true, force: true });
  }
});

test('the package exports MCP, native functions and every skill file', () => {
  const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
  assert.equal(pkg.exports['.'], './dist/mcp.js');
  assert.equal(pkg.exports['./runtime'], './dist/runtime.js');
  assert.equal(pkg.exports['./skills/*'], './skills/*');
  const resolved = createRequire(join(root, 'package.json')).resolve(
    '@octocodeai/octocode-chrome-devtools/skills/octocode-chrome-devtools/SKILL.md'
  );
  assert.ok(existsSync(resolved));
});
