import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test, vi } from 'vitest';
import { buildDefaultOctocodeMcpServer } from '../src/tools/mcp/config.js';
import { readOwnDependencyVersion, readOwnVersion } from '../src/package-metadata.js';

test('MCP client version uses this extension manifest', () => {
  const manifest = JSON.parse(fs.readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
  assert.equal(readOwnVersion(), manifest.version);
});

test('package metadata rejects malformed dependency values and normalizes declared strings', (context) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'pi-package-metadata-'));
  context.onTestFinished(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const [index, value] of [undefined, 42, '', '  19.1.0  '].entries()) {
    const packageDir = path.join(root, String(index));
    fs.mkdirSync(packageDir);
    fs.writeFileSync(path.join(packageDir, 'package.json'), JSON.stringify({ dependencies: { 'octocode-mcp': value } }));
    assert.equal(readOwnDependencyVersion('octocode-mcp', path.join(packageDir, 'dist')), index === 3 ? '19.1.0' : undefined);
  }
});

test('MCP fallback uses the version declared by this extension', () => {
  const manifest = JSON.parse(fs.readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
  const unavailable = vi.spyOn(fs, 'existsSync').mockReturnValue(false);
  try {
    const config = buildDefaultOctocodeMcpServer();
    assert.equal(config.command, 'npx');
    assert.deepEqual(config.args, ['-y', `octocode-mcp@${manifest.dependencies['octocode-mcp']}`]);
  } finally {
    unavailable.mockRestore();
  }
});
