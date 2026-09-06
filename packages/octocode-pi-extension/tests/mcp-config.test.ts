import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'vitest';
import { setMcpServerEnabled } from '@octocodeai/octocode-awareness/mcp-state';
import { openOctocodeDb } from '../src/tools/storage-policy.js';
import {
  buildServerHeaders,
  globalMcpConfigPaths,
  loadMcpConfig,
  projectMcpPath,
  projectMcpConfigPaths,
} from '../src/tools/mcp-config.js';
import type { PiContext } from '../src/types.js';

function writeServer(filePath: string, name: string, command: string): void {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, JSON.stringify({ mcpServers: { [name]: { command } } }), 'utf8');
}

function writeServers(filePath: string, servers: Record<string, { command: string }>): void {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, JSON.stringify({ mcpServers: servers }), 'utf8');
}

test('MCP config has one canonical global and project location', () => {
  const cwd = '/workspace/project';
  const homeDir = '/users/demo';
  const octocodeHome = '/custom/octocode-home';

  assert.deepEqual(globalMcpConfigPaths({ homeDir, octocodeHome }), [
    path.join(octocodeHome, 'extension', 'mcp', 'servers.json'),
  ]);
  assert.deepEqual(projectMcpConfigPaths(cwd, octocodeHome), [
    projectMcpPath(cwd, octocodeHome),
  ]);
});

test('loadMcpConfig merges canonical global and project config deterministically', async () => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-config-cwd-'));
  const homeDir = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-config-home-'));
  const octocodeHome = path.join(homeDir, '.octocode-custom');
  const globalPaths = globalMcpConfigPaths({ homeDir, octocodeHome });
  const projectPaths = projectMcpConfigPaths(cwd, octocodeHome);

  writeServers(globalPaths[0]!, { shared: { command: 'global-command' }, globalOnly: { command: 'global-only' } });
  writeServers(projectPaths[0]!, { shared: { command: 'project-command' }, projectOnly: { command: 'project-only' } });

  const ctx = { cwd, isProjectTrusted: () => true } as unknown as PiContext;
  const loaded = await loadMcpConfig(ctx, { homeDir, octocodeHome });

  assert.equal(loaded.servers.get('shared')?.command, 'project-command');
  for (const name of ['globalOnly', 'projectOnly']) {
    assert.ok(loaded.servers.has(name), `${name} loaded`);
  }
  assert.deepEqual(loaded.sources.slice(1).map((source) => source.path), [...globalPaths, ...projectPaths]);
  assert.deepEqual(loaded.warnings, []);
});

test('HTTP MCP servers accept URL and headers without a command', async () => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-http-cwd-'));
  const homeDir = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-http-home-'));
  const octocodeHome = path.join(homeDir, '.octocode-custom');
  const [configPath] = globalMcpConfigPaths({ homeDir, octocodeHome });
  fs.mkdirSync(path.dirname(configPath!), { recursive: true });
  fs.writeFileSync(configPath!, JSON.stringify({ mcpServers: { remote: {
    url: 'https://mcp.example.test/api',
    headers: { Authorization: 'Bearer test' },
  } } }));
  const loaded = await loadMcpConfig({ cwd, isProjectTrusted: () => true } as unknown as PiContext, { homeDir, octocodeHome });
  assert.equal(loaded.servers.get('remote')?.transport, 'http');
  assert.equal(loaded.servers.get('remote')?.url, 'https://mcp.example.test/api');
});

test('HTTP bearer credentials are resolved from an environment reference only at connection time', () => {
  const previous = process.env['OCTOCODE_TEST_MCP_BEARER'];
  process.env['OCTOCODE_TEST_MCP_BEARER'] = 'secret-token';
  try {
    assert.deepEqual(buildServerHeaders({
      url: 'https://mcp.example.test',
      bearerTokenEnvVar: 'OCTOCODE_TEST_MCP_BEARER',
    }), { Authorization: 'Bearer secret-token' });
  } finally {
    if (previous === undefined) delete process.env['OCTOCODE_TEST_MCP_BEARER'];
    else process.env['OCTOCODE_TEST_MCP_BEARER'] = previous;
  }
});

test('untrusted projects skip every project alias but still load global aliases', async () => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-config-untrusted-cwd-'));
  const homeDir = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-config-untrusted-home-'));
  const octocodeHome = path.join(homeDir, '.octocode-custom');
  const globalPaths = globalMcpConfigPaths({ homeDir, octocodeHome });
  const projectPaths = projectMcpConfigPaths(cwd, octocodeHome);

  writeServer(globalPaths[0]!, 'globalOnly', 'global-command');
  for (const [index, filePath] of projectPaths.entries()) writeServer(filePath, `project${index}`, `project-command-${index}`);

  const ctx = { cwd, isProjectTrusted: () => false } as unknown as PiContext;
  const loaded = await loadMcpConfig(ctx, { homeDir, octocodeHome });

  assert.equal(loaded.servers.get('globalOnly')?.command, 'global-command');
  for (const index of projectPaths.keys()) assert.equal(loaded.servers.has(`project${index}`), false);
  assert.deepEqual(
    loaded.sources.filter((source) => source.scope === 'project').map((source) => ({ path: source.path, trusted: source.trusted })),
    projectPaths.map((filePath) => ({ path: filePath, trusted: false })),
  );
  assert.equal(loaded.warnings.filter((warning) => warning.includes('project is not trusted')).length, projectPaths.length);
});

test('foreign MCP definitions are discovered read-only and disabled by default', async () => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-config-import-cwd-'));
  const homeDir = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-config-import-home-'));
  const cursorPath = path.join(cwd, '.cursor', 'mcp.json');
  writeServer(cursorPath, 'docs', 'docs-mcp');

  const loaded = await loadMcpConfig(
    { cwd, isProjectTrusted: () => true } as unknown as PiContext,
    { homeDir, octocodeHome: path.join(homeDir, '.octocode-custom') },
  );

  assert.equal(loaded.configuredServers.get('cursor.docs')?.command, 'docs-mcp');
  assert.equal(loaded.configuredServers.get('cursor.docs')?.disabled, true);
  assert.equal(loaded.servers.has('cursor.docs'), false, 'discovery never starts a server without an explicit override');
  assert.deepEqual(loaded.serverSources.get('cursor.docs'), {
    scope: 'discovered-project', path: cursorPath, trusted: true, host: 'cursor', readOnly: true,
  });
});

test('an explicit SQLite override enables a discovered definition without copying it', async () => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-config-import-enable-'));
  const homeDir = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-config-import-enable-home-'));
  const octocodeHome = path.join(homeDir, '.octocode');
  const previousHome = process.env['OCTOCODE_HOME'];
  const previousMode = process.env['OCTOCODE_STORAGE_MODE'];
  process.env['OCTOCODE_HOME'] = octocodeHome;
  process.env['OCTOCODE_STORAGE_MODE'] = 'persistent';
  try {
    const cursorPath = path.join(cwd, '.cursor', 'mcp.json');
    writeServer(cursorPath, 'docs', 'docs-mcp');
    setMcpServerEnabled(openOctocodeDb(), path.resolve(cwd), 'cursor.docs', true);

    const loaded = await loadMcpConfig(
      { cwd, isProjectTrusted: () => true } as unknown as PiContext,
      { homeDir, octocodeHome },
    );
    assert.equal(loaded.servers.get('cursor.docs')?.command, 'docs-mcp');
    assert.equal(fs.existsSync(projectMcpPath(cwd, octocodeHome)), false, 'definition was not duplicated');
  } finally {
    if (previousHome === undefined) delete process.env['OCTOCODE_HOME'];
    else process.env['OCTOCODE_HOME'] = previousHome;
    if (previousMode === undefined) delete process.env['OCTOCODE_STORAGE_MODE'];
    else process.env['OCTOCODE_STORAGE_MODE'] = previousMode;
  }
});

test('untrusted projects inventory but never import foreign project definitions', async () => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-config-import-untrusted-'));
  const homeDir = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-config-import-untrusted-home-'));
  const cursorPath = path.join(cwd, '.cursor', 'mcp.json');
  writeServer(cursorPath, 'docs', 'docs-mcp');

  const loaded = await loadMcpConfig(
    { cwd, isProjectTrusted: () => false } as unknown as PiContext,
    { homeDir, octocodeHome: path.join(homeDir, '.octocode-custom') },
  );
  assert.equal(loaded.configuredServers.has('cursor.docs'), false);
  assert.equal(loaded.sources.some((source) => source.path === cursorPath && !source.trusted), true);
});
