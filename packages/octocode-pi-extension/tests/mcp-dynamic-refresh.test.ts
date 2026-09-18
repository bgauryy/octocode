import fs from 'node:fs';
import { expect, test, vi } from 'vitest';
import { createDelayedMcpFixture } from './helpers/mcp-fixture.js';
import { projectMcpPath } from '../src/tools/mcp/config.js';
import { getCachedMcpCatalogAddendum, getEffectiveMcpSnapshot, refreshMcpCapabilities, stopAllMcpServers, waitForMcpShutdown, warmMcpCatalog } from '../src/tools/mcp-tool.js';

test('configuration changes during discovery remove stale tools and adopt re-enabled sources without a new session', async () => {
  vi.stubEnv('OCTOCODE_HOME', process.env['OCTOCODE_HOME'] ?? '');
  const fixture = createDelayedMcpFixture(250);
  try {
    const warm = warmMcpCatalog(fixture.ctx);
    await vi.waitFor(() => expect(fs.existsSync(fixture.discoveryStartedMarker)).toBe(true));
    const configPath = projectMcpPath(fixture.ctx.cwd!);
    const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
    config.mcpServers.octocode.disabled = true;
    fs.writeFileSync(configPath, JSON.stringify(config));
    await refreshMcpCapabilities(fixture.ctx);
    await warm;
    expect(getEffectiveMcpSnapshot(fixture.ctx)?.servers.some(server => server.name === 'octocode')).toBe(false);
    expect(getCachedMcpCatalogAddendum(fixture.ctx)).not.toContain('mockTool');
    fs.writeFileSync(fixture.serverPath, fs.readFileSync(fixture.serverPath, 'utf8').replace('Mocked cache-flow tool', 'Updated live configuration tool'));
    config.mcpServers.octocode.disabled = false;
    fs.writeFileSync(configPath, JSON.stringify(config));
    await refreshMcpCapabilities(fixture.ctx);
    expect(getCachedMcpCatalogAddendum(fixture.ctx)).toContain('Updated live configuration tool');
    const stable = getCachedMcpCatalogAddendum(fixture.ctx);
    await refreshMcpCapabilities(fixture.ctx);
    expect(getCachedMcpCatalogAddendum(fixture.ctx)).toBe(stable);
  } finally {
    stopAllMcpServers();
    await waitForMcpShutdown();
    fixture.cleanup();
    vi.unstubAllEnvs();
  }
});
