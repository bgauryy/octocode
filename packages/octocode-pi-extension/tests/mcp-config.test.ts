import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'vitest';
import { setMcpServerEnabled } from '../src/contracts/mcp-state.js';
import { openOctocodeDb } from '../src/tools/storage-policy.js';
import { extensionWorkspaceRoot } from '../src/extension-paths.js';
import { discoverMcpSystem } from '../src/tools/mcp/discovery.js';
import {
  buildServerHeaders,
  globalMcpConfigPaths,
  loadMcpConfig,
  projectMcpPath,
  projectMcpConfigPaths,
  reviewMcpSource,
} from '../src/tools/mcp/config.js';
import type { PiContext } from '../src/types.js';

function writeServer(filePath: string, name: string, command: string): void {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, JSON.stringify({ mcpServers: { [name]: { command } } }), 'utf8');
}

function writeServers(filePath: string, servers: Record<string, { command: string }>): void {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, JSON.stringify({ mcpServers: servers }), 'utf8');
}

test('MCP public native paths win after retained private and Pi resources', () => {
  const cwd = '/workspace/project';
  const homeDir = '/users/demo';
  const octocodeHome = '/custom/octocode-home';

  assert.deepEqual(globalMcpConfigPaths({ homeDir, octocodeHome }), [
    path.join(octocodeHome, 'agent', 'mcp', 'servers.json'),
    path.join(octocodeHome, 'extension', 'mcp', 'servers.json'),
    path.join(octocodeHome, 'mcp.json'),
  ]);
  assert.deepEqual(projectMcpConfigPaths(cwd, octocodeHome), [
    path.join(extensionWorkspaceRoot(cwd, octocodeHome), 'mcp', 'servers.json'),
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
  assert.deepEqual(loaded.sources.slice(1).map((source) => source.path), [globalPaths[0], projectPaths[0]]);
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

test.each(['cursor', 'claude'])('foreign MCP definitions are discovered read-only and disabled by default (%s)', async (host) => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-config-import-cwd-'));
  const homeDir = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-config-import-home-'));
  const cursorPath = path.join(cwd, `.${host}`, 'mcp.json');
  writeServer(cursorPath, 'docs', 'docs-mcp');

  const loaded = await loadMcpConfig(
    { cwd, isProjectTrusted: () => true } as unknown as PiContext,
    { homeDir, octocodeHome: path.join(homeDir, '.octocode-custom') },
  );

  assert.equal(loaded.configuredServers.get(`${host}.docs`)?.command, 'docs-mcp');
  assert.equal(loaded.configuredServers.get(`${host}.docs`)?.disabled, true);
  assert.equal(loaded.servers.has(`${host}.docs`), false, 'discovery never starts a server without an explicit override');
  assert.deepEqual(loaded.serverSources.get(`${host}.docs`), {
    scope: 'discovered-project', path: cursorPath, trusted: true, host, readOnly: true,
  });
});

test.each(['cursor', 'claude'])('a reviewed linked definition enables an import without copying it (%s)', async (host) => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-config-import-enable-'));
  const homeDir = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-config-import-enable-home-'));
  const octocodeHome = path.join(homeDir, '.octocode');
  const previousHome = process.env['OCTOCODE_HOME'];
  const previousMode = process.env['OCTOCODE_STORAGE_MODE'];
  process.env['OCTOCODE_HOME'] = octocodeHome;
  process.env['OCTOCODE_STORAGE_MODE'] = 'persistent';
  try {
    const cursorPath = path.join(cwd, `.${host}`, 'mcp.json');
    writeServer(cursorPath, 'docs', 'docs-mcp');
    setMcpServerEnabled(openOctocodeDb(), path.resolve(cwd), `${host}.docs`, true);
    const candidate = discoverMcpSystem(cwd, { homeDir, octocodeHome }).definitions.find(item => item.name === `${host}.docs`)!;
    reviewMcpSource(cwd, candidate.sourceId, candidate.revision, 'project', { homeDir, octocodeHome, trusted: true });

    const loaded = await loadMcpConfig(
      { cwd, isProjectTrusted: () => true } as unknown as PiContext,
      { homeDir, octocodeHome },
    );
    assert.equal(loaded.servers.get(`${host}.docs`)?.command, 'docs-mcp');
    assert.equal(fs.existsSync(projectMcpPath(cwd, octocodeHome)), false, 'definition was not duplicated');
  } finally {
    if (previousHome === undefined) delete process.env['OCTOCODE_HOME'];
    else process.env['OCTOCODE_HOME'] = previousHome;
    if (previousMode === undefined) delete process.env['OCTOCODE_STORAGE_MODE'];
    else process.env['OCTOCODE_STORAGE_MODE'] = previousMode;
  }
});

test.each(['cursor', 'pi'])('untrusted projects inventory but never import foreign project definitions (%s)', async (host) => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-config-import-untrusted-'));
  const homeDir = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-config-import-untrusted-home-'));
  const cursorPath = path.join(cwd, `.${host}`, 'mcp.json');
  writeServer(cursorPath, 'docs', 'docs-mcp');

  const loaded = await loadMcpConfig(
    { cwd, isProjectTrusted: () => false } as unknown as PiContext,
    { homeDir, octocodeHome: path.join(homeDir, '.octocode-custom') },
  );
  assert.equal(loaded.servers.has(`${host}.docs`), false);
  assert.equal(loaded.servers.has('docs'), false);
  if (host === 'cursor') assert.equal(loaded.configuredServers.get('cursor.docs')?.discovered?.reviewStatus, 'untrusted');
  assert.equal(loaded.sources.some((source) => source.path === cursorPath && !source.trusted), host !== 'pi');
});


test('active configuration rejects symlinked and oversized sources before parsing', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-active-admission-'));
  const cwd = path.join(root, 'workspace');
  const homeDir = path.join(root, 'home');
  const octocodeHome = path.join(homeDir, '.octocode');
  try {
    const global = globalMcpConfigPaths({ homeDir, octocodeHome })[0]!;
    const project = projectMcpConfigPaths(cwd, octocodeHome)[0]!;
    const outside = path.join(root, 'outside.json');
    writeServer(outside, 'escaped', 'must-not-run');
    fs.mkdirSync(path.dirname(global), { recursive: true });
    fs.symlinkSync(outside, global);
    writeServer(project, 'oversized', 'x'.repeat(1024 * 1024));
    const loaded = await loadMcpConfig(
      { cwd, isProjectTrusted: () => true } as unknown as PiContext,
      { homeDir, octocodeHome },
    );
    assert.equal(loaded.configuredServers.has('escaped'), false);
    assert.equal(loaded.configuredServers.has('oversized'), false);
    assert.ok(loaded.warnings.some((warning) => warning.includes(global) && warning.includes('non-symbolic-link')));
    assert.ok(loaded.warnings.some((warning) => warning.includes(project) && warning.includes('exceeds')));
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
